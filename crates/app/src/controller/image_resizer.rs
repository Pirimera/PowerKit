//! Image Resizer jobs: the preview that must precede every write, and the
//! write itself.
//!
//! A job runs on a worker thread. It reads and writes images only through
//! [`ImageCodec`], reports progress into a [`Shared`] record the dialog reads
//! back, and wakes the dialog after each report. A job never writes without a
//! complete preview: [`ready_only`] refuses a plan that is empty or holds a
//! failed item. Each job carries the dialog's generation, so a job the user
//! has moved on from cannot overwrite newer status.
use crate::fault;
use crate::ports::ImageCodec;
use powerkit_core::image_resizer::{PlanFault, PlanItem, PlannedWrite, ResizeSettings, plan_batch};
use powerkit_core::locale::Strings;
use std::{
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// What the worker last reported, shared with the dialog.
pub struct Shared {
    /// The dialog's current generation; a job of another generation is stale.
    pub generation: u64,
    pub status: String,
    /// One line per file, once a preview finished.
    pub lines: Option<Vec<String>>,
    pub plan: Option<Vec<PlanItem>>,
    /// The job has finished, successfully or not.
    pub stop: bool,
}
impl Shared {
    pub fn clone_update(&self) -> Update {
        Update {
            generation: self.generation,
            status: self.status.clone(),
            lines: self.lines.clone(),
            plan: self.plan.clone(),
            stop: self.stop,
        }
    }
}

/// A snapshot of [`Shared`] the dialog applies on its own thread.
pub struct Update {
    pub generation: u64,
    pub status: String,
    pub lines: Option<Vec<String>>,
    pub plan: Option<Vec<PlanItem>>,
    pub stop: bool,
}

/// One preview or write over a set of files.
pub struct Job {
    pub files: Vec<PathBuf>,
    pub settings: ResizeSettings,
    /// The previewed plan; a write uses it and never plans again.
    pub plan: Vec<PlanItem>,
    pub shared: Arc<Mutex<Shared>>,
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    /// `false` previews; `true` writes the previewed plan.
    pub write: bool,
    pub strings: &'static Strings,
}

/// Runs `job` to its end on the calling thread. `open` creates the codec on
/// that thread; `wake` tells the dialog that [`Shared`] changed and is always
/// called once at the end. A failure, including a panic inside the codec, ends
/// the job with a status the user can read; nothing is left "busy".
pub fn run<C: ImageCodec>(open: impl FnOnce() -> io::Result<C>, job: &Job, wake: &dyn Fn()) {
    let t = job.strings;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let codec = open()?;
        if job.write {
            write_files(&codec, job, wake)
        } else {
            preview_files(&codec, job)
        }
    }));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => publish(job, &fault::describe(&error, t), None, None, true),
        Err(_) => publish(job, t.resize_stopped, None, None, true),
    }
    wake();
}

fn preview_files(codec: &impl ImageCodec, job: &Job) -> io::Result<()> {
    if job.cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    let probed = job
        .files
        .iter()
        .map(|path| codec.probe(path).ok())
        .collect::<Vec<_>>();
    let t = job.strings;
    let plan = plan_batch(
        &job.files,
        &job.settings,
        &probed,
        |path| path.exists(),
        |key| t.resize_size_name(key),
    );
    let lines = plan
        .iter()
        .map(|item| preview_line(item, t))
        .collect::<Vec<_>>();
    let failed = plan
        .iter()
        .any(|item| matches!(item, PlanItem::Failed { .. }));
    let status = if plan.is_empty() {
        t.resize_missing.into()
    } else if failed {
        t.resize_preview_has_errors.into()
    } else {
        t.resize_files_ready.replace("{n}", &plan.len().to_string())
    };
    publish(job, &status, Some(lines), Some(plan), true);
    Ok(())
}

fn write_files(codec: &impl ImageCodec, job: &Job, wake: &dyn Fn()) -> io::Result<()> {
    let t = job.strings;
    let Some(ready) = ready_only(&job.plan) else {
        publish(job, t.resize_need_preview, None, None, true);
        return Ok(());
    };
    for (index, plan) in ready.iter().enumerate() {
        if job.cancel.load(Ordering::Relaxed) {
            publish(job, t.resize_cancelled_left, None, None, true);
            return Ok(());
        }
        publish(
            job,
            &t.resize_writing_progress
                .replace("{current}", &(index + 1).to_string())
                .replace("{total}", &ready.len().to_string()),
            None,
            None,
            false,
        );
        wake();
        if let Err(error) = codec.write(plan, &job.settings) {
            publish(
                job,
                &t.resize_write_failed
                    .replace("{name}", &file_name(&plan.source))
                    .replace("{error}", &fault::describe(&error, t)),
                None,
                None,
                true,
            );
            return Ok(());
        }
    }
    publish(job, t.power_store_succeeded, None, None, true);
    Ok(())
}

