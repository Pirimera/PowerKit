//! WinGet Packaged COM client (`Microsoft.Management.Deployment`).

#[allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    dead_code,
    unused_imports,
    clippy::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/winget_bindings.rs"));
}

use crate::power_store::{ManagerKind, OperationKind, PackageKey, PackageSummary, QueuedOperation};
use bindings::Microsoft::Management::Deployment::{
    CatalogPackage, CompositeSearchBehavior, ConnectResultStatus,
    CreateCompositePackageCatalogOptions, FindPackagesOptions, FindPackagesResultStatus,
    InstallOptions, InstallResultStatus, MatchResult, PackageFieldMatchOption, PackageInstallScope,
    PackageManager, PackageMatchField, PackageMatchFilter, UninstallOptions, UninstallResultStatus,
};
use std::fmt;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows_core::{GUID, HSTRING, IUnknown, Interface, h};

const PACKAGE_MANAGER: GUID = GUID::from_u128(0xC53A4F16_787E_42A4_B304_29EFFB4BF597);
const FIND_PACKAGES_OPTIONS: GUID = GUID::from_u128(0x572DED96_9C60_4526_8F92_EE7D91D38C1A);
const CREATE_COMPOSITE_OPTIONS: GUID = GUID::from_u128(0x526534B8_7E46_47C8_8416_B1685C327D37);
const PACKAGE_MATCH_FILTER: GUID = GUID::from_u128(0xD02C9DAF_99DC_429C_B503_4E504E4AB000);
const INSTALL_OPTIONS: GUID = GUID::from_u128(0x1095F097_EB96_453B_B4E6_1613637F3B14);
const UNINSTALL_OPTIONS: GUID = GUID::from_u128(0xE1D9A11E_9F85_4D87_9C17_2B93143ADB8D);

/// HRESULT `0x80073D54` — winmd missing beside the executable, or proxy catalog gap.
const PROXY_NOT_FOUND: windows_core::HRESULT = windows_core::HRESULT(0x80073D54_u32 as i32);

#[derive(Debug)]
pub enum WinGetError {
    Unavailable(String),
    Operation(String),
}

