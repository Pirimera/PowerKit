//! Image Resizer jobs, Explorer verb and second-instance handoff. The dialog itself is
//! WinUI (`resize_dialog.rs`); without the Windows App Runtime there is no resize UI.
use super::{resize_wic::Images, wide};
use crate::image_resizer::{PlanFault, PlanItem, PlannedWrite, ResizeSettings, plan_batch};
use std::{
    cell::RefCell,
    ffi::OsString,
    io,
    mem::size_of,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use windows_sys::Win32::{
    Foundation::HWND,
    System::{
        DataExchange::COPYDATASTRUCT,
        Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW},
    },
    UI::WindowsAndMessaging::PostMessageW,
};
pub(super) const SHOW: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 8;
pub(super) const JOB: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 9;
const COPY_ID: usize = 0x5245_5A31;
pub(super) struct Shared {
    pub generation: u64,
    pub status: String,
    pub lines: Option<Vec<String>>,
    pub plan: Option<Vec<PlanItem>>,
    pub stop: bool,
}
thread_local! {
    static PENDING: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}
pub(super) fn parse_resize_args(args: &[String]) -> Option<Vec<PathBuf>> {
    let index = args.iter().position(|arg| arg == "--resize")?;
    Some(
        args.iter()
            .skip(index + 1)
            .take_while(|arg| !arg.starts_with('-'))
            .filter(|arg| !arg.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}
pub(super) fn command_files() -> Option<Vec<PathBuf>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(list) = resize_list(&args) {
        let files = read_resize_list(Path::new(list)).unwrap_or_default();
        let _ = std::fs::remove_file(list);
        return Some(files);
    }
    parse_resize_args(&args)
}
fn resize_list(args: &[String]) -> Option<&str> {
    let index = args.iter().position(|arg| arg == "--resize-list")?;
    args.get(index + 1).map(String::as_str)
}
pub(super) fn read_resize_list(path: &Path) -> io::Result<Vec<PathBuf>> {
    read_path_list(path, crate::image_resizer::MAX_BATCH)
}
/// A UTF-16LE (BOM) newline-separated path list the shell DLL wrote for a long selection.
pub(super) fn read_path_list(path: &Path, limit: usize) -> io::Result<Vec<PathBuf>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > limit.saturating_mul(1_024).max(1_048_576) {
        return Err(io::Error::other("resize list is too large"));
    }
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xFE {
        return Err(io::Error::other("resize list is not valid UTF-16"));
    }
    let (chunks, _) = bytes[2..].as_chunks::<2>();
    let units: Vec<u16> = chunks
        .iter()
        .map(|chunk| u16::from_le_bytes(*chunk))
        .collect();
    let mut files = Vec::new();
    for line in units.split(|unit| *unit == u16::from(b'\n') || *unit == u16::from(b'\r')) {
        if line.is_empty() || files.len() == limit {
            continue;
        }
        files.push(PathBuf::from(OsString::from_wide(line)));
    }
    Ok(files)
}
pub(super) fn forward(files: &[PathBuf]) -> io::Result<()> {
    forward_with(COPY_ID, files)
}
/// Sends paths to the running instance as one WM_COPYDATA tagged with `copy_id`.
pub(super) fn forward_with(copy_id: usize, files: &[PathBuf]) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let hwnd = find_main();
    if hwnd.is_null() {
        return Err(super::fault::Fault::NoRunningInstance.into());
    }
    let mut units = Vec::<u16>::new();
    for file in files {
        units.extend(file.as_os_str().encode_wide());
        units.push(0);
    }
    let mut copy = COPYDATASTRUCT {
        dwData: copy_id,
        cbData: (units.len() * 2) as u32,
        lpData: if units.is_empty() {
            null_mut()
        } else {
            units.as_mut_ptr().cast()
        },
    };
    // SAFETY: the UTF-16 buffer stays alive until SendMessageW returns to this process.
    let accepted = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_COPYDATA,
            0,
            (&mut copy as *mut COPYDATASTRUCT).cast::<std::ffi::c_void>() as isize,
        )
    };
    if accepted == 0 {
        Err(super::fault::Fault::ForwardFailed.into())
    } else {
        Ok(())
    }
}
pub(super) fn accept_lparam(l: isize) {
    if l == 0 {
        return;
    }
    // SAFETY: WM_COPYDATA keeps the sender's COPYDATASTRUCT alive until this handler returns.
    accept(unsafe { &*(l as *const COPYDATASTRUCT) });
}
pub(super) fn accept(data: &COPYDATASTRUCT) {
    if data.dwData != COPY_ID {
        return;
    }
    let files = if data.cbData == 0 || data.lpData.is_null() {
        Vec::new()
    } else {
        paths_from_utf16(data)
    };
    PENDING.with(|pending| *pending.borrow_mut() = Some(files));
}
pub(super) fn open_pending(main: HWND, settings: ResizeSettings) {
    let Some(files) = PENDING.with(|pending| pending.borrow_mut().take()) else {
        return;
    };
    open(main, files, settings);
}
/// Opens the WinUI dialog, asking for files first when none were given. Without the
/// settings shell (missing Windows App Runtime) there is no dialog to open.
pub(super) fn open(main: HWND, mut files: Vec<PathBuf>, settings: ResizeSettings) {
    if files.is_empty() {
        files = pick(main);
        if files.is_empty() {
            return;
        }
    }
    if !super::window::open_resize(main, files, settings) {
        super::report(super::ui_strings().settings_unavailable);
    }
}
/// Routes a worker completion posted to the app window.
pub(super) fn on_job() {
    super::resize_dialog::on_job();
}
pub(super) fn register_context_menu() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    let exe = exe.display().to_string().replace('"', "");
    let base = "Software\\Classes\\SystemFileAssociations\\image\\shell\\PowerKit.ImageResizer";
    let _ = set_sz(base, None, super::ui_strings().resize_verb);
    let _ = set_sz(base, Some("MultiSelectModel"), "Document");
    let _ = set_sz(base, Some("Icon"), &format!("\"{exe}\",0"));
    let _ = set_sz(
        &format!("{base}\\command"),
        None,
        &format!("\"{exe}\" --resize \"%1\""),
    );
    let dll = dir.join("powerkit_shell.dll");
    let clsid = format!("{{{}}}", include_str!("../../shell_clsid.txt").trim());
    if dll.is_file() {
        let _ = set_sz(base, Some("DelegateExecute"), &clsid);
        let key = format!("Software\\Classes\\CLSID\\{clsid}");
        let _ = set_sz(&key, None, "PowerKit Image Resizer");
        let server = format!("{key}\\InprocServer32");
        let _ = set_sz(&server, None, &dll.display().to_string().replace('"', ""));
        let _ = set_sz(&server, Some("ThreadingModel"), "Apartment");
        super::shell_package::start(dir.to_path_buf());
    } else {
        delete_value(base, "DelegateExecute");
    }
}
pub(super) fn delete_value(subkey: &str, name: &str) {
    let subkey = wide(subkey);
    let name = wide(name);
    // SAFETY: both strings are NUL-terminated and live for this call. No key handle is returned.
    unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), name.as_ptr());
    }
}
pub(super) fn set_sz(subkey: &str, name: Option<&str>, value: &str) -> bool {
    let subkey = wide(subkey);
    let name = name.map(wide);
    let mut value = wide(value);
    // SAFETY: registry strings are NUL-terminated and live for this call. No key handle is returned.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ref().map_or(null(), |name| name.as_ptr()),
            REG_SZ,
            value.as_mut_ptr().cast(),
            (value.len() * 2) as u32,
        )
    };
    status == 0
}
fn find_main() -> HWND {
    let class = wide("PowerKit.Window");
    for _ in 0..30 {
        let hwnd = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(class.as_ptr(), null())
        };
        if !hwnd.is_null() {
            return hwnd;
        }
        thread::sleep(std::time::Duration::from_millis(50));
    }
    null_mut()
}
pub(super) struct JobInput {
    /// Hidden app window that receives `JOB` updates for both Win32 and WinUI UIs.
    pub notify: isize,
    pub files: Vec<PathBuf>,
    pub settings: ResizeSettings,
    pub plan: Vec<PlanItem>,
    pub shared: Arc<Mutex<Shared>>,
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub write: bool,
    pub strings: &'static crate::locale::Strings,
}
pub(super) fn worker(input: JobInput) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if input.write {
            write_files(&input)
        } else {
            preview_files(&input)
        }
    }));
    if outcome.is_err() {
        publish(&input, input.strings.resize_stopped, None, None, true);
    }
    let hwnd = input.notify as HWND;
    unsafe { PostMessageW(hwnd, JOB, 0, 0) };
}
fn preview_files(input: &JobInput) -> io::Result<()> {
    let images = Images::new()?;
    if input.cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    let probed = input
        .files
        .iter()
        .map(|path| images.probe(path).ok())
        .collect::<Vec<_>>();
    let t = input.strings;
    let plan = plan_batch(
        &input.files,
        &input.settings,
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
    publish(input, &status, Some(lines), Some(plan), true);
    Ok(())
}
fn write_files(input: &JobInput) -> io::Result<()> {
    let t = input.strings;
    let Some(ready) = ready_only(&input.plan) else {
        publish(input, t.resize_need_preview, None, None, true);
        return Ok(());
    };
    let images = Images::new()?;
    for (index, plan) in ready.iter().enumerate() {
        if input.cancel.load(Ordering::Relaxed) {
            publish(input, t.resize_cancelled_left, None, None, true);
            return Ok(());
        }
        publish(
            input,
            &t.resize_writing_progress
                .replace("{current}", &(index + 1).to_string())
                .replace("{total}", &ready.len().to_string()),
            None,
            None,
            false,
        );
        unsafe { PostMessageW(input.notify as HWND, JOB, 0, 0) };
        if let Err(error) = images.write(plan, &input.settings) {
            publish(
                input,
                &t.resize_write_failed
                    .replace("{name}", &file_name(&plan.source))
                    .replace("{error}", &super::fault::describe(&error, t)),
                None,
                None,
                true,
            );
            return Ok(());
        }
    }
    publish(input, t.power_store_succeeded, None, None, true);
    Ok(())
}
fn publish(
    input: &JobInput,
    status: &str,
    lines: Option<Vec<String>>,
    plan: Option<Vec<PlanItem>>,
    done: bool,
) {
    let Ok(mut shared) = input.shared.lock() else {
        return;
    };
    if shared.generation != input.generation {
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
pub(super) fn ready_only(items: &[PlanItem]) -> Option<Vec<PlannedWrite>> {
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
fn preview_line(item: &PlanItem, t: &crate::locale::Strings) -> String {
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
fn fault_text(fault: PlanFault, t: &crate::locale::Strings) -> &'static str {
    match fault {
        PlanFault::Unreadable => t.resize_fault_unreadable,
        PlanFault::Dimension => t.resize_fault_dimension,
        PlanFault::Name => t.resize_fault_name,
        PlanFault::TooMany => t.resize_fault_too_many,
    }
}
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
/// Paths from a WM_COPYDATA buffer; valid only while the sender is blocked in SendMessageW.
pub(super) fn paths_from_utf16(data: &COPYDATASTRUCT) -> Vec<PathBuf> {
    if data.cbData == 0 || data.lpData.is_null() {
        return Vec::new();
    }
    use std::os::windows::ffi::OsStringExt;
    let bytes = data.cbData as usize;
    if !bytes.is_multiple_of(2) {
        return Vec::new();
    }
    let units = unsafe { std::slice::from_raw_parts(data.lpData.cast::<u16>(), bytes / 2) };
    let mut files = Vec::new();
    let mut start = 0;
    for (index, unit) in units.iter().enumerate() {
        if *unit == 0 {
            if index > start {
                files.push(PathBuf::from(std::ffi::OsString::from_wide(
                    &units[start..index],
                )));
            }
            start = index + 1;
        }
    }
    files
}
fn pick(owner: HWND) -> Vec<PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut buffer = vec![0u16; 32_768];
    let t = super::ui_strings();
    let filter = wide(&format!(
        "{}\0*.bmp;*.dib;*.gif;*.heic;*.jpg;*.jpeg;*.jpe;*.png;*.tif;*.tiff;*.wdp;*.webp\0\0",
        t.resize_picker_filter
    ));
    let title = wide(t.resize_picker_title);
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filter.as_ptr(),
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        lpstrTitle: title.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_ALLOWMULTISELECT
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_HIDEREADONLY,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        return Vec::new();
    }
    split_picker(&buffer)
}
fn split_picker(buffer: &[u16]) -> Vec<PathBuf> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, unit) in buffer.iter().enumerate() {
        if *unit == 0 {
            if index == start {
                break;
            }
            parts.push(String::from_utf16_lossy(&buffer[start..index]));
            start = index + 1;
        }
    }
    match parts.as_slice() {
        [] => Vec::new(),
        [one] => vec![PathBuf::from(one)],
        [directory, names @ ..] => names
            .iter()
            .map(|name| PathBuf::from(directory).join(name))
            .collect(),
    }
}
pub(super) fn parse_extent(value: &str) -> Option<f64> {
    let value = value.trim().replace(',', ".");
    if value.is_empty() {
        return Some(0.0);
    }
    let parsed = value.parse::<f64>().ok()?;
    (parsed.is_finite() && (0.0..=10_000_000.0).contains(&parsed)).then_some(parsed)
}
pub(super) fn quantity_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}
impl Shared {
    pub(super) fn clone_update(&self) -> Update {
        Update {
            generation: self.generation,
            status: self.status.clone(),
            lines: self.lines.clone(),
            plan: self.plan.clone(),
            stop: self.stop,
        }
    }
}
pub(super) struct Update {
    pub generation: u64,
    pub status: String,
    pub lines: Option<Vec<String>>,
    pub plan: Option<Vec<PlanItem>>,
    pub stop: bool,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_arguments_keep_files_and_stop_at_the_next_flag() {
        assert_eq!(
            parse_resize_args(&["--resize".into(), "a.jpg".into(), "b.png".into()]).unwrap(),
            vec![PathBuf::from("a.jpg"), PathBuf::from("b.png")]
        );
        assert_eq!(
            parse_resize_args(&["--resize".into(), "a.jpg".into(), "--smoke-test".into()]).unwrap(),
            vec![PathBuf::from("a.jpg")]
        );
        assert!(parse_resize_args(&["--smoke-test".into()]).is_none());
        assert_eq!(
            parse_resize_args(&["--resize".into()]).unwrap(),
            Vec::<PathBuf>::new()
        );
    }
    #[test]
    fn resize_list_reads_utf16_paths() {
        let path =
            std::env::temp_dir().join(format!("powerkit-list-test-{}.txt", std::process::id()));
        let mut bytes = vec![0xFF, 0xFE];
        for line in ["C:\\photos\\a.jpg", "", "C:\\photos\\b.png"] {
            for unit in line.encode_utf16() {
                bytes.extend(unit.to_le_bytes());
            }
            bytes.extend(u16::from(b'\n').to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        let files = read_resize_list(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(
            files,
            vec![
                PathBuf::from(r"C:\photos\a.jpg"),
                PathBuf::from(r"C:\photos\b.png")
            ]
        );
        assert!(read_resize_list(Path::new("missing-powerkit-resize-list.txt")).is_err());
    }
}