/// Records progress unless the dialog has moved on to another generation.
fn publish(
    job: &Job,
    status: &str,
    lines: Option<Vec<String>>,
    plan: Option<Vec<PlanItem>>,
    done: bool,
) {
    let Ok(mut shared) = job.shared.lock() else {
        return;
    };
    if shared.generation != job.generation {
        return;
    }
    shared.status = status.into();
    if lines.is_some() {
        shared.lines = lines;
    }
    if plan.is_some() {
        shared.plan = plan;
    }
    shared.stop = done;
}

/// The writes of a complete preview: `None` when the plan is empty or any
/// item failed, so nothing is ever written from a partial preview.
pub fn ready_only(items: &[PlanItem]) -> Option<Vec<PlannedWrite>> {
    if items.is_empty()
        || items
            .iter()
            .any(|item| matches!(item, PlanItem::Failed { .. }))
    {
        return None;
    }
    Some(
        items
            .iter()
            .filter_map(|item| match item {
                PlanItem::Ready(plan) => Some(plan.clone()),
                PlanItem::Failed { .. } => None,
            })
            .collect(),
    )
}

fn preview_line(item: &PlanItem, t: &Strings) -> String {
    match item {
        PlanItem::Failed { source, fault } => {
            format!("{} — {}", file_name(source), fault_text(*fault, t))
        }
        PlanItem::Ready(plan) => {
            let destination = if plan.replace {
                t.resize_original.into()
            } else {
                file_name(&plan.destination)
            };
            let size = if plan.frame.unchanged {
                t.resize_unchanged
                    .replace("{w}", &plan.source_width.to_string())
                    .replace("{h}", &plan.source_height.to_string())
            } else {
                format!(
                    "{}×{} → {}×{}",
                    plan.source_width,
                    plan.source_height,
                    plan.frame.output_width,
                    plan.frame.output_height
                )
            };
            format!("{} → {destination} · {size}", file_name(&plan.source))
        }
    }
}

