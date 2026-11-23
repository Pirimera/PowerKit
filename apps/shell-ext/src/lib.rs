//! In-process Explorer commands for Image Resizer and MediaRenamer.
//!
//! Windows 11 loads this DLL through a sparse-package surrogate. The classic
//! verbs load it in-process. Both paths collect the selected paths and start
//! `powerkit.exe` beside this DLL. COM is owned by the host thread.
//!
//! The class ids are the text files in `packaging/msix`, shared packaging data
//! that the platform crate and the packaging script read too.
#![cfg(windows)]

use powerkit_core::media_extensions;
use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicI32, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{
            CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, CloseHandle, E_FAIL, E_NOTIMPL,
            E_POINTER, S_FALSE, S_OK,
        },
        System::{
            Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl},
            LibraryLoader::GetModuleFileNameW,
            Threading::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW},
        },
        UI::Shell::{
            AssocGetPerceivedType,
            Common::{PERCEIVED, PERCEIVED_TYPE_IMAGE},
            ECF_DEFAULT, ECS_ENABLED, ECS_HIDDEN, IEnumExplorerCommand, IExecuteCommand,
            IExecuteCommand_Impl, IExplorerCommand, IExplorerCommand_Impl, IObjectWithSelection,
            IObjectWithSelection_Impl, IShellItem, IShellItemArray, SHStrDupW, SIGDN_FILESYSPATH,
        },
    },
    core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result, implement},
};

const COMMAND_LIMIT: usize = 32_000;

/// Which PowerKit tool a COM class launches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Resize,
    Rename,
}
impl Tool {
    const ALL: [Self; 2] = [Self::Resize, Self::Rename];
    fn class_id(self) -> GUID {
        static IDS: OnceLock<[GUID; 2]> = OnceLock::new();
        let parse = |text: &str| GUID::try_from(text.trim()).unwrap_or_else(|_| GUID::from_u128(0));
        IDS.get_or_init(|| {
            [
                parse(include_str!("../../../packaging/msix/shell_clsid.txt")),
                parse(include_str!(
                    "../../../packaging/msix/shell_clsid_rename.txt"
                )),
            ]
        })[self as usize]
    }
    fn from_class(class: &GUID) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|tool| tool.class_id() == *class && tool.class_id() != GUID::from_u128(0))
    }
    fn title(self) -> &'static str {
        match (self, shell_ui_language()) {
            (Self::Resize, ShellUi::English) => "Resize with Image Resizer",
            (Self::Resize, ShellUi::Arabic) => "تغيير الحجم باستخدام Image Resizer",
            (Self::Resize, ShellUi::Turkish) => "Image Resizer ile yeniden boyutlandır",
            (Self::Rename, ShellUi::English) => "Rename with MediaRenamer",
            (Self::Rename, ShellUi::Arabic) => "إعادة التسمية باستخدام MediaRenamer",
            (Self::Rename, ShellUi::Turkish) => "MediaRenamer ile yeniden adlandır",
        }
    }
    /// Inline flag, list flag, list file prefix and the most paths one launch carries.
    fn launch_flags(self) -> (&'static str, &'static str, &'static str, usize) {
        match self {
            Self::Resize => ("--resize", "--resize-list", "powerkit-resize", 1_000),
            Self::Rename => ("--rename", "--rename-list", "powerkit-rename", 20_000),
        }
    }
    fn accepts(self, path: &Path) -> bool {
        match self {
            Self::Resize => perceived_image(path) != Some(false),
            Self::Rename => {
                path.is_dir()
                    || path
                        .extension()
                        .and_then(OsStr::to_str)
                        .is_some_and(media_extensions::is_media)
            }
        }
    }
    fn shows_for(self, path: &Path) -> bool {
        match self {
            Self::Resize => perceived_image(path) == Some(true),
            Self::Rename => self.accepts(path),
        }
    }
}

#[derive(Clone, Copy)]
enum ShellUi {
    Turkish,
    English,
    Arabic,
}