impl fmt::Display for WinGetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Operation(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for WinGetError {}

impl From<windows_core::Error> for WinGetError {
    fn from(error: windows_core::Error) -> Self {
        if error.code() == PROXY_NOT_FOUND {
            return Self::Unavailable(
                "WinGet COM metadata missing beside powerkit.exe (Microsoft.Management.Deployment.winmd)"
                    .into(),
            );
        }
        let message = error.message();
        if message.is_empty() {
            Self::Operation(format!("WinGet COM error {:#010x}", error.code().0 as u32))
        } else {
            Self::Operation(message)
        }
    }
}

/// RAII COM apartment for a worker thread that owns WinGet objects.
struct ComApartment;

impl ComApartment {
    fn enter() -> Result<Self, WinGetError> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(WinGetError::from)?;
        }
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn create_instance<T: Interface>(clsid: &GUID) -> Result<T, WinGetError> {
    unsafe {
        let object: IUnknown = CoCreateInstance(clsid, None, CLSCTX_LOCAL_SERVER)?;
        Ok(object.cast()?)
    }
}

fn hstring_to_string(value: &HSTRING) -> String {
    value.to_string()
}

fn catalog_package_summary(package: &CatalogPackage) -> Result<PackageSummary, WinGetError> {
    let id = hstring_to_string(&package.Id()?);
    let name = hstring_to_string(&package.Name()?);
    let version = package
        .InstalledVersion()
        .ok()
        .and_then(|version| version.Version().ok())
        .map(|version| hstring_to_string(&version));
    let available_version = package
        .DefaultInstallVersion()
        .ok()
        .and_then(|version| version.Version().ok())
        .map(|version| hstring_to_string(&version));
    let source = package
        .DefaultInstallVersion()
        .ok()
        .and_then(|version| version.PackageCatalog().ok())
        .and_then(|catalog| catalog.Info().ok())
        .and_then(|info| info.Name().ok())
        .map(|name| hstring_to_string(&name))
        .unwrap_or_else(|| "winget".into());
    Ok(PackageSummary {
        key: PackageKey::new(ManagerKind::WinGet, source, id),
        name,
        version,
        available_version,
    })
}

fn read_matches(
    matches: &windows_collections::IVectorView<MatchResult>,
) -> Result<Vec<PackageSummary>, WinGetError> {
    let count = matches.Size()?;
    let mut packages = Vec::with_capacity(count as usize);
    for index in 0..count {
        let package = matches.GetAt(index)?.CatalogPackage()?;
        packages.push(catalog_package_summary(&package)?);
    }
    Ok(packages)
}

/// Thin WinGet catalog client. Create on a worker thread; do not share across threads.
pub struct WinGetSource {
    _apartment: ComApartment,
    manager: PackageManager,
}

impl WinGetSource {
    pub fn connect() -> Result<Self, WinGetError> {
        let apartment = ComApartment::enter()?;
        let manager = create_instance::<PackageManager>(&PACKAGE_MANAGER).map_err(|error| {
            if matches!(error, WinGetError::Operation(_)) {
                WinGetError::Unavailable(format!("WinGet / App Installer is unavailable ({error})"))
            } else {
                error
            }
        })?;
        Ok(Self {
            _apartment: apartment,
            manager,
        })
    }

    pub fn search_equals_id(&self, package_id: &str) -> Result<Vec<PackageSummary>, WinGetError> {
        self.search(
            PackageMatchField::Id,
            PackageFieldMatchOption::Equals,
            package_id,
        )
    }

    pub fn search_query(&self, query: &str) -> Result<Vec<PackageSummary>, WinGetError> {
        self.search(
            PackageMatchField::Name,
            PackageFieldMatchOption::ContainsCaseInsensitive,
            query,
        )
    }

    fn search(
        &self,
        field: PackageMatchField,
        option: PackageFieldMatchOption,
        value: &str,
    ) -> Result<Vec<PackageSummary>, WinGetError> {
        let winget = self.manager.GetPackageCatalogByName(h!("winget"))?;
        let connect = winget.Connect()?;
        if connect.Status()? != ConnectResultStatus::Ok {
            return Err(WinGetError::Operation(format!(
                "WinGet catalog connect failed ({:?})",
                connect.Status()?
            )));
        }
        let find_options = create_instance::<FindPackagesOptions>(&FIND_PACKAGES_OPTIONS)?;
        let filter = create_instance::<PackageMatchFilter>(&PACKAGE_MATCH_FILTER)?;
        filter.SetField(field)?;
        filter.SetOption(option)?;
        filter.SetValue(&HSTRING::from(value))?;
        find_options.Selectors()?.Append(&filter)?;
        let find = connect.PackageCatalog()?.FindPackages(&find_options)?;
        if find.Status()? != FindPackagesResultStatus::Ok {
            return Err(WinGetError::Operation(format!(
                "WinGet search failed ({:?})",
                find.Status()?
            )));
        }
        read_matches(&find.Matches()?)
    }

    pub fn list_installed(&self) -> Result<Vec<PackageSummary>, WinGetError> {
        let winget = self.manager.GetPackageCatalogByName(h!("winget"))?;
        let msstore = self.manager.GetPackageCatalogByName(h!("msstore"))?;
        let options =
            create_instance::<CreateCompositePackageCatalogOptions>(&CREATE_COMPOSITE_OPTIONS)?;
        options.Catalogs()?.Append(&winget)?;
        options.Catalogs()?.Append(&msstore)?;
        options.SetCompositeSearchBehavior(CompositeSearchBehavior::LocalCatalogs)?;
        options.SetInstalledScope(PackageInstallScope::Any)?;
        let connect = self
            .manager
            .CreateCompositePackageCatalog(&options)?
            .Connect()?;
        if connect.Status()? != ConnectResultStatus::Ok {
            return Err(WinGetError::Operation(format!(
                "WinGet installed catalog connect failed ({:?})",
                connect.Status()?
            )));
        }
        let find_options = create_instance::<FindPackagesOptions>(&FIND_PACKAGES_OPTIONS)?;
        let find = connect.PackageCatalog()?.FindPackages(&find_options)?;
        if find.Status()? != FindPackagesResultStatus::Ok {
            return Err(WinGetError::Operation(format!(
                "WinGet installed listing failed ({:?})",
                find.Status()?
            )));
        }
        read_matches(&find.Matches()?)
    }

    /// Resolve a catalog package for install/upgrade (remote) or uninstall (installed).
    fn find_package(
        &self,
        package_id: &str,
        installed: bool,
    ) -> Result<CatalogPackage, WinGetError> {
        let catalog = if installed {
            let winget = self.manager.GetPackageCatalogByName(h!("winget"))?;
            let msstore = self.manager.GetPackageCatalogByName(h!("msstore"))?;
            let options =
                create_instance::<CreateCompositePackageCatalogOptions>(&CREATE_COMPOSITE_OPTIONS)?;
            options.Catalogs()?.Append(&winget)?;
            options.Catalogs()?.Append(&msstore)?;
            options.SetCompositeSearchBehavior(CompositeSearchBehavior::LocalCatalogs)?;
            options.SetInstalledScope(PackageInstallScope::Any)?;
            let connect = self
                .manager
                .CreateCompositePackageCatalog(&options)?
                .Connect()?;
            if connect.Status()? != ConnectResultStatus::Ok {
                return Err(WinGetError::Operation(format!(
                    "WinGet installed catalog connect failed ({:?})",
                    connect.Status()?
                )));
            }
            connect.PackageCatalog()?
        } else {
            let winget = self.manager.GetPackageCatalogByName(h!("winget"))?;
            let connect = winget.Connect()?;
            if connect.Status()? != ConnectResultStatus::Ok {
                return Err(WinGetError::Operation(format!(
                    "WinGet catalog connect failed ({:?})",
                    connect.Status()?
                )));
            }
            connect.PackageCatalog()?
        };
        let find_options = create_instance::<FindPackagesOptions>(&FIND_PACKAGES_OPTIONS)?;
        let filter = create_instance::<PackageMatchFilter>(&PACKAGE_MATCH_FILTER)?;
        filter.SetField(PackageMatchField::Id)?;
        filter.SetOption(PackageFieldMatchOption::Equals)?;
        filter.SetValue(&HSTRING::from(package_id))?;
        find_options.Selectors()?.Append(&filter)?;
        let find = catalog.FindPackages(&find_options)?;
        if find.Status()? != FindPackagesResultStatus::Ok {
            return Err(WinGetError::Operation(format!(
                "WinGet package lookup failed ({:?})",
                find.Status()?
            )));
        }
        let matches = find.Matches()?;
        if matches.Size()? == 0 {
            return Err(WinGetError::Operation(format!(
                "package not found: {package_id}"
            )));
        }
        Ok(matches.GetAt(0)?.CatalogPackage()?)
    }

    pub fn run_operation(&self, operation: &QueuedOperation) -> Result<(), WinGetError> {
        match operation.kind {
            OperationKind::Install => {
                let package = self.find_package(&operation.key.id, false)?;
                let options = create_instance::<InstallOptions>(&INSTALL_OPTIONS)?;
                let async_op = self.manager.InstallPackageAsync(&package, &options)?;
                let result = async_op.join()?;
                if result.Status()? != InstallResultStatus::Ok {
                    return Err(WinGetError::Operation(format!(
                        "install failed ({:?})",
                        result.Status()?
                    )));
                }
            }
            OperationKind::Update => {
                let package = self.find_package(&operation.key.id, false)?;
                let options = create_instance::<InstallOptions>(&INSTALL_OPTIONS)?;
                let async_op = self.manager.UpgradePackageAsync(&package, &options)?;
                let result = async_op.join()?;
                if result.Status()? != InstallResultStatus::Ok {
                    return Err(WinGetError::Operation(format!(
                        "upgrade failed ({:?})",
                        result.Status()?
                    )));
                }
            }
            OperationKind::Uninstall => {
                let package = self.find_package(&operation.key.id, true)?;
                let options = create_instance::<UninstallOptions>(&UNINSTALL_OPTIONS)?;
                let async_op = self.manager.UninstallPackageAsync(&package, &options)?;
                let result = async_op.join()?;
                if result.Status()? != UninstallResultStatus::Ok {
                    return Err(WinGetError::Operation(format!(
                        "uninstall failed ({:?})",
                        result.Status()?
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn stage_winmd_beside_test_exe() {
        let exe = std::env::current_exe().expect("current test exe");
        let dest = exe.with_file_name("Microsoft.Management.Deployment.winmd");
        if dest.is_file() {
            return;
        }
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("third_party/winget/Microsoft.Management.Deployment.winmd");
        std::fs::copy(&src, &dest).unwrap_or_else(|error| {
            panic!("copy {} -> {}: {error}", src.display(), dest.display())
        });
    }

    #[test]
    #[ignore = "touches the live WinGet COM server; run with --ignored on a machine with App Installer"]
    fn live_search_and_installed_listing() {
        stage_winmd_beside_test_exe();
        let source = WinGetSource::connect().expect("WinGet COM connect");
        let matches = source
            .search_equals_id("Microsoft.WindowsTerminal")
            .expect("search Windows Terminal");
        assert!(
            matches
                .iter()
                .any(|package| package.key.id == "Microsoft.WindowsTerminal"),
            "expected Microsoft.WindowsTerminal in search results: {matches:?}"
        );
        let installed = source.list_installed().expect("list installed");
        assert!(
            !installed.is_empty(),
            "expected at least one installed package"
        );
    }
}