fn fault_text(fault: PlanFault, t: &Strings) -> &'static str {
    match fault {
        PlanFault::Unreadable => t.resize_fault_unreadable,
        PlanFault::Dimension => t.resize_fault_dimension,
        PlanFault::Name => t.resize_fault_name,
        PlanFault::TooMany => t.resize_fault_too_many,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// A custom width or height as typed: empty means "any", a comma is accepted
/// as the decimal separator. `None` for text that is not a usable size.
pub fn parse_extent(value: &str) -> Option<f64> {
    let value = value.trim().replace(',', ".");
    if value.is_empty() {
        return Some(0.0);
    }
    let parsed = value.parse::<f64>().ok()?;
    (parsed.is_finite() && (0.0..=10_000_000.0).contains(&parsed)).then_some(parsed)
}

/// A size value as the dialog shows it, without a trailing `.0`.
pub fn quantity_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use powerkit_core::image_resizer::{Container, SourceImage};
    use powerkit_core::locale::UiLanguage;
    use std::{cell::RefCell, collections::HashMap};

    /// A codec over synthetic images that never touches a file.
    #[derive(Default)]
    struct FakeCodec {
        images: HashMap<PathBuf, SourceImage>,
        written: RefCell<Vec<PathBuf>>,
        fail_write: Option<PathBuf>,
        panic_on_probe: bool,
        /// Set after this many writes, as a user pressing Cancel would.
        cancel_after: Option<(usize, Arc<AtomicBool>)>,
    }
    impl FakeCodec {
        fn with(names: &[&str]) -> Self {
            Self {
                images: names.iter().map(|name| (photo(name), image())).collect(),
                ..Self::default()
            }
        }
    }
    impl ImageCodec for FakeCodec {
        fn probe(&self, path: &Path) -> io::Result<SourceImage> {
            if self.panic_on_probe {
                panic!("codec crashed");
            }
            self.images
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::other("not an image"))
        }
        fn write(&self, plan: &PlannedWrite, _: &ResizeSettings) -> io::Result<()> {
            if self.fail_write.as_ref() == Some(&plan.source) {
                return Err(fault::Fault::SourceChanged.into());
            }
            self.written.borrow_mut().push(plan.source.clone());
            if let Some((count, cancel)) = &self.cancel_after
                && self.written.borrow().len() == *count
            {
                cancel.store(true, Ordering::Relaxed);
            }
            Ok(())
        }
    }

    fn strings() -> &'static Strings {
        UiLanguage::English.strings()
    }
    /// A path in a folder that does not exist, so no destination is ever taken.
    fn photo(name: &str) -> PathBuf {
        Path::new("powerkit-synthetic-photos").join(name)
    }
    fn image() -> SourceImage {
        SourceImage {
            width: 4000,
            height: 3000,
            dpi_x: 96.0,
            dpi_y: 96.0,
            container: Container::Jpeg,
        }
    }
    fn job(names: &[&str], write: bool, plan: Vec<PlanItem>) -> Job {
        Job {
            files: names.iter().map(|name| photo(name)).collect(),
            settings: ResizeSettings::default(),
            plan,
            shared: Arc::new(Mutex::new(Shared {
                generation: 7,
                status: String::new(),
                lines: None,
                plan: None,
                stop: false,
            })),
            generation: 7,
            cancel: Arc::new(AtomicBool::new(false)),
            write,
            strings: strings(),
        }
    }
    /// Runs the job and returns what the dialog would read, with the wake count.
    fn ran(codec: FakeCodec, job: &Job) -> (Update, usize, FakeCodec) {
        let wakes = RefCell::new(0);
        let codec = RefCell::new(Some(codec));
        let kept = RefCell::new(None);
        run(
            || {
                let codec = codec.borrow_mut().take().unwrap();
                Ok(Recording(codec, &kept))
            },
            job,
            &|| *wakes.borrow_mut() += 1,
        );
        let update = job.shared.lock().unwrap().clone_update();
        let codec = kept.borrow_mut().take().unwrap();
        (update, wakes.into_inner(), codec)
    }
    /// Hands the codec back to the test when the job drops it.
    struct Recording<'a>(FakeCodec, &'a RefCell<Option<FakeCodec>>);
    impl ImageCodec for Recording<'_> {
        fn probe(&self, path: &Path) -> io::Result<SourceImage> {
            self.0.probe(path)
        }
        fn write(&self, plan: &PlannedWrite, settings: &ResizeSettings) -> io::Result<()> {
            self.0.write(plan, settings)
        }
    }
    impl Drop for Recording<'_> {
        fn drop(&mut self) {
            *self.1.borrow_mut() = Some(std::mem::take(&mut self.0));
        }
    }
    fn previewed(names: &[&str]) -> Vec<PlanItem> {
        let (update, _, _) = ran(FakeCodec::with(names), &job(names, false, Vec::new()));
        update.plan.unwrap()
    }

    #[test]
    fn a_preview_plans_every_file_and_reports_one_line_each() {
        let names = ["a.jpg", "b.jpg"];
        let (update, wakes, codec) = ran(FakeCodec::with(&names), &job(&names, false, Vec::new()));
        let t = strings();
        assert!(update.stop);
        assert_eq!(update.status, t.resize_files_ready.replace("{n}", "2"));
        let lines = update.lines.unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("a.jpg → ") && lines[0].contains("4000×3000 → "));
        let plan = update.plan.unwrap();
        assert_eq!(ready_only(&plan).unwrap().len(), 2);
        // A preview writes nothing and wakes the dialog once.
        assert!(codec.written.borrow().is_empty());
        assert_eq!(wakes, 1);
    }

    #[test]
    fn an_unreadable_file_fails_the_preview_and_blocks_writing() {
        let (update, _, _) = ran(
            FakeCodec::with(&["a.jpg"]),
            &job(&["a.jpg", "notes.txt"], false, Vec::new()),
        );
        let t = strings();
        assert_eq!(update.status, t.resize_preview_has_errors);
        let lines = update.lines.unwrap();
        assert_eq!(
            lines[1],
            format!("notes.txt — {}", t.resize_fault_unreadable)
        );
        let plan = update.plan.unwrap();
        assert!(ready_only(&plan).is_none());
        // Writing that plan is refused without touching a file.
        let (update, _, codec) = ran(FakeCodec::default(), &job(&[], true, plan));
        assert_eq!(update.status, t.resize_need_preview);
        assert!(update.stop && codec.written.borrow().is_empty());
    }

    #[test]
    fn no_files_and_no_preview_are_both_reported_not_written() {
        let t = strings();
        let (update, _, _) = ran(FakeCodec::default(), &job(&[], false, Vec::new()));
        assert_eq!(update.status, t.resize_missing);
        assert!(ready_only(&[]).is_none());
        let (update, _, codec) = ran(FakeCodec::default(), &job(&[], true, Vec::new()));
        assert_eq!(update.status, t.resize_need_preview);
        assert!(codec.written.borrow().is_empty());
    }

    #[test]
    fn a_write_follows_the_preview_in_order_and_wakes_for_each_file() {
        let names = ["a.jpg", "b.jpg", "c.jpg"];
        let plan = previewed(&names);
        let (update, wakes, codec) = ran(FakeCodec::with(&names), &job(&names, true, plan));
        assert!(update.stop);
        assert_eq!(update.status, strings().power_store_succeeded);
        assert_eq!(
            *codec.written.borrow(),
            names.iter().map(|name| photo(name)).collect::<Vec<_>>()
        );
        // One wake per file for its progress line, and the final one.
        assert_eq!(wakes, 4);
    }

    #[test]
    fn a_failed_write_stops_the_batch_and_names_the_file() {
        let names = ["a.jpg", "b.jpg", "c.jpg"];
        let plan = previewed(&names);
        let codec = FakeCodec {
            fail_write: Some(photo("b.jpg")),
            ..FakeCodec::with(&names)
        };
        let (update, _, codec) = ran(codec, &job(&names, true, plan));
        let t = strings();
        assert!(update.stop);
        assert_eq!(
            update.status,
            t.resize_write_failed
                .replace("{name}", "b.jpg")
                .replace("{error}", t.faults.source_changed)
        );
        // The file before it was written; the one after it was not.
        assert_eq!(*codec.written.borrow(), [photo("a.jpg")]);
    }

    #[test]
    fn cancelling_stops_before_the_next_file() {
        let names = ["a.jpg", "b.jpg", "c.jpg"];
        let plan = previewed(&names);
        let job = job(&names, true, plan);
        let codec = FakeCodec {
            cancel_after: Some((1, job.cancel.clone())),
            ..FakeCodec::with(&names)
        };
        let (update, _, codec) = ran(codec, &job);
        assert!(update.stop);
        assert_eq!(update.status, strings().resize_cancelled_left);
        assert_eq!(codec.written.borrow().len(), 1);
        // A preview that was cancelled before it began reports nothing.
        let cancelled = super::tests::job(&names, false, Vec::new());
        cancelled.cancel.store(true, Ordering::Relaxed);
        let (update, wakes, _) = ran(FakeCodec::with(&names), &cancelled);
        assert!(update.plan.is_none() && !update.stop);
        assert_eq!(wakes, 1);
    }

    #[test]
    fn a_job_the_dialog_moved_on_from_reports_nothing() {
        let names = ["a.jpg"];
        let stale = job(&names, false, Vec::new());
        stale.shared.lock().unwrap().generation = 8;
        let (update, wakes, _) = ran(FakeCodec::with(&names), &stale);
        assert_eq!(update.generation, 8);
        assert!(update.status.is_empty() && update.plan.is_none() && !update.stop);
        assert_eq!(wakes, 1);
    }

    #[test]
    fn a_codec_that_cannot_start_or_crashes_ends_the_job_with_a_status() {
        let names = ["a.jpg"];
        let unavailable = job(&names, false, Vec::new());
        let wakes = RefCell::new(0);
        run(
            || -> io::Result<FakeCodec> { Err(fault::Fault::ImagingUnavailable.into()) },
            &unavailable,
            &|| *wakes.borrow_mut() += 1,
        );
        let update = unavailable.shared.lock().unwrap().clone_update();
        assert!(update.stop);
        assert_eq!(update.status, strings().faults.imaging_unavailable);
        assert_eq!(wakes.into_inner(), 1);

        let crashing = FakeCodec {
            panic_on_probe: true,
            ..FakeCodec::default()
        };
        let (update, wakes, _) = ran(crashing, &job(&names, false, Vec::new()));
        assert!(update.stop);
        assert_eq!(update.status, strings().resize_stopped);
        assert_eq!(wakes, 1);
    }

    #[test]
    fn custom_sizes_parse_leniently_and_print_without_a_trailing_zero() {
        assert_eq!(parse_extent(""), Some(0.0));
        assert_eq!(parse_extent(" 1920 "), Some(1920.0));
        assert_eq!(parse_extent("12,5"), Some(12.5));
        assert_eq!(parse_extent("-1"), None);
        assert_eq!(parse_extent("abc"), None);
        assert_eq!(parse_extent("inf"), None);
        assert_eq!(parse_extent("10000001"), None);
        assert_eq!(quantity_text(1920.0), "1920");
        assert_eq!(quantity_text(12.5), "12.5");
        assert_eq!(parse_extent(&quantity_text(12.5)), Some(12.5));
    }
}