fn shell_ui_language() -> ShellUi {
    if let Some(lang) = read_powerkit_language() {
        return lang;
    }
    windows_shell_ui_language()
}

fn read_powerkit_language() -> Option<ShellUi> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let path = std::path::PathBuf::from(local)
        .join("PowerKit")
        .join("settings.txt");
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "language" {
            continue;
        }
        return Some(match value.trim() {
            "en" => ShellUi::English,
            "ar" => ShellUi::Arabic,
            "tr" => ShellUi::Turkish,
            "system" => windows_shell_ui_language(),
            _ => return None,
        });
    }
    None
}

fn windows_shell_ui_language() -> ShellUi {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // SAFETY: returns the current user UI language; no pointers involved.
    let primary = unsafe { GetUserDefaultUILanguage() } & 0x3ff;
    match primary {
        0x01 => ShellUi::Arabic,
        0x09 => ShellUi::English,
        _ => ShellUi::Turkish,
    }
}
static OBJECTS: AtomicI32 = AtomicI32::new(0);
static MODULE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[implement(IClassFactory)]
struct Factory(Tool);

impl Drop for Factory {
    fn drop(&mut self) {
        OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        riid: *const GUID,
        object: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        if outer.as_ref().is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        if riid.is_null() || object.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: the caller provided a writable interface slot.
        unsafe { *object = core::ptr::null_mut() };
        let verb: IUnknown = Verb::new(self.0).into();
        // SAFETY: riid and object are non-null. Query writes one interface pointer.
        unsafe { verb.query(riid, object) }.ok()
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            OBJECTS.fetch_add(1, Ordering::Relaxed);
        } else {
            OBJECTS.fetch_sub(1, Ordering::Relaxed);
        }
        Ok(())
    }
}

#[implement(IExplorerCommand, IExecuteCommand, IObjectWithSelection)]
struct Verb {
    tool: Tool,
    files: Mutex<Vec<PathBuf>>,
}

impl Verb {
    fn new(tool: Tool) -> Self {
        OBJECTS.fetch_add(1, Ordering::Relaxed);
        Self {
            tool,
            files: Mutex::new(Vec::new()),
        }
    }
}

