//! In-process Explorer command for Image Resizer.
//!
//! Windows 11 loads this DLL through a sparse-package surrogate. The classic
//! verb loads it in-process. Both paths collect the selected image paths and
//! start `powerkit.exe` beside this DLL. COM is owned by the host thread.

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

const TITLE: &str = "Image Resizer ile yeniden boyutlandır";
const COMMAND_LIMIT: usize = 32_000;
static OBJECTS: AtomicI32 = AtomicI32::new(0);
static MODULE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn class_id() -> GUID {
    static ID: OnceLock<GUID> = OnceLock::new();
    *ID.get_or_init(|| {
        GUID::try_from(include_str!("shell_clsid.txt").trim())
            .unwrap_or_else(|_| GUID::from_u128(0))
    })
}

#[implement(IClassFactory)]
struct Factory;

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
        let verb: IUnknown = Verb::new().into();
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
    files: Mutex<Vec<PathBuf>>,
}

impl Verb {
    fn new() -> Self {
        OBJECTS.fetch_add(1, Ordering::Relaxed);
        Self {
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
        guard(|| copy_wide(TITLE))
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
        Ok(class_id())
    }

    fn GetState(&self, items: Ref<IShellItemArray>, _slow: BOOL) -> Result<u32> {
        guard(|| {
            let enabled = items
                .as_ref()
                .and_then(|items| item_at(items, 0))
                .and_then(|item| path_of(&item))
                .is_some_and(|path| perceived_image(&path) == Some(true));
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
            launch(&selected_images(items))
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
            launch(&files)
        })
    }
}

impl IObjectWithSelection_Impl for Verb_Impl {
    fn SetSelection(&self, items: Ref<IShellItemArray>) -> Result<()> {
        guard(|| {
            let files = items.as_ref().map(selected_images).unwrap_or_default();
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

fn selected_images(items: &IShellItemArray) -> Vec<PathBuf> {
    // SAFETY: GetCount only reads the selection the shell passed in.
    let count = unsafe { items.GetCount() }.unwrap_or(0).min(1_000);
    (0..count)
        .filter_map(|index| item_at(items, index))
        .filter_map(|item| path_of(&item))
        .filter(|path| perceived_image(path) != Some(false))
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

fn launch(files: &[PathBuf]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let dir = install_dir()?;
    let exe = dir.join("powerkit.exe");
    if !exe.is_file() {
        return Err(E_FAIL.into());
    }
    let mut command = inline_command(&exe, files);
    let list = if command.len() > COMMAND_LIMIT {
        let list = write_list(files).map_err(|_| windows::core::Error::from(E_FAIL))?;
        let args = [
            OsString::from("--resize-list"),
            list.as_os_str().to_os_string(),
        ];
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

fn inline_command(exe: &Path, files: &[PathBuf]) -> Vec<u16> {
    let mut args = Vec::with_capacity(files.len() + 1);
    args.push("--resize".into());
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

fn write_list(files: &[PathBuf]) -> std::io::Result<PathBuf> {
    let name = format!(
        "powerkit-resize-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    );
    let path = std::env::temp_dir().join(name);
    let mut bytes = vec![0xFF, 0xFE];
    for file in files.iter().take(1_000) {
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
        if class.is_null()
            || iid.is_null()
            || unsafe { *class } != class_id()
            || class_id() == GUID::from_u128(0)
        {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        OBJECTS.fetch_add(1, Ordering::Relaxed);
        let factory: IUnknown = Factory.into();
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
    fn class_id_matches_the_shared_shell_guid() {
        assert_eq!(
            class_id(),
            GUID::try_from("B7E4C1A9-2F63-4D85-A1C0-9E8D7B6A5432").unwrap()
        );
    }

    #[test]
    fn command_line_quotes_only_arguments_that_need_it() {
        let units = inline_command(
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
        let units = inline_command(Path::new(r"C:\PowerKit\powerkit.exe"), &files);
        assert!(units.len() > COMMAND_LIMIT);
    }
}
