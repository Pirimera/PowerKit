//! MSIX package identity. Packaged installs declare the startup task and Explorer
//! verbs in `packaging/msix/AppxManifest.xml`; unpackaged builds register them per user.
use std::{ptr::null_mut, sync::OnceLock};
use windows_sys::Win32::{
    Foundation::APPMODEL_ERROR_NO_PACKAGE, Storage::Packaging::Appx::GetCurrentPackageFullName,
};

/// Whether this process runs with package identity (installed MSIX).
pub(super) fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut length = 0u32;
        // SAFETY: a zero length with a null buffer only queries the required size.
        let status = unsafe { GetCurrentPackageFullName(&mut length, null_mut()) };
        status != APPMODEL_ERROR_NO_PACKAGE
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_runner_has_no_package_identity() {
        assert!(!super::is_packaged());
    }
}