impl Drop for Verb {
    fn drop(&mut self) {
        OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl IExplorerCommand_Impl for Verb_Impl {
    fn GetTitle(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        guard(|| copy_wide(self.tool.title()))
    }

    fn GetIcon(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        guard(|| {
            let exe = install_dir()?.join("powerkit.exe");
            copy_wide(&format!("\"{}\",0", exe.display()))
        })
    }

    fn GetToolTip(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(self.tool.class_id())
    }

    fn GetState(&self, items: Ref<IShellItemArray>, _slow: BOOL) -> Result<u32> {
        guard(|| {
            let enabled = items
                .as_ref()
                .and_then(|items| item_at(items, 0))
                .and_then(|item| path_of(&item))
                .is_some_and(|path| self.tool.shows_for(&path));
            Ok(if enabled {
                ECS_ENABLED.0 as u32
            } else {
                ECS_HIDDEN.0 as u32
            })
        })
    }

    fn Invoke(&self, items: Ref<IShellItemArray>, _context: Ref<IBindCtx>) -> Result<()> {
        guard(|| {
            let Some(items) = items.as_ref() else {
                return Ok(());
            };
            launch(self.tool, &selected(self.tool, items))
        })
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

impl IExecuteCommand_Impl for Verb_Impl {
    fn SetKeyState(&self, _state: u32) -> Result<()> {
        Ok(())
    }

    fn SetParameters(&self, _parameters: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn SetPosition(&self, _point: &windows::Win32::Foundation::POINT) -> Result<()> {
        Ok(())
    }

    fn SetShowWindow(&self, _show: i32) -> Result<()> {
        Ok(())
    }

    fn SetNoShowUI(&self, _no_ui: BOOL) -> Result<()> {
        Ok(())
    }

    fn SetDirectory(&self, _directory: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn Execute(&self) -> Result<()> {
        guard(|| {
            let files = self
                .files
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            launch(self.tool, &files)
        })
    }
}

impl IObjectWithSelection_Impl for Verb_Impl {
    fn SetSelection(&self, items: Ref<IShellItemArray>) -> Result<()> {
        guard(|| {
            let files = items
                .as_ref()
                .map(|items| selected(self.tool, items))
                .unwrap_or_default();
            *self
                .files
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = files;
            Ok(())
        })
    }

    fn GetSelection(
        &self,
        _riid: *const GUID,
        _interface: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
}

fn guard<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(_) => Err(E_FAIL.into()),
    }
}

fn copy_wide(text: &str) -> Result<PWSTR> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: wide is NUL-terminated. SHStrDupW allocates a copy the caller frees.
    unsafe { SHStrDupW(PCWSTR(wide.as_ptr())) }
}

fn item_at(items: &IShellItemArray, index: u32) -> Option<IShellItem> {
    // SAFETY: index is bounded by the caller. GetItemAt returns an owned shell item.
    unsafe { items.GetItemAt(index) }.ok()
}

fn path_of(item: &IShellItem) -> Option<PathBuf> {
    // SAFETY: GetDisplayName returns a CoTaskMemAlloc string. It is copied and freed here.
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
    if name.0.is_null() {
        return None;
    }
    let mut units = Vec::new();
    unsafe {
        let mut cursor = name.0;
        while units.len() < 32_767 && *cursor != 0 {
            units.push(*cursor);
            cursor = cursor.add(1);
        }
        CoTaskMemFree(Some(name.0.cast()));
    }
    Some(PathBuf::from(OsString::from_wide(&units)))
}

fn perceived_image(path: &Path) -> Option<bool> {
    let Some(extension) = path.extension().filter(|extension| !extension.is_empty()) else {
        return Some(false);
    };
    let mut wide = Vec::with_capacity(16);
    wide.push(u16::from(b'.'));
    wide.extend(extension.encode_wide());
    wide.push(0);
    let mut perceived = PERCEIVED(0);
    let mut flags = 0u32;
    // SAFETY: wide is a NUL-terminated extension. The perceived-type outputs are writable.
    if unsafe { AssocGetPerceivedType(PCWSTR(wide.as_ptr()), &mut perceived, &mut flags, None) }
        .is_err()
    {
        return None;
    }
    Some(perceived.0 == PERCEIVED_TYPE_IMAGE.0)
}

fn selected(tool: Tool, items: &IShellItemArray) -> Vec<PathBuf> {
    let (_, _, _, limit) = tool.launch_flags();
    // SAFETY: GetCount only reads the selection the shell passed in.
    let count = unsafe { items.GetCount() }.unwrap_or(0).min(limit as u32);
    (0..count)
        .filter_map(|index| item_at(items, index))
        .filter_map(|item| path_of(&item))
        .filter(|path| tool.accepts(path))
        .collect()
}

fn install_dir() -> Result<PathBuf> {
    let module = MODULE.load(Ordering::Relaxed);
    if module == 0 {
        return Err(E_FAIL.into());
    }
    let mut buffer = vec![0u16; 32_768];
    // SAFETY: buffer is writable and belongs to this call. module is the DLL handle from DllMain.
    let length = unsafe {
        GetModuleFileNameW(
            Some(windows::Win32::Foundation::HMODULE(
                module as *mut core::ffi::c_void,
            )),
            &mut buffer,
        )
    } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(E_FAIL.into());
    }
    PathBuf::from(OsString::from_wide(&buffer[..length]))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| E_FAIL.into())
}

fn launch(tool: Tool, files: &[PathBuf]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let dir = install_dir()?;
    let exe = dir.join("powerkit.exe");
    if !exe.is_file() {
        return Err(E_FAIL.into());
    }
    let (_, list_flag, _, _) = tool.launch_flags();
    let mut command = inline_command(tool, &exe, files);
    let list = if command.len() > COMMAND_LIMIT {
        let list = write_list(tool, files).map_err(|_| windows::core::Error::from(E_FAIL))?;
        let args = [OsString::from(list_flag), list.as_os_str().to_os_string()];
        command = inline_command_args(&exe, &args);
        Some(list)
    } else {
        None
    };
    if command.len() > 32_767 {
        if let Some(list) = &list {
            let _ = std::fs::remove_file(list);
        }
        return Err(E_FAIL.into());
    }
    let directory: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..STARTUPINFOW::default()
    };
    let mut process = PROCESS_INFORMATION {
        hProcess: windows::Win32::Foundation::HANDLE::default(),
        hThread: windows::Win32::Foundation::HANDLE::default(),
        dwProcessId: 0,
        dwThreadId: 0,
    };
    // SAFETY: command, application and directory are NUL-terminated and live until CreateProcessW returns.
    // The new process is independent; both handles are closed below.
    let created = unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            windows::Win32::System::Threading::PROCESS_CREATION_FLAGS(0),
            None,
            PCWSTR(directory.as_ptr()),
            &startup,
            &mut process,
        )
    };
    if let Err(error) = created {
        if let Some(list) = list {
            let _ = std::fs::remove_file(list);
        }
        return Err(error);
    }
    // SAFETY: both handles were created by CreateProcessW and are not used again.
    unsafe {
        let _ = CloseHandle(process.hProcess);
        let _ = CloseHandle(process.hThread);
    }
    Ok(())
}

