//! MSIX package identity. Packaged installs declare the startup task and Explorer
//! verbs in `packaging/msix/AppxManifest.xml`; unpackaged builds register them per user.
use std::{mem::size_of, ptr::null_mut, sync::OnceLock};
use windows_sys::Win32::{
    Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
    Storage::Packaging::Appx::{GetCurrentPackageFullName, GetCurrentPackageId, PACKAGE_ID},
};

/// Fourth part of the installed MSIX version: 0 for a Store package and for an
/// unpackaged build, the `-Revision` of `scripts/Package.ps1` for a local test package.
pub fn revision() -> u16 {
    static REVISION: OnceLock<u16> = OnceLock::new();
    *REVISION.get_or_init(|| current_package_version().map_or(0, |version| version as u16))
}

/// The package version as `Major.Minor.Build.Revision`, 16 bits each, Major highest.
fn current_package_version() -> Option<u64> {
    let mut length = 0u32;
    // SAFETY: a zero length with a null buffer only queries the required size.
    let status = unsafe { GetCurrentPackageId(&mut length, null_mut()) };
    if status != ERROR_INSUFFICIENT_BUFFER || (length as usize) < size_of::<PACKAGE_ID>() {
        return None;
    }
    // u64 elements keep the buffer aligned for PACKAGE_ID, which holds pointers.
    let mut buffer = vec![0u64; (length as usize).div_ceil(size_of::<u64>())];
    // SAFETY: the buffer is writable for at least `length` bytes and outlives the call.
    let status = unsafe { GetCurrentPackageId(&mut length, buffer.as_mut_ptr().cast()) };
    if status != ERROR_SUCCESS {
        return None;
    }
    // SAFETY: on success the buffer starts with an initialized PACKAGE_ID; it is
    // aligned (see above) and at least that large (checked above). Every bit
    // pattern is a valid u64, so reading the version union as one is sound.
    let id = unsafe { &*buffer.as_ptr().cast::<PACKAGE_ID>() };
    Some(unsafe { id.version.Anonymous.Version })
}

/// Whether this process runs with package identity (installed MSIX).
pub fn is_packaged() -> bool {
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
        assert_eq!(super::revision(), 0);
    }
}
