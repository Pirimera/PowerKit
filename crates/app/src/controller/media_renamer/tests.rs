//! The session over an in-memory folder: nothing here touches a real file, so
//! these run the same on every platform. The Windows crate runs the session
//! again over real files.
use super::*;
use crate::testkit::FakeMedia;
use std::{
    sync::atomic::AtomicUsize,
    time::{Duration, Instant},
};

const SETTLE: Duration = Duration::from_secs(20);

struct Bench {
    media: Arc<FakeMedia>,
    session: Session,
    prefs: Preferences,
    wakes: Arc<AtomicUsize>,
    dir: PathBuf,
}
impl Bench {
    fn new() -> Self {
        let media = Arc::new(FakeMedia::default());
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let prefs = Preferences::default();
        let session = Session::new(
            media.clone(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            }),
            &prefs,
        );
        Self {
            media,
            session,
            prefs,
            wakes,
            dir: PathBuf::from("fake-media"),
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
    /// A photo taken on 2024-05-`day` at 10:00:00.
    fn photo(&self, name: &str, day: u8) {
        self.media
            .add(&self.path(name), Some((2024, 5, day, 10, 0, 0)));
    }
    /// Polls like the main thread until no scan or work job remains.
    fn settle(&mut self) {
        let start = Instant::now();
        while self.session.working() {
            assert!(start.elapsed() < SETTLE, "session did not settle");
            self.session.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn view(&self) -> Snapshot {
        self.session.snapshot(&self.prefs)
    }
    fn names(&self) -> Vec<String> {
        self.media.names(&self.dir)
    }
    fn group_of(&self, name: &str) -> usize {
        self.view()
            .rows
            .iter()
            .find(|row| row.current == name)
            .unwrap()
            .group
    }
}

#[test]
fn a_folder_is_previewed_applied_with_its_sidecar_and_undone() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.photo("b.jpg", 2);
    bench.media.add(&bench.path("a.xmp"), None);
    bench.media.add(&bench.path("notes.txt"), None);
    let original = bench.names();

    bench.session.add(vec![bench.dir.clone()]);
    assert!(bench.session.working());
    bench.settle();
    let view = bench.view();
    assert_eq!(view.summary.groups, 2);
    assert_eq!(view.summary.ready, 2);
    assert_eq!(view.skipped, 1, "the text file is not media");
    assert!(view.can_apply);
    assert_eq!(view.busy, Busy::Idle);
    assert!(bench.session.has_unapplied());
    assert_eq!(bench.session.tray_status(), TrayStatus::Ready(2));
    // Nothing is renamed by a preview.
    assert_eq!(bench.names(), original);
    assert!(bench.wakes.load(Ordering::Relaxed) > 0);

    bench.session.apply();
    bench.settle();
    assert_eq!(
        bench.names(),
        [
            "2024-05-01_10-00-00.jpg",
            "2024-05-01_10-00-00.xmp",
            "2024-05-02_10-00-00.jpg",
            "notes.txt",
        ]
    );
    let view = bench.view();
    assert!(matches!(
        view.notice,
        Some(Notice::Applied { renamed: 3, ref failures, cancelled: false }) if failures.is_empty()
    ));
    // The workspace follows the files to their new names.
    assert_eq!(view.summary.ready, 0);
    assert!(
        view.rows
            .iter()
            .any(|row| row.current == "2024-05-01_10-00-00.jpg")
    );
    assert!(!bench.session.has_unapplied());
    assert_eq!(bench.session.tray_status(), TrayStatus::Idle);
    assert_eq!(bench.session.history().len(), 1);
    let id = bench.session.history()[0].id;
    assert!(bench.session.history()[0].undoable());

    bench.session.undo(id);
    bench.settle();
    assert_eq!(bench.names(), original);
    assert!(matches!(
        bench.view().notice,
        Some(Notice::Undone { restored: 3, skipped: 0, ref failures }) if failures.is_empty()
    ));
    // The undo is its own batch with a newer id; nothing is undoable any more.
    let history = bench.session.history();
    assert_eq!(history.len(), 2);
    assert!(history[0].id > history[1].id);
    assert!(!history.iter().any(Batch::undoable));
}

#[test]
fn a_file_changed_since_the_preview_blocks_the_rename_until_refreshed() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.photo("b.jpg", 2);
    bench.session.add(vec![bench.dir.clone()]);
    bench.settle();
    let original = bench.names();
    bench.media.touch(&bench.path("a.jpg"));
    bench.media.remove(&bench.path("b.jpg"));

    bench.session.apply();
    bench.settle();
    assert_eq!(bench.view().notice, Some(Notice::Stale));
    assert_eq!(bench.names(), ["a.jpg"]);
    assert_ne!(bench.names(), original);
    assert!(bench.session.history().is_empty());

    bench.session.act(Action::Refresh);
    bench.settle();
    // The missing file left the workspace; the changed one was read again.
    let view = bench.view();
    assert_eq!(view.summary.groups, 1);
    assert_eq!(view.notice, None);
    bench.session.apply();
    bench.settle();
    assert_eq!(bench.names(), ["2024-05-01_10-00-00.jpg"]);
}

#[test]
fn files_added_during_a_scan_wait_their_turn_and_a_cancel_drops_them() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.photo("b.jpg", 2);
    bench.photo("c.jpg", 3);
    bench.media.hold_scans();
    bench.session.add(vec![bench.path("a.jpg")]);
    bench.session.add(vec![bench.path("b.jpg")]);
    let view = bench.view();
    assert_eq!(view.queued, 1);
    assert!(matches!(view.busy, Busy::Scanning { .. }));
    assert!(!view.can_apply);
    assert_eq!(bench.session.tray_status(), TrayStatus::Scanning);
    // Applying while a scan runs does nothing.
    bench.session.apply();
    bench.media.release_scans();
    bench.settle();
    assert_eq!(bench.view().summary.groups, 2);
    assert_eq!(bench.view().queued, 0);
    assert_eq!(bench.names(), ["a.jpg", "b.jpg", "c.jpg"]);