fn inline_command(tool: Tool, exe: &Path, files: &[PathBuf]) -> Vec<u16> {
    let (flag, _, _, _) = tool.launch_flags();
    let mut args = Vec::with_capacity(files.len() + 1);
    args.push(flag.into());
    args.extend(files.iter().map(|file| file.as_os_str().to_os_string()));
    inline_command_args(exe, &args)
}

fn inline_command_args(exe: &Path, args: &[OsString]) -> Vec<u16> {
    let mut units = Vec::new();
    push_quoted(&mut units, exe.as_os_str());
    for arg in args {
        units.push(u16::from(b' '));
        push_quoted(&mut units, arg);
    }
    units.push(0);
    units
}

fn push_quoted(out: &mut Vec<u16>, arg: &OsStr) {
    let wide: Vec<u16> = arg.encode_wide().collect();
    let quote = wide.is_empty()
        || wide.iter().any(|unit| {
            *unit == u16::from(b' ') || *unit == u16::from(b'\t') || *unit == u16::from(b'"')
        });
    if !quote {
        out.extend(wide);
        return;
    }
    out.push(u16::from(b'"'));
    let mut slashes = 0usize;
    for unit in wide {
        if unit == u16::from(b'\\') {
            slashes += 1;
            continue;
        }
        if unit == u16::from(b'"') {
            out.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2 + 1));
            out.push(u16::from(b'"'));
            slashes = 0;
            continue;
        }
        out.extend(std::iter::repeat_n(u16::from(b'\\'), slashes));
        slashes = 0;
        out.push(unit);
    }
    out.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2));
    out.push(u16::from(b'"'));
}

