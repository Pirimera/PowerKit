//! Best-effort install scope and dates from the Windows Uninstall registry and install folders.
//!
//! WinGet's `CatalogPackage` exposes `ProductCodes` and `InstalledLocation` on `InstalledVersion`,
//! while `InstallDate` and registry key modification timestamps live in the Uninstall hives.
use super::super::{clock, wide};
use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey,
        RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
    },
};

const UNINSTALL_MAIN: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
const UNINSTALL_WOW64: &str =
    "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall";

#[derive(Clone, Debug, Default)]
pub struct InstalledMeta {
    pub scope: Option<String>,
    pub install_date: Option<String>,
    pub updated_at: Option<String>,
    /// Raw `DisplayIcon` value (`path[,index]`), possibly quoted.
    pub icon: Option<String>,
}

#[derive(Default)]
pub struct RegistryIndex {
    by_code: HashMap<String, InstalledMeta>,
    by_name: HashMap<String, InstalledMeta>,
}

impl RegistryIndex {
    pub fn collect() -> Self {
        let mut index = Self::default();
        index.scan_hive(HKEY_CURRENT_USER, UNINSTALL_MAIN, "user");
        index.scan_hive(HKEY_LOCAL_MACHINE, UNINSTALL_MAIN, "machine");
        index.scan_hive(HKEY_LOCAL_MACHINE, UNINSTALL_WOW64, "machine");
        index
    }

    pub fn lookup(
        &self,
        product_codes: &[String],
        display_name: &str,
        install_location: Option<&str>,
    ) -> InstalledMeta {
        let from_registry = product_codes
            .iter()
            .find_map(|code| self.by_code.get(&code.to_lowercase()))
            .or_else(|| {
                let key = display_name.trim().to_lowercase();
                (!key.is_empty()).then(|| self.by_name.get(&key)).flatten()
            })
            .cloned()
            .unwrap_or_default();

        let (folder_created, folder_modified) = install_location
            .filter(|path| !path.trim().is_empty())
            .and_then(|path| std::fs::metadata(Path::new(path)).ok())
            .map(|meta| {
                (
                    clock::local_date(clock::file_time(meta.creation_time())),
                    clock::local_date(clock::file_time(meta.last_write_time())),
                )
            })
            .unwrap_or((None, None));

        let install_date = from_registry.install_date.or(folder_created);
        let updated_at = from_registry
            .updated_at
            .or(folder_modified)
            .or_else(|| install_date.clone());
        InstalledMeta {
            scope: from_registry.scope,
            install_date,
            updated_at,
            icon: from_registry.icon,
        }
    }

    fn scan_hive(&mut self, hive: HKEY, subkey: &str, scope: &'static str) {
        let path = wide(subkey);
        let mut root: HKEY = null_mut();
        // SAFETY: NUL-terminated UTF-16 path; `root` is closed before returning.
        if unsafe { RegOpenKeyExW(hive, path.as_ptr(), 0, KEY_READ, &mut root) } != 0
            || root.is_null()
        {
            return;
        }
        let install_date_name = wide("InstallDate");
        let display_name_name = wide("DisplayName");
        let display_icon_name = wide("DisplayIcon");
        let mut index = 0u32;
        let mut name_buf = [0u16; 260];
        loop {
            let mut len = (name_buf.len() - 1) as u32;
            let mut last_write = FILETIME::default();
            // SAFETY: `root` is an open key handle; `name_buf` has room for `len + 1` UTF-16 units.
            let status = unsafe {
                RegEnumKeyExW(
                    root,
                    index,
                    name_buf.as_mut_ptr(),
                    &mut len,
                    null(),
                    null_mut(),
                    null_mut(),
                    &mut last_write,
                )
            };
            if status != 0 {
                break;
            }
            index += 1;
            name_buf[len as usize] = 0;
            let code = String::from_utf16_lossy(&name_buf[..len as usize]);
            let raw_install = read_reg_sz(root, &name_buf, &install_date_name);
            let display_name = read_reg_sz(root, &name_buf, &display_name_name);
            let install_date = raw_install.as_deref().and_then(parse_yyyymmdd);
            let updated_at =
                clock::filetime_local_date(last_write).or_else(|| install_date.clone());
            let entry = InstalledMeta {
                scope: Some(scope.to_string()),
                install_date,
                updated_at,
                icon: read_reg_sz(root, &name_buf, &display_icon_name),
            };
            self.by_code
                .entry(code.to_lowercase())
                .or_insert_with(|| entry.clone());
            if let Some(name) = display_name {
                let normalized = name.trim().to_lowercase();
                if !normalized.is_empty() {
                    self.by_name.entry(normalized).or_insert(entry);
                }
            }
        }
        // SAFETY: `root` was opened by `RegOpenKeyExW` above and is not used afterwards.
        unsafe {
            RegCloseKey(root);
        }
    }
}

fn read_reg_sz(root: HKEY, subkey: &[u16], value_name: &[u16]) -> Option<String> {
    let mut buf = [0u16; 260];
    let mut bytes = (buf.len() * size_of::<u16>()) as u32;
    // SAFETY: `subkey` and `value_name` are NUL-terminated; `buf` holds `bytes` bytes.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != 0 || bytes < 2 {
        return None;
    }
    let units = (bytes as usize / 2).saturating_sub(1);
    let text = String::from_utf16_lossy(&buf[..units]);
    let trimmed = text.trim_end_matches('\0').trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub fn parse_yyyymmdd(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() != 8 || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: u16 = trimmed[0..4].parse().ok()?;
    let month: u8 = trimmed[4..6].parse().ok()?;
    let day: u8 = trimmed[6..8].parse().ok()?;
    if !(1980..=2100).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yyyymmdd_parses_valid_dates_and_rejects_junk() {
        assert_eq!(parse_yyyymmdd("20261116"), Some("2026-11-16".into()));
        assert_eq!(parse_yyyymmdd(" 20240229 "), Some("2024-02-29".into()));
        assert_eq!(parse_yyyymmdd("20261301"), None);
        assert_eq!(parse_yyyymmdd("2026-11-16"), None);
        assert_eq!(parse_yyyymmdd(""), None);
    }
}