    // A cancelled scan forgets what was queued behind it.
    bench.media.hold_scans();
    bench.session.add(vec![bench.path("c.jpg")]);
    bench.session.add(vec![bench.path("c.jpg")]);
    bench.session.act(Action::CancelScan);
    assert_eq!(bench.view().queued, 0);
    bench.media.release_scans();
    bench.settle();
    // A file that is already listed is not listed twice.
    bench.session.add(vec![bench.path("a.jpg")]);
    bench.settle();
    assert_eq!(
        bench
            .view()
            .rows
            .iter()
            .filter(|row| row.current == "a.jpg")
            .count(),
        1
    );
}

#[test]
fn an_unreadable_history_file_blocks_renames_so_it_is_never_overwritten() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.media.make_history_foreign();
    bench.session.load_history(bench.path("history.txt"));
    assert_eq!(bench.view().busy, Busy::Loading);
    bench.settle();
    bench.session.add(vec![bench.dir.clone()]);
    bench.settle();
    assert!(bench.view().can_apply);

    bench.session.apply();
    assert!(!bench.session.working(), "no rename was started");
    assert_eq!(bench.view().notice, Some(Notice::HistoryUnreadable));
    assert_eq!(bench.names(), ["a.jpg"]);
    bench.session.act(Action::DismissNotice);
    bench.session.act(Action::ClearHistory);
    assert_eq!(bench.view().notice, Some(Notice::HistoryUnreadable));
}

#[test]
fn deselected_rows_stay_put_and_filters_only_hide_rows() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.photo("b.jpg", 2);
    bench.session.add(vec![bench.dir.clone()]);
    bench.settle();
    let first = bench.group_of("a.jpg");
    bench.session.act(Action::SetSelected(first, false));
    bench.session.act(Action::SetFilterText("B.JPG".into()));
    let view = bench.view();
    assert_eq!(view.visible, 1);
    assert_eq!(view.filter_counts[StateFilter::All.index()], 2);
    assert_eq!(view.summary.ready, 1);
    assert!(view.all_visible_selected);
    bench
        .session
        .act(Action::SetStateFilter(StateFilter::Problem));
    assert_eq!(bench.view().visible, 0);
    bench.session.act(Action::ClearFilter);
    assert_eq!(bench.view().visible, 2);
    assert!(!bench.view().all_visible_selected);

    bench.session.apply();
    bench.settle();
    assert_eq!(bench.names(), ["2024-05-02_10-00-00.jpg", "a.jpg"]);

    // Select-all acts on the rows that are showing.
    bench.session.act(Action::SetSelectAllVisible(true));
    assert_eq!(bench.view().summary.ready, 1);
    bench.session.act(Action::SetSelectAllVisible(false));
    assert_eq!(bench.view().summary.ready, 0);
    assert!(!bench.view().can_apply);
}

#[test]
fn an_invalid_template_keeps_the_last_preview_and_cannot_be_applied() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.session.add(vec![bench.dir.clone()]);
    bench.settle();
    bench.session.act(Action::SetTemplate("{yyyy}".into()));
    assert!(
        bench
            .view()
            .rows
            .iter()
            .any(|row| row.new_name == "2024.jpg")
    );
    bench.session.act(Action::SetTemplate("{nope".into()));
    let view = bench.view();
    assert!(view.template_error.is_some());
    assert_eq!(view.template, "{nope");
    assert!(!view.can_apply);
    // The rows still show the last valid template's names.
    assert!(view.rows.iter().any(|row| row.new_name == "2024.jpg"));
    bench.session.apply();
    assert!(!bench.session.working());
    assert_eq!(bench.names(), ["a.jpg"]);
}

#[test]
fn clearing_empties_the_workspace_but_keeps_the_history() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.session.add(vec![bench.dir.clone()]);
    bench.settle();
    bench.session.apply();
    bench.settle();
    bench.session.act(Action::OpenDetails(0));
    assert!(matches!(bench.view().drawer, DrawerView::Details(_)));
    bench.session.act(Action::Clear);
    let view = bench.view();
    assert_eq!(view.summary.groups, 0);
    assert_eq!(view.notice, None);
    assert_eq!(view.drawer, DrawerView::Hidden);
    assert_eq!(view.history_count, 1);
    // The history drawer lists the batch with the platform's time text.
    bench.session.act(Action::ToggleHistory);
    let DrawerView::History(batches) = bench.view().drawer else {
        panic!("history drawer should be open");
    };
    assert_eq!(batches.len(), 1);
    assert!(batches[0].at.starts_with("at ") && batches[0].undoable);
    assert_eq!(
        batches[0].sample,
        [("a.jpg".to_string(), "2024-05-01_10-00-00.jpg".to_string())]
    );
    bench.session.act(Action::ToggleHistory);
    assert_eq!(bench.view().drawer, DrawerView::Hidden);
}

#[test]
fn explorer_input_replaces_the_workspace() {
    let mut bench = Bench::new();
    bench.photo("a.jpg", 1);
    bench.photo("b.jpg", 2);
    bench.session.add(vec![bench.path("a.jpg")]);
    bench.settle();
    bench.session.replace(vec![bench.path("b.jpg")]);
    bench.settle();
    let view = bench.view();
    assert_eq!(view.summary.groups, 1);
    assert_eq!(view.rows[0].current, "b.jpg");
    bench.session.shutdown();
    assert!(!bench.session.working());
}
