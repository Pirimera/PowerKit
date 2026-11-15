//! Image Resizer dialog, Explorer verb and second-instance handoff.
use super::{resize_wic::Images, wide};
use crate::image_resizer::{
    Fit, PlanFault, PlanItem, PlannedWrite, ResizeSettings, Unit, plan_batch,
};
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
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{COLOR_WINDOW, CreateFontW, DEFAULT_CHARSET, DeleteObject, FW_NORMAL, HFONT},
    System::{
        DataExchange::COPYDATASTRUCT,
        LibraryLoader::GetModuleHandleW,
        Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW},
    },
    UI::{
        Controls::BST_CHECKED,
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::EnableWindow,
        WindowsAndMessaging::{
            BS_AUTOCHECKBOX, BS_PUSHBUTTON, CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL,
            CBS_DROPDOWNLIST, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
            DefWindowProcW, DestroyWindow, ES_AUTOHSCROLL, GetClientRect, GetDlgItem,
            GetWindowTextLengthW, GetWindowTextW, HMENU, IsWindow, LB_ADDSTRING, LB_GETCURSEL,
            LB_RESETCONTENT, LB_SETCURSEL, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY, PostMessageW,
            RegisterClassW, SW_SHOW, SendMessageW, SetForegroundWindow, SetWindowPos,
            SetWindowTextW, ShowWindow, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_SIZE,
            WNDCLASSW, WS_BORDER, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
            WS_VSCROLL,
        },
    },
};
pub(super) const SHOW: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 8;
pub(super) const JOB: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 9;
const COPY_ID: usize = 0x5245_5A31;
const SIZE_LIST: i32 = 1;
const WIDTH: i32 = 2;
const HEIGHT: i32 = 3;
const UNIT: i32 = 4;
const FIT: i32 = 5;
const SHRINK: i32 = 6;
const REPLACE: i32 = 7;
const ORIENT: i32 = 8;
const PREVIEW: i32 = 9;
const STATUS: i32 = 10;
const PREVIEW_BTN: i32 = 11;
const APPLY_BTN: i32 = 12;
const SETTINGS_BTN: i32 = 13;
const CANCEL_BTN: i32 = 14;
const BM_GETCHECK: u32 = 240;
const BM_SETCHECK: u32 = 241;
struct Seed {
    main: HWND,
    files: Vec<PathBuf>,
    settings: ResizeSettings,
}
pub(super) struct Shared {
    pub generation: u64,
    pub status: String,
    pub lines: Option<Vec<String>>,
    pub plan: Option<Vec<PlanItem>>,
    pub stop: bool,
}
struct Dialog {
    main: HWND,
    hwnd: HWND,
    files: Vec<PathBuf>,
    settings: ResizeSettings,
    plan: Option<Vec<PlanItem>>,
    generation: u64,
    cancel: Option<Arc<AtomicBool>>,
    close_when_idle: bool,
    font: HFONT,
    shared: Arc<Mutex<Shared>>,
}
thread_local! {
    static SEED: RefCell<Option<Seed>> = const { RefCell::new(None) };
    static DIALOG: RefCell<Option<Dialog>> = const { RefCell::new(None) };
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
    let bytes = std::fs::read(path)?;
    if bytes.len() > 1_048_576 {
        return Err(io::Error::other("Boyutlandırma listesi çok büyük"));
    }
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xFE {
        return Err(io::Error::other("Boyutlandırma listesi okunamadı"));
    }
    let (chunks, _) = bytes[2..].as_chunks::<2>();
    let units: Vec<u16> = chunks
        .iter()
        .map(|chunk| u16::from_le_bytes(*chunk))
        .collect();
    let mut files = Vec::new();
    for line in units.split(|unit| *unit == u16::from(b'\n') || *unit == u16::from(b'\r')) {
        if line.is_empty() || files.len() == crate::image_resizer::MAX_BATCH {
            continue;
        }
        files.push(PathBuf::from(OsString::from_wide(line)));
    }
    Ok(files)
}
pub(super) fn forward(files: &[PathBuf]) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let hwnd = find_main();
    if hwnd.is_null() {
        return Err(io::Error::other("Çalışan PowerKit penceresi bulunamadı"));
    }
    let mut units = Vec::<u16>::new();
    for file in files {
        units.extend(file.as_os_str().encode_wide());
        units.push(0);
    }
    let mut copy = COPYDATASTRUCT {
        dwData: COPY_ID,
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
        Err(io::Error::other(
            "Dosya listesi çalışan PowerKit'e iletilemedi",
        ))
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
pub(super) fn open(main: HWND, mut files: Vec<PathBuf>, settings: ResizeSettings) {
    if files.is_empty() {
        files = pick(main);
        if files.is_empty() {
            return;
        }
    }
    if super::window::open_resize(main, files.clone(), settings.clone()) {
        return;
    }
    open_win32(main, files, settings);
}
/// Win32 dialog used when the WinUI shell is unavailable (tray-only fallback).
pub(super) fn open_win32(main: HWND, files: Vec<PathBuf>, settings: ResizeSettings) {
    if let Some(hwnd) = live() {
        DIALOG.with(|slot| {
            let mut dialog = slot.borrow_mut();
            let Some(dialog) = dialog.as_mut() else {
                return;
            };
            dialog.files = files;
            dialog.settings = settings;
            invalidate(dialog);
        });
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
        fill(hwnd);
        return;
    }
    SEED.with(|seed| {
        *seed.borrow_mut() = Some(Seed {
            main,
            files,
            settings,
        })
    });
    if !register() {
        SEED.with(|seed| seed.borrow_mut().take());
        super::report("Image Resizer penceresi açılamadı");
        return;
    }
    let dpi = unsafe { GetDpiForWindow(main) }.max(96);
    let class = wide("PowerKit.ImageResizer");
    // SAFETY: class name is live; the window procedure stays registered for the process.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Image Resizer").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(dpi, 560),
            scale(dpi, 700),
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        )
    };
    if hwnd.is_null() {
        SEED.with(|seed| seed.borrow_mut().take());
        super::report("Image Resizer penceresi açılamadı");
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}
/// Routes a worker completion posted to the app window.
pub(super) fn on_job() {
    if let Some(hwnd) = live() {
        apply_job(hwnd);
    }
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
    let _ = set_sz(base, None, "Image Resizer ile yeniden boyutlandır");
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
fn delete_value(subkey: &str, name: &str) {
    let subkey = wide(subkey);
    let name = wide(name);
    // SAFETY: both strings are NUL-terminated and live for this call. No key handle is returned.
    unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), name.as_ptr());
    }
}
fn set_sz(subkey: &str, name: Option<&str>, value: &str) -> bool {
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
fn register() -> bool {
    use std::sync::atomic::AtomicBool;
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Relaxed) {
        return true;
    }
    let class = wide("PowerKit.ImageResizer");
    let window = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(procedure),
        hbrBackground: (COLOR_WINDOW + 1) as _,
        lpszClassName: class.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: the class name and procedure outlive the process.
    let atom = unsafe { RegisterClassW(&window) };
    let ok = atom != 0 || std::io::Error::last_os_error().raw_os_error() == Some(1410);
    if ok {
        REGISTERED.store(true, Ordering::Relaxed);
    }
    ok
}
pub(super) fn live() -> Option<HWND> {
    DIALOG.with(|dialog| {
        dialog.borrow().as_ref().and_then(|dialog| {
            (!dialog.hwnd.is_null() && unsafe { IsWindow(dialog.hwnd) } != 0).then_some(dialog.hwnd)
        })
    })
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
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let Some(seed) = SEED.with(|seed| seed.borrow_mut().take()) else {
                return -1;
            };
            if build(hwnd, seed).is_err() {
                return -1;
            }
            0
        }
        WM_SIZE => {
            layout(hwnd);
            0
        }
        WM_COMMAND => {
            on_command(hwnd, (w & 0xffff) as i32, (w >> 16) as u32);
            0
        }
        msg if msg == JOB => {
            apply_job(hwnd);
            0
        }
        WM_CLOSE => {
            let running = DIALOG.with(|dialog| {
                dialog
                    .borrow()
                    .as_ref()
                    .is_some_and(|dialog| dialog.cancel.is_some())
            });
            if running {
                cancel_job(true);
                set_status(hwnd, "İptal ediliyor");
            } else {
                unsafe { DestroyWindow(hwnd) };
            }
            0
        }
        WM_DESTROY => {
            DIALOG.with(|dialog| {
                if let Some(dialog) = dialog.borrow_mut().take()
                    && !dialog.font.is_null()
                {
                    unsafe { DeleteObject(dialog.font) };
                }
            });
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}
fn build(hwnd: HWND, seed: Seed) -> io::Result<()> {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let font = ui_font(dpi)?;
    let instance = unsafe { GetModuleHandleW(null()) };
    create_child(hwnd, instance, font, "Boyut", "STATIC", 20, 0)?;
    create_child(
        hwnd,
        instance,
        font,
        "",
        "LISTBOX",
        SIZE_LIST,
        WS_BORDER | WS_VSCROLL | WS_TABSTOP | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
    )?;
    create_child(hwnd, instance, font, "", "EDIT", WIDTH, edit_style())?;
    create_child(hwnd, instance, font, "", "EDIT", HEIGHT, edit_style())?;
    create_child(hwnd, instance, font, "", "COMBOBOX", UNIT, combo_style())?;
    create_child(hwnd, instance, font, "", "COMBOBOX", FIT, combo_style())?;
    create_child(
        hwnd,
        instance,
        font,
        "Yalnızca küçült, büyütme",
        "BUTTON",
        SHRINK,
        check_style(),
    )?;
    create_child(
        hwnd,
        instance,
        font,
        "Özgün resimlerin üzerine yaz",
        "BUTTON",
        REPLACE,
        check_style(),
    )?;
    create_child(
        hwnd,
        instance,
        font,
        "Yönü yoksay",
        "BUTTON",
        ORIENT,
        check_style(),
    )?;
    create_child(hwnd, instance, font, "Önizleme", "STATIC", 21, 0)?;
    create_child(
        hwnd,
        instance,
        font,
        "",
        "LISTBOX",
        PREVIEW,
        WS_BORDER | WS_VSCROLL | LBS_NOINTEGRALHEIGHT as u32,
    )?;
    create_child(
        hwnd,
        instance,
        font,
        "Önizleme dosyaya yazmaz.",
        "STATIC",
        STATUS,
        22,
    )?;
    create_child(
        hwnd,
        instance,
        font,
        "Önizle",
        "BUTTON",
        PREVIEW_BTN,
        button_style(),
    )?;
    create_child(
        hwnd,
        instance,
        font,
        "Yeniden boyutlandır",
        "BUTTON",
        APPLY_BTN,
        button_style(),
    )?;
    create_child(
        hwnd,
        instance,
        font,
        "Ayarlar",
        "BUTTON",
        SETTINGS_BTN,
        button_style(),
    )?;
    create_child(
        hwnd,
        instance,
        font,
        "İptal",
        "BUTTON",
        CANCEL_BTN,
        button_style(),
    )?;
    for (id, label) in Unit::Pixel.label_pairs() {
        add_combo(hwnd, UNIT, label, *id as isize);
    }
    for (id, label) in Fit::Fit.label_pairs() {
        add_combo(hwnd, FIT, label, *id as isize);
    }
    let shared = Arc::new(Mutex::new(Shared {
        generation: 0,
        status: "Önizleme dosyaya yazmaz.".into(),
        lines: None,
        plan: None,
        stop: false,
    }));
    DIALOG.with(|dialog| {
        *dialog.borrow_mut() = Some(Dialog {
            main: seed.main,
            hwnd,
            files: seed.files,
            settings: seed.settings,
            plan: None,
            generation: 0,
            cancel: None,
            close_when_idle: false,
            font,
            shared,
        });
    });
    fill(hwnd);
    layout(hwnd);
    Ok(())
}
fn fill(hwnd: HWND) {
    let (labels, settings) = DIALOG.with(|dialog| {
        let dialog = dialog.borrow();
        let Some(dialog) = dialog.as_ref() else {
            return (Vec::new(), None);
        };
        (
            dialog.settings.choice_labels(),
            Some(dialog.settings.clone()),
        )
    });
    let Some(settings) = settings else {
        return;
    };
    reset_list(hwnd, SIZE_LIST);
    for label in &labels {
        add_list(hwnd, SIZE_LIST, label);
    }
    unsafe {
        SendMessageW(hwnd_of(hwnd, SIZE_LIST), LB_SETCURSEL, settings.selected, 0);
        set_text(hwnd_of(hwnd, WIDTH), &quantity_text(settings.custom.width));
        set_text(
            hwnd_of(hwnd, HEIGHT),
            &quantity_text(settings.custom.height),
        );
        SendMessageW(
            hwnd_of(hwnd, UNIT),
            CB_SETCURSEL,
            settings.custom.unit as usize,
            0,
        );
        SendMessageW(
            hwnd_of(hwnd, FIT),
            CB_SETCURSEL,
            settings.custom.fit as usize,
            0,
        );
        SendMessageW(
            hwnd_of(hwnd, SHRINK),
            BM_SETCHECK,
            usize::from(settings.shrink_only),
            0,
        );
        SendMessageW(
            hwnd_of(hwnd, REPLACE),
            BM_SETCHECK,
            usize::from(settings.replace),
            0,
        );
        SendMessageW(
            hwnd_of(hwnd, ORIENT),
            BM_SETCHECK,
            usize::from(settings.ignore_orientation),
            0,
        );
        EnableWindow(hwnd_of(hwnd, APPLY_BTN), 0);
    }
    custom_enabled(hwnd, settings.selected == settings.sizes.len());
}
fn layout(hwnd: HWND) {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) };
    let pad = scale(dpi, 12);
    let row = scale(dpi, 28);
    let gap = scale(dpi, 8);
    let width = (client.right - pad * 2).max(pad);
    let mut y = pad;
    place(hwnd, 20, pad, y, width, row);
    y += row + gap;
    let list = scale(dpi, 132);
    place(hwnd, SIZE_LIST, pad, y, width, list);
    y += list + gap;
    let part = (width - gap * 3) / 4;
    place(hwnd, WIDTH, pad, y, part, row);
    place(hwnd, HEIGHT, pad + part + gap, y, part, row);
    place(hwnd, UNIT, pad + (part + gap) * 2, y, part, row);
    place(hwnd, FIT, pad + (part + gap) * 3, y, part, row);
    y += row + gap;
    for id in [SHRINK, REPLACE, ORIENT] {
        place(hwnd, id, pad, y, width, row);
        y += row + gap / 2;
    }
    y += gap;
    place(hwnd, 21, pad, y, width, row);
    y += row;
    let buttons = row + gap;
    let preview_h = (client.bottom - y - buttons - row - pad).max(scale(dpi, 80));
    place(hwnd, PREVIEW, pad, y, width, preview_h);
    y += preview_h + gap;
    place(hwnd, STATUS, pad, y, width, row);
    y += row + gap;
    let button = (width - gap * 3) / 4;
    for (index, id) in [PREVIEW_BTN, APPLY_BTN, SETTINGS_BTN, CANCEL_BTN]
        .into_iter()
        .enumerate()
    {
        place(
            hwnd,
            id,
            pad + (button + gap) * index as i32,
            y,
            button,
            row,
        );
    }
}
fn on_command(hwnd: HWND, id: i32, note: u32) {
    if matches!(id, SIZE_LIST | UNIT | FIT) && note == 1
        || matches!(id, WIDTH | HEIGHT) && note == 0x0300
        || matches!(id, SHRINK | REPLACE | ORIENT) && note == 0
    {
        if id == SIZE_LIST {
            let selected = unsafe { SendMessageW(hwnd_of(hwnd, SIZE_LIST), LB_GETCURSEL, 0, 0) };
            let count = DIALOG.with(|dialog| {
                dialog
                    .borrow()
                    .as_ref()
                    .map(|dialog| dialog.settings.sizes.len())
                    .unwrap_or(0)
            });
            custom_enabled(hwnd, selected as usize == count);
        }
        invalidate_preview();
    }
    if note != 0 {
        return;
    }
    match id {
        PREVIEW_BTN => preview(hwnd),
        APPLY_BTN => apply(hwnd),
        SETTINGS_BTN => {
            let main = DIALOG.with(|dialog| dialog.borrow().as_ref().map(|dialog| dialog.main));
            if let Some(main) = main {
                super::present_resize_page(main);
            }
        }
        CANCEL_BTN => unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(hwnd, WM_CLOSE, 0, 0);
        },
        _ => {}
    }
}
fn preview(hwnd: HWND) {
    let Some(settings) = read_settings(hwnd) else {
        set_status(hwnd, "Özel boyut sayı değil");
        return;
    };
    if let Err(error) = super::remember_resize(dialog_main(), settings.clone()) {
        super::report(&error.to_string());
        return;
    }
    DIALOG.with(|dialog| {
        if let Some(dialog) = dialog.borrow_mut().as_mut() {
            dialog.settings = settings;
        }
    });
    start_preview_thread(hwnd, false);
}
fn apply(hwnd: HWND) {
    let plan = DIALOG.with(|dialog| {
        dialog
            .borrow()
            .as_ref()
            .and_then(|dialog| dialog.plan.clone())
    });
    let Some(plan) = plan else {
        return;
    };
    if ready_only(&plan).is_none() {
        set_status(hwnd, "Önce hatasız bir önizleme gerekir");
        return;
    }
    start_preview_thread(hwnd, true);
}
fn invalidate_preview() {
    DIALOG.with(|slot| {
        let mut dialog = slot.borrow_mut();
        let Some(dialog) = dialog.as_mut() else {
            return;
        };
        dialog.plan = None;
        dialog.generation = dialog.generation.wrapping_add(1);
        if let Some(cancel) = dialog.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Ok(mut shared) = dialog.shared.lock() {
            shared.generation = dialog.generation;
            shared.stop = false;
            shared.status = "Önizleme geçersiz".into();
        }
        unsafe { EnableWindow(hwnd_of(dialog.hwnd, APPLY_BTN), 0) };
    });
}
fn invalidate(dialog: &mut Dialog) {
    dialog.plan = None;
    dialog.generation = dialog.generation.wrapping_add(1);
}
fn apply_job(hwnd: HWND) {
    let update = DIALOG.with(|dialog| {
        dialog.borrow().as_ref().and_then(|dialog| {
            dialog
                .shared
                .lock()
                .ok()
                .map(|shared| shared.clone_update())
        })
    });
    let Some(update) = update else {
        return;
    };
    let current = DIALOG.with(|dialog| dialog.borrow().as_ref().map(|dialog| dialog.generation));
    if current != Some(update.generation) {
        return;
    }
    set_status(hwnd, &update.status);
    if let Some(lines) = update.lines {
        reset_list(hwnd, PREVIEW);
        for line in lines {
            add_list(hwnd, PREVIEW, &line);
        }
    }
    if update.stop {
        DIALOG.with(|dialog| {
            if let Some(dialog) = dialog.borrow_mut().as_mut() {
                dialog.cancel = None;
                if let Some(plan) = update.plan {
                    let enable = ready_only(&plan).is_some();
                    dialog.plan = Some(plan);
                    unsafe { EnableWindow(hwnd_of(hwnd, APPLY_BTN), i32::from(enable)) };
                }
                if dialog.close_when_idle {
                    unsafe { DestroyWindow(hwnd) };
                }
            }
        });
    }
}
fn read_settings(hwnd: HWND) -> Option<ResizeSettings> {
    let mut settings = DIALOG.with(|dialog| {
        dialog
            .borrow()
            .as_ref()
            .map(|dialog| dialog.settings.clone())
    })?;
    let latest = super::current_resize();
    settings.jpeg_quality = latest.jpeg_quality;
    settings.png = latest.png;
    settings.tiff = latest.tiff;
    settings.filename = latest.filename;
    settings.keep_date = latest.keep_date;
    settings.remove_metadata = latest.remove_metadata;
    settings.fallback = latest.fallback;
    settings.sizes = latest.sizes;
    let selected = unsafe { SendMessageW(hwnd_of(hwnd, SIZE_LIST), LB_GETCURSEL, 0, 0) };
    if selected < 0 {
        return None;
    }
    settings.selected = selected as usize;
    settings.shrink_only = checked(hwnd, SHRINK);
    settings.replace = checked(hwnd, REPLACE);
    settings.ignore_orientation = checked(hwnd, ORIENT);
    if settings.selected == settings.sizes.len() {
        settings.custom.width = parse_extent(&text_of(hwnd, WIDTH))?;
        settings.custom.height = parse_extent(&text_of(hwnd, HEIGHT))?;
        settings.custom.unit = Unit::from_index(combo(hwnd, UNIT)).unwrap_or(Unit::Pixel);
        settings.custom.fit = Fit::from_index(combo(hwnd, FIT)).unwrap_or(Fit::Fit);
    }
    Some(settings)
}
fn start_preview_thread(hwnd: HWND, write: bool) {
    let prepared = DIALOG.with(|dialog| {
        let mut slot = dialog.borrow_mut();
        let dialog = slot.as_mut()?;
        if let Some(cancel) = dialog.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        dialog.generation = dialog.generation.wrapping_add(1);
        let cancel = Arc::new(AtomicBool::new(false));
        dialog.cancel = Some(Arc::clone(&cancel));
        let generation = dialog.generation;
        if let Ok(mut shared) = dialog.shared.lock() {
            shared.generation = generation;
            shared.stop = false;
            shared.lines = None;
            shared.plan = None;
        }
        Some(JobInput {
            notify: dialog.main as isize,
            files: dialog.files.clone(),
            settings: dialog.settings.clone(),
            plan: dialog.plan.clone().unwrap_or_default(),
            shared: Arc::clone(&dialog.shared),
            generation,
            cancel,
            write,
        })
    });
    let Some(input) = prepared else {
        return;
    };
    set_status(hwnd, if write { "Yazılıyor" } else { "Önizleniyor" });
    unsafe { EnableWindow(hwnd_of(hwnd, APPLY_BTN), 0) };
    thread::spawn(move || worker(input));
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
        publish(&input, "Yeniden boyutlandırma durdu", None, None, true);
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
    let plan = plan_batch(&input.files, &input.settings, &probed, |path| path.exists());
    let lines = plan.iter().map(preview_line).collect::<Vec<_>>();
    let failed = plan
        .iter()
        .any(|item| matches!(item, PlanItem::Failed { .. }));
    let status = if plan.is_empty() {
        "Dosya yok".into()
    } else if failed {
        "Önizlemede hata var. Yazılmadı.".into()
    } else {
        format!("{} dosya hazır", plan.len())
    };
    publish(input, &status, Some(lines), Some(plan), true);
    Ok(())
}
fn write_files(input: &JobInput) -> io::Result<()> {
    let Some(ready) = ready_only(&input.plan) else {
        publish(input, "Önce hatasız bir önizleme gerekir", None, None, true);
        return Ok(());
    };
    let images = Images::new()?;
    for (index, plan) in ready.iter().enumerate() {
        if input.cancel.load(Ordering::Relaxed) {
            publish(
                input,
                "İptal edildi. Kalan dosyalara dokunulmadı.",
                None,
                None,
                true,
            );
            return Ok(());
        }
        publish(
            input,
            &format!("{} / {} yazılıyor", index + 1, ready.len()),
            None,
            None,
            false,
        );
        unsafe { PostMessageW(input.notify as HWND, JOB, 0, 0) };
        if let Err(error) = images.write(plan, &input.settings) {
            publish(
                input,
                &format!("{} yazılamadı: {error}", file_name(&plan.source)),
                None,
                None,
                true,
            );
            return Ok(());
        }
    }
    publish(input, "Tamamlandı", None, None, true);
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
fn preview_line(item: &PlanItem) -> String {
    match item {
        PlanItem::Failed { source, fault } => {
            format!("{} — {}", file_name(source), fault_text(*fault))
        }
        PlanItem::Ready(plan) => {
            let destination = if plan.replace {
                "özgün dosya".into()
            } else {
                file_name(&plan.destination)
            };
            let size = if plan.frame.unchanged {
                format!("{}×{} aynı", plan.source_width, plan.source_height)
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
fn fault_text(fault: PlanFault) -> &'static str {
    match fault {
        PlanFault::Unreadable => "okunamadı",
        PlanFault::Dimension => "boyut geçersiz",
        PlanFault::Name => "dosya adı üretilemedi",
        PlanFault::TooMany => "en fazla 1000 dosya",
    }
}
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
fn paths_from_utf16(data: &COPYDATASTRUCT) -> Vec<PathBuf> {
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
    let filter = wide(
        "Resimler\0*.bmp;*.dib;*.gif;*.heic;*.jpg;*.jpeg;*.jpe;*.png;*.tif;*.tiff;*.wdp;*.webp\0\0",
    );
    let title = wide("Resimleri seçin");
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
fn dialog_main() -> HWND {
    DIALOG.with(|dialog| {
        dialog
            .borrow()
            .as_ref()
            .map(|dialog| dialog.main)
            .unwrap_or(null_mut())
    })
}
fn cancel_job(close_when_idle: bool) {
    DIALOG.with(|slot| {
        let mut dialog = slot.borrow_mut();
        let Some(dialog) = dialog.as_mut() else {
            return;
        };
        dialog.close_when_idle = close_when_idle;
        if let Some(cancel) = &dialog.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    });
}
fn custom_enabled(hwnd: HWND, enabled: bool) {
    let flag = i32::from(enabled);
    unsafe {
        for id in [WIDTH, HEIGHT, UNIT, FIT] {
            EnableWindow(hwnd_of(hwnd, id), flag);
        }
    }
}
fn checked(hwnd: HWND, id: i32) -> bool {
    unsafe { SendMessageW(hwnd_of(hwnd, id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize }
}
fn combo(hwnd: HWND, id: i32) -> i32 {
    unsafe { SendMessageW(hwnd_of(hwnd, id), CB_GETCURSEL, 0, 0) as i32 }
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
fn text_of(hwnd: HWND, id: i32) -> String {
    let control = hwnd_of(hwnd, id);
    unsafe {
        let length = GetWindowTextLengthW(control).max(0) as usize;
        let mut buffer = vec![0u16; length + 1];
        GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf16_lossy(&buffer)
            .trim_end_matches('\0')
            .to_string()
    }
}
fn set_text(hwnd: HWND, text: &str) {
    unsafe { SetWindowTextW(hwnd, wide(text).as_ptr()) };
}
fn set_status(hwnd: HWND, text: &str) {
    set_text(hwnd_of(hwnd, STATUS), text);
}
fn add_list(hwnd: HWND, id: i32, text: &str) {
    let text = wide(text);
    unsafe { SendMessageW(hwnd_of(hwnd, id), LB_ADDSTRING, 0, text.as_ptr() as isize) };
}
fn add_combo(hwnd: HWND, id: i32, text: &str, _value: isize) {
    let text = wide(text);
    unsafe { SendMessageW(hwnd_of(hwnd, id), CB_ADDSTRING, 0, text.as_ptr() as isize) };
}
fn reset_list(hwnd: HWND, id: i32) {
    unsafe { SendMessageW(hwnd_of(hwnd, id), LB_RESETCONTENT, 0, 0) };
}
fn hwnd_of(hwnd: HWND, id: i32) -> HWND {
    if id >= 20 {
        return unsafe { GetDlgItem(hwnd, id) };
    }
    unsafe { GetDlgItem(hwnd, id) }
}
fn place(hwnd: HWND, id: i32, x: i32, y: i32, width: i32, height: i32) {
    let child = hwnd_of(hwnd, id);
    if child.is_null() {
        return;
    }
    unsafe {
        SetWindowPos(
            child,
            null_mut(),
            x,
            y,
            width,
            height,
            windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
        );
    }
}
fn create_child(
    parent: HWND,
    instance: windows_sys::Win32::Foundation::HINSTANCE,
    font: HFONT,
    text: &str,
    class: &str,
    id: i32,
    style: u32,
) -> io::Result<HWND> {
    let child = unsafe {
        CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            parent,
            id as HMENU,
            instance,
            null(),
        )
    };
    if child.is_null() {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        SendMessageW(
            child,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_SETFONT,
            font as usize,
            1,
        )
    };
    Ok(child)
}
fn edit_style() -> u32 {
    WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32
}
fn combo_style() -> u32 {
    WS_VSCROLL | WS_TABSTOP | CBS_DROPDOWNLIST as u32
}
fn check_style() -> u32 {
    WS_TABSTOP | BS_AUTOCHECKBOX as u32
}
fn button_style() -> u32 {
    WS_TABSTOP | BS_PUSHBUTTON as u32
}
fn scale(dpi: u32, value: i32) -> i32 {
    value * dpi as i32 / 96
}
fn ui_font(dpi: u32) -> io::Result<HFONT> {
    let font = unsafe {
        CreateFontW(
            -scale(dpi, 14),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            0,
            0,
            wide("Segoe UI").as_ptr(),
        )
    };
    if font.is_null() {
        Err(io::Error::other("Yazı tipi oluşturulamadı"))
    } else {
        Ok(font)
    }
}
trait ChoiceLabel {
    fn label_pairs(self) -> &'static [(i32, &'static str)];
}
impl ChoiceLabel for Unit {
    fn label_pairs(self) -> &'static [(i32, &'static str)] {
        &[
            (Unit::Pixel as i32, "Piksel"),
            (Unit::Percent as i32, "Yüzde"),
            (Unit::Inch as i32, "İnç"),
            (Unit::Centimeter as i32, "Santimetre"),
        ]
    }
}
impl ChoiceLabel for Fit {
    fn label_pairs(self) -> &'static [(i32, &'static str)] {
        &[
            (Fit::Fit as i32, "Sığdır"),
            (Fit::Fill as i32, "Doldur"),
            (Fit::Stretch as i32, "Uzat"),
        ]
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