fn write_list(tool: Tool, files: &[PathBuf]) -> std::io::Result<PathBuf> {
    let (_, _, prefix, limit) = tool.launch_flags();
    let name = format!(
        "{prefix}-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    );
    let path = std::env::temp_dir().join(name);
    let mut bytes = vec![0xFF, 0xFE];
    for file in files.iter().take(limit) {
        for unit in file.as_os_str().encode_wide() {
            bytes.extend(unit.to_le_bytes());
        }
        bytes.extend(u16::from(b'\n').to_le_bytes());
    }
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// # Safety
/// `class`, `iid` and `object` follow the COM `DllGetClassObject` contract.
/// A non-null `object` is writable for one interface pointer. The host owns the
/// returned pointer and releases it.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    class: *const GUID,
    iid: *const GUID,
    object: *mut *mut core::ffi::c_void,
) -> HRESULT {
    let trapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if object.is_null() {
            return E_POINTER;
        }
        // SAFETY: object is non-null and writable for one pointer.
        unsafe { *object = core::ptr::null_mut() };
        if iid.is_null() {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        // SAFETY: a non-null class points at the caller's CLSID for this call.
        let Some(tool) = (!class.is_null())
            .then(|| unsafe { *class })
            .and_then(|class| Tool::from_class(&class))
        else {
            return CLASS_E_CLASSNOTAVAILABLE;
        };
        OBJECTS.fetch_add(1, Ordering::Relaxed);
        let factory: IUnknown = Factory(tool).into();
        // SAFETY: iid and object are non-null. Query writes the requested interface.
        unsafe { factory.query(iid, object) }
    }));
    trapped.unwrap_or(E_FAIL)
}

#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECTS.load(Ordering::Relaxed) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(
    module: windows::Win32::Foundation::HMODULE,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> i32 {
    if reason == 1 {
        MODULE.store(module.0 as isize, Ordering::Relaxed);
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_ids_match_the_shared_shell_guids() {
        assert_eq!(
            Tool::Resize.class_id(),
            GUID::try_from("B7E4C1A9-2F63-4D85-A1C0-9E8D7B6A5432").unwrap()
        );
        assert_eq!(
            Tool::Rename.class_id(),
            GUID::try_from(include_str!("../../../packaging/msix/shell_clsid_rename.txt").trim())
                .unwrap()
        );
        assert_ne!(Tool::Rename.class_id(), GUID::from_u128(0));
        for tool in Tool::ALL {
            assert_eq!(Tool::from_class(&tool.class_id()), Some(tool));
        }
        assert_eq!(Tool::from_class(&GUID::from_u128(0)), None);
    }

    #[test]
    fn rename_accepts_folders_and_media_only() {
        assert!(Tool::Rename.accepts(&std::env::temp_dir()));
        assert!(Tool::Rename.accepts(Path::new(r"C:\x\IMG_1.HEIC")));
        assert!(Tool::Rename.accepts(Path::new(r"C:\x\clip.mov")));
        assert!(!Tool::Rename.accepts(Path::new(r"C:\x\IMG_1.xmp")));
        assert!(!Tool::Rename.accepts(Path::new(r"C:\x\notes.txt")));
        let units = inline_command(
            Tool::Rename,
            Path::new(r"C:\PowerKit\powerkit.exe"),
            &[PathBuf::from(r"C:\My Photos")],
        );
        let text = String::from_utf16(&units[..units.len() - 1]).unwrap();
        assert_eq!(text, r#"C:\PowerKit\powerkit.exe --rename "C:\My Photos""#);
    }

    #[test]
    fn command_line_quotes_only_arguments_that_need_it() {
        let units = inline_command(
            Tool::Resize,
            Path::new(r"C:\Program Files\PowerKit\powerkit.exe"),
            &[
                PathBuf::from(r"C:\photos\a.jpg"),
                PathBuf::from(r"C:\My Photos\b.jpg"),
            ],
        );
        let text = String::from_utf16(&units[..units.len() - 1]).unwrap();
        assert_eq!(
            text,
            r#""C:\Program Files\PowerKit\powerkit.exe" --resize C:\photos\a.jpg "C:\My Photos\b.jpg""#
        );
    }

    #[test]
    fn a_long_selection_does_not_fit_the_command_line() {
        let files: Vec<_> = (0..400)
            .map(|index| PathBuf::from(format!(r"C:\photos\{index:04}-{}.jpg", "n".repeat(80))))
            .collect();
        let units = inline_command(Tool::Resize, Path::new(r"C:\PowerKit\powerkit.exe"), &files);
        assert!(units.len() > COMMAND_LIMIT);
    }
}
