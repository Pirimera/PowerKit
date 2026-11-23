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

use super::arp::RegistryIndex;
use bindings::Microsoft::Management::Deployment::{
    CatalogPackage, CompositeSearchBehavior, ConnectResultStatus,
    CreateCompositePackageCatalogOptions, FindPackagesOptions, FindPackagesResultStatus,
    InstallOptions, InstallProgress, InstallResultStatus, MatchResult, PackageCatalog,
    PackageFieldMatchOption, PackageInstallProgressState, PackageInstallScope, PackageManager,
    PackageMatchField, PackageMatchFilter, PackageUninstallProgressState, PackageUninstallScope,
    PackageVersionMetadataField, UninstallOptions, UninstallProgress, UninstallResultStatus,
};
use bindings::Windows::System::ProcessorArchitecture;
use powerkit_core::power_store::{
    InstallOptions as PackageOptions, ManagerKind, OperationKind, OperationProgress,
    PackageDetails, PackageKey, PackageSummary, ProgressStage, QueuedOperation,
    WINGET_UNAVAILABLE_MISSING, WINGET_UNAVAILABLE_TOO_OLD,
};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows_core::{GUID, HSTRING, IUnknown, Interface, Ref, RuntimeType, h};
use windows_future::{AsyncOperationProgressHandler, AsyncStatus, IAsyncOperationWithProgress};

const PACKAGE_MANAGER: GUID = GUID::from_u128(0xC53A4F16_787E_42A4_B304_29EFFB4BF597);
const FIND_PACKAGES_OPTIONS: GUID = GUID::from_u128(0x572DED96_9C60_4526_8F92_EE7D91D38C1A);
const CREATE_COMPOSITE_OPTIONS: GUID = GUID::from_u128(0x526534B8_7E46_47C8_8416_B1685C327D37);
const PACKAGE_MATCH_FILTER: GUID = GUID::from_u128(0xD02C9DAF_99DC_429C_B503_4E504E4AB000);
const INSTALL_OPTIONS: GUID = GUID::from_u128(0x1095F097_EB96_453B_B4E6_1613637F3B14);
const UNINSTALL_OPTIONS: GUID = GUID::from_u128(0xE1D9A11E_9F85_4D87_9C17_2B93143ADB8D);

const WINMD_NAME: &str = "Microsoft.Management.Deployment.winmd";
/// HRESULT `0x80073D54` — winmd missing beside the executable, or proxy catalog gap.
const PROXY_NOT_FOUND: windows_core::HRESULT = windows_core::HRESULT(0x80073D54_u32 as i32);
/// HRESULT `0x80040154` — App Installer / Packaged COM class not registered.
const CLASS_NOT_REGISTERED: windows_core::HRESULT = windows_core::HRESULT(0x80040154_u32 as i32);
/// HRESULT `0x800704C7` — `ERROR_CANCELLED`.
const CANCELLED: windows_core::HRESULT = windows_core::HRESULT(0x800704C7_u32 as i32);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum ManagerError {
    Unavailable(String),
    Operation(String),
    Cancelled,
}

impl fmt::Display for ManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Operation(message) => f.write_str(message),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for ManagerError {}

impl From<windows_core::Error> for ManagerError {
    fn from(error: windows_core::Error) -> Self {
        if error.code() == PROXY_NOT_FOUND {
            return Self::Unavailable(format!(
                "WinGet COM metadata missing beside the running executable ({WINMD_NAME})"
            ));
        }
        if error.code() == CLASS_NOT_REGISTERED {
            // Prefer `WinGetSource::connect`, which classifies missing vs too-old
            // after the COM apartment is entered. Other call sites keep a generic
            // unavailable string.
            return Self::Unavailable(
                "WinGet / App Installer COM class is not registered on this machine".into(),
            );
        }
        if error.code() == CANCELLED {
            return Self::Cancelled;
        }
        let message = error.message();
        if message.is_empty() {
            Self::Operation(format!("WinGet COM error {:#010x}", error.code().0 as u32))
        } else {
            Self::Operation(message)
        }
    }
}

const APP_INSTALLER_FAMILY: &str = "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe";

/// Distinguish a missing App Installer package from one that is present but too
/// old (or incomplete) for the pinned WinGet Packaged COM surface. No numeric
/// minimum version is pinned; presence plus `CLASS_NOT_REGISTERED` means too old.
fn classify_app_installer_gap() -> ManagerError {
    if desktop_app_installer_present() {
        ManagerError::Unavailable(WINGET_UNAVAILABLE_TOO_OLD.into())
    } else {
        ManagerError::Unavailable(WINGET_UNAVAILABLE_MISSING.into())
    }
}

fn desktop_app_installer_present() -> bool {
    let Ok(manager) = windows::Management::Deployment::PackageManager::new() else {
        return false;
    };
    let Ok(packages) =
        manager.FindPackagesByPackageFamilyName(&HSTRING::from(APP_INSTALLER_FAMILY))
    else {
        return false;
    };
    let Ok(iterator) = packages.First() else {
        return false;
    };
    iterator.HasCurrent().unwrap_or(false)
}

/// WinRT proxy loading looks next to the process image. build.rs copies the
/// vendored winmd beside development builds; the MSIX stages it in the package root.
fn ensure_winmd_beside_exe() -> Result<(), ManagerError> {
    let exe = std::env::current_exe().map_err(|error| {
        ManagerError::Operation(format!("current executable path unavailable: {error}"))
    })?;
    if exe.with_file_name(WINMD_NAME).is_file() {
        return Ok(());
    }
    Err(ManagerError::Unavailable(format!(
        "WinGet COM metadata ({WINMD_NAME}) not found beside {}",
        exe.display()
    )))
}

/// RAII COM apartment for a worker thread that owns WinGet objects.
struct ComApartment;

impl ComApartment {
    fn enter() -> Result<Self, ManagerError> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(ManagerError::from)?;
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

fn create_instance<T: Interface>(clsid: &GUID) -> Result<T, ManagerError> {
    unsafe {
        let object: IUnknown = CoCreateInstance(clsid, None, CLSCTX_LOCAL_SERVER)?;
        Ok(object.cast()?)
    }
}

fn hstring_to_string(value: &HSTRING) -> String {
    value.to_string()
}

fn normalize_scope(raw: &str) -> Option<String> {
    if raw.eq_ignore_ascii_case("user") {
        Some("user".into())
    } else if raw.eq_ignore_ascii_case("machine") || raw.eq_ignore_ascii_case("system") {
        Some("machine".into())
    } else {
        None
    }
}

fn catalog_package_summary(
    package: &CatalogPackage,
    registry: Option<&RegistryIndex>,
) -> Result<PackageSummary, ManagerError> {
    // Some ARP / framework rows throw on Name or version accessors (for example a
    // stale Windows App Runtime identity). Keep Id hard and soft-fail the rest.
    let id = hstring_to_string(&package.Id()?);
    let name = package
        .Name()
        .ok()
        .map(|name| hstring_to_string(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.clone());
    let installed_info = package.InstalledVersion().ok();
    let version = installed_info
        .as_ref()
        .and_then(|info| info.Version().ok())
        .map(|version| hstring_to_string(&version))
        .filter(|version| !version.is_empty());
    let available_version = package
        .DefaultInstallVersion()
        .ok()
        .and_then(|version| version.Version().ok())
        .map(|version| hstring_to_string(&version))
        .filter(|version| !version.is_empty());
    let source = package
        .DefaultInstallVersion()
        .ok()
        .and_then(|version| version.PackageCatalog().ok())
        .and_then(|catalog| catalog.Info().ok())
        .and_then(|info| info.Name().ok())
        .map(|name| hstring_to_string(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            if version.is_some() {
                "local".into()
            } else {
                "winget".into()
            }
        });
    let update_available = version.is_some() && package.IsUpdateAvailable().unwrap_or(false);

    let (scope, install_date, updated_at, icon) = match (&installed_info, registry) {
        (Some(info), Some(index)) => {
            let winget_scope = info
                .GetMetadata(PackageVersionMetadataField::InstalledScope)
                .ok()
                .and_then(|raw| normalize_scope(&hstring_to_string(&raw)));
            let location = info
                .GetMetadata(PackageVersionMetadataField::InstalledLocation)
                .ok()
                .map(|raw| hstring_to_string(&raw))
                .filter(|raw| !raw.is_empty());
            let mut codes = Vec::new();
            if let Ok(list) = info.ProductCodes()
                && let Ok(len) = list.Size()
            {
                for idx in 0..len {
                    if let Ok(code) = list.GetAt(idx) {
                        let text = hstring_to_string(&code);
                        if !text.is_empty() {
                            codes.push(text);
                        }
                    }
                }
            }
            let meta = index.lookup(&codes, &name, location.as_deref());
            (
                winget_scope.or(meta.scope),
                meta.install_date,
                meta.updated_at,
                meta.icon,
            )
        }
        _ => (None, None, None, None),
    };
    // Prefer a local DisplayIcon; otherwise use the first HTTP(S) WinGet manifest icon.
    let icon = icon.or_else(|| pick_manifest_icon(package));

    Ok(PackageSummary {
        key: PackageKey::new(ManagerKind::WinGet, source, id),
        name,
        version,
        available_version,
        update_available,
        scope,
        install_date,
        updated_at,
        icon,
    })
}

/// Soft-fails: missing metadata/icons must not drop the package row.
fn pick_manifest_icon(package: &CatalogPackage) -> Option<String> {
    let version = package.DefaultInstallVersion().ok()?;
    let metadata = version.GetCatalogPackageMetadata().ok()?;
    let icons = metadata.Icons().ok()?;
    let count = icons.Size().ok()?;
    for index in 0..count {
        let Ok(icon) = icons.GetAt(index) else {
            continue;
        };
        let Ok(url) = icon.Url() else {
            continue;
        };
        let url = hstring_to_string(&url);
        if url.starts_with("https://") || url.starts_with("http://") {
            return Some(url);
        }
    }
    None
}

fn read_matches(
    matches: &windows_collections::IVectorView<MatchResult>,
    registry: Option<&RegistryIndex>,
) -> Result<Vec<PackageSummary>, ManagerError> {
    let count = matches.Size()?;
    let mut packages = Vec::with_capacity(count as usize);
    for index in 0..count {
        // One broken installed row must not fail Discover / Installed / Updates.
        let Ok(entry) = matches.GetAt(index) else {
            continue;
        };
        let Ok(package) = entry.CatalogPackage() else {
            continue;
        };
        if let Ok(summary) = catalog_package_summary(&package, registry) {
            packages.push(summary);
        }
    }
    Ok(packages)
}

/// Thin WinGet catalog client. Create on a worker thread; do not share across threads.
pub struct WinGetSource {
    // Declaration order is drop order: release the WinGet proxy before CoUninitialize.
    manager: PackageManager,
    _apartment: ComApartment,
}

impl WinGetSource {
    pub fn connect() -> Result<Self, ManagerError> {
        ensure_winmd_beside_exe()?;
        let apartment = ComApartment::enter()?;
        let unknown: IUnknown =
            match unsafe { CoCreateInstance(&PACKAGE_MANAGER, None, CLSCTX_LOCAL_SERVER) } {
                Ok(object) => object,
                Err(error) if error.code() == CLASS_NOT_REGISTERED => {
                    return Err(classify_app_installer_gap());
                }
                Err(error) => return Err(error.into()),
            };
        let manager = unknown.cast().map_err(ManagerError::from)?;
        Ok(Self {
            _apartment: apartment,
            manager,
        })
    }

    /// Searches remote catalogs by name, id and moniker, excluding packages already installed.
    pub fn search_discover(
        &self,
        query: &str,
        source: Option<&str>,
    ) -> Result<Vec<PackageSummary>, ManagerError> {
        let catalog = self.discover_catalog(source)?;
        let find_options = create_instance::<FindPackagesOptions>(&FIND_PACKAGES_OPTIONS)?;
        let value = HSTRING::from(query);
        for field in [
            PackageMatchField::Name,
            PackageMatchField::Id,
            PackageMatchField::Moniker,
        ] {
            let filter = create_instance::<PackageMatchFilter>(&PACKAGE_MATCH_FILTER)?;
            filter.SetField(field)?;
            filter.SetOption(PackageFieldMatchOption::ContainsCaseInsensitive)?;
            filter.SetValue(&value)?;
            find_options.Selectors()?.Append(&filter)?;
        }
        let find = catalog.FindPackages(&find_options)?;
        if find.Status()? != FindPackagesResultStatus::Ok {
            return Err(ManagerError::Operation(format!(
                "WinGet search failed ({:?})",
                find.Status()?
            )));
        }
        Ok(read_matches(&find.Matches()?, None)?
            .into_iter()
            .filter(|package| package.version.is_none())
            .collect())
    }

    fn discover_catalog(&self, source: Option<&str>) -> Result<PackageCatalog, ManagerError> {
        let options =
            create_instance::<CreateCompositePackageCatalogOptions>(&CREATE_COMPOSITE_OPTIONS)?;
        match source.filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all")) {
            Some(specific) => {
                let name = remote_source_name(specific);
                options
                    .Catalogs()?
                    .Append(&self.manager.GetPackageCatalogByName(&HSTRING::from(name))?)?;
            }
            None => {
                options
                    .Catalogs()?
                    .Append(&self.manager.GetPackageCatalogByName(h!("winget"))?)?;
                if let Ok(msstore) = self.manager.GetPackageCatalogByName(h!("msstore")) {
                    let _ = options.Catalogs()?.Append(&msstore);
                }
            }
        }
        // Correlates remote hits with local installs so `InstalledVersion()` marks installed apps.
        options
            .SetCompositeSearchBehavior(CompositeSearchBehavior::RemotePackagesFromAllCatalogs)?;
        let connect = self
            .manager
            .CreateCompositePackageCatalog(&options)?
            .Connect()?;
        if connect.Status()? != ConnectResultStatus::Ok {
            return Err(ManagerError::Operation(format!(
                "WinGet discover catalog connect failed ({:?})",
                connect.Status()?
            )));
        }
        Ok(connect.PackageCatalog()?)
    }

    pub fn list_installed(&self) -> Result<Vec<PackageSummary>, ManagerError> {
        let registry = RegistryIndex::collect();
        let find_options = create_instance::<FindPackagesOptions>(&FIND_PACKAGES_OPTIONS)?;
        let find = self.installed_catalog()?.FindPackages(&find_options)?;
        if find.Status()? != FindPackagesResultStatus::Ok {
            return Err(ManagerError::Operation(format!(
                "WinGet installed listing failed ({:?})",
                find.Status()?
            )));
        }
        read_matches(&find.Matches()?, Some(&registry))
    }

    /// Installed packages correlated with the `winget` and `msstore` catalogs.
    fn installed_catalog(&self) -> Result<PackageCatalog, ManagerError> {
        let options =
            create_instance::<CreateCompositePackageCatalogOptions>(&CREATE_COMPOSITE_OPTIONS)?;
        for source in [h!("winget"), h!("msstore")] {
            options
                .Catalogs()?
                .Append(&self.manager.GetPackageCatalogByName(source)?)?;
        }
        options.SetCompositeSearchBehavior(CompositeSearchBehavior::LocalCatalogs)?;
        options.SetInstalledScope(PackageInstallScope::Any)?;
        let connect = self
            .manager
            .CreateCompositePackageCatalog(&options)?
            .Connect()?;
        if connect.Status()? != ConnectResultStatus::Ok {
            return Err(ManagerError::Operation(format!(
                "WinGet installed catalog connect failed ({:?})",
                connect.Status()?
            )));
        }
        Ok(connect.PackageCatalog()?)
    }

    fn remote_catalog(&self, source: &str) -> Result<PackageCatalog, ManagerError> {
        let connect = self
            .manager
            .GetPackageCatalogByName(&HSTRING::from(source))?
            .Connect()?;
        if connect.Status()? != ConnectResultStatus::Ok {
            return Err(ManagerError::Operation(format!(
                "WinGet {source} catalog connect failed ({:?})",
                connect.Status()?
            )));
        }
        Ok(connect.PackageCatalog()?)
    }

    fn find_package(
        &self,
        catalog: &PackageCatalog,
        package_id: &str,
    ) -> Result<CatalogPackage, ManagerError> {
        let find_options = create_instance::<FindPackagesOptions>(&FIND_PACKAGES_OPTIONS)?;
        let filter = create_instance::<PackageMatchFilter>(&PACKAGE_MATCH_FILTER)?;
        filter.SetField(PackageMatchField::Id)?;
        filter.SetOption(PackageFieldMatchOption::Equals)?;
        filter.SetValue(&HSTRING::from(package_id))?;
        find_options.Selectors()?.Append(&filter)?;
        let find = catalog.FindPackages(&find_options)?;
        if find.Status()? != FindPackagesResultStatus::Ok {
            return Err(ManagerError::Operation(format!(
                "WinGet package lookup failed ({:?})",
                find.Status()?
            )));
        }
        let matches = find.Matches()?;
        if matches.Size()? == 0 {
            return Err(ManagerError::Operation(format!(
                "package not found: {package_id}"
            )));
        }
        Ok(matches.GetAt(0)?.CatalogPackage()?)
    }

    /// Runs one operation to completion. `cancel` is polled on this thread; `report`
    /// is invoked on a COM worker thread with each progress update.
    pub fn run_operation(
        &self,
        operation: &QueuedOperation,
        cancel: &AtomicBool,
        report: fn(OperationProgress),
    ) -> Result<(), ManagerError> {
        let id = operation.key.id.as_str();
        match operation.kind {
            OperationKind::Install => {
                let source = remote_source_name(&operation.key.source);
                let package = self.find_package(&self.remote_catalog(source)?, id)?;
                let options = install_options(&package, &operation.options)?;
                let async_op = self.manager.InstallPackageAsync(&package, &options)?;
                let result = await_install(&async_op, cancel, report)?;
                check_install_status(result.Status()?, "install")
            }
            OperationKind::Update => {
                // Upgrade needs the installed version, so resolve through the local catalog.
                let package = self.find_package(&self.installed_catalog()?, id)?;
                let options = install_options(&package, &operation.options)?;
                let async_op = self.manager.UpgradePackageAsync(&package, &options)?;
                let result = await_install(&async_op, cancel, report)?;
                check_install_status(result.Status()?, "upgrade")
            }
            OperationKind::Uninstall => {
                let package = self.find_package(&self.installed_catalog()?, id)?;
                let options = create_instance::<UninstallOptions>(&UNINSTALL_OPTIONS)?;
                match operation.options.scope.as_deref() {
                    Some("user") => {
                        options.SetPackageUninstallScope(PackageUninstallScope::User)?
                    }
                    Some("machine") => {
                        options.SetPackageUninstallScope(PackageUninstallScope::System)?
                    }
                    _ => {}
                }
                let async_op = self.manager.UninstallPackageAsync(&package, &options)?;
                async_op.SetProgress(&AsyncOperationProgressHandler::new(
                    move |_, progress: Ref<UninstallProgress>| {
                        report(uninstall_progress(&progress));
                        Ok(())
                    },
                ))?;
                let result = await_operation(&async_op, cancel)?;
                match result.Status()? {
                    UninstallResultStatus::Ok => Ok(()),
                    status => Err(ManagerError::Operation(format!(
                        "uninstall failed ({status:?})"
                    ))),
                }
            }
        }
    }

    /// Manifest metadata from the package's source catalog, else the installed catalog.
    pub fn details(&self, package_id: &str, source: &str) -> Result<PackageDetails, ManagerError> {
        let remote = self
            .remote_catalog(remote_source_name(source))
            .and_then(|catalog| self.find_package(&catalog, package_id));
        let package = match remote {
            Ok(package) => package,
            Err(_) => self.find_package(&self.installed_catalog()?, package_id)?,
        };
        let version = package.DefaultInstallVersion()?;
        let metadata = version.GetCatalogPackageMetadata()?;
        let text = |value: windows_core::Result<HSTRING>| {
            value
                .map(|value| hstring_to_string(&value))
                .unwrap_or_default()
        };
        let description = match text(metadata.Description()) {
            description if description.is_empty() => text(metadata.ShortDescription()),
            description => description,
        };
        Ok(PackageDetails {
            id: package_id.to_string(),
            name: text(package.Name()),
            version: text(version.Version()),
            publisher: text(metadata.Publisher()),
            author: text(metadata.Author()),
            description,
            homepage: text(metadata.PackageUrl()),
            publisher_url: text(metadata.PublisherUrl()),
            license: text(metadata.License()),
            license_url: text(metadata.LicenseUrl()),
        })
    }
}

/// Install/upgrade options from the stored per-package choices. Agreements are
/// accepted because the user explicitly started this operation (WinGet would
/// otherwise fail packages that declare agreements, such as most `msstore` apps).
fn install_options(
    package: &CatalogPackage,
    choices: &PackageOptions,
) -> Result<InstallOptions, ManagerError> {
    let options = create_instance::<InstallOptions>(&INSTALL_OPTIONS)?;
    options.SetAcceptPackageAgreements(true)?;
    match choices.scope.as_deref() {
        Some("user") => options.SetPackageInstallScope(PackageInstallScope::User)?,
        Some("machine") => options.SetPackageInstallScope(PackageInstallScope::System)?,
        _ => {}
    }
    if let Some(architecture) = choices.architecture.as_deref() {
        let architecture = match architecture {
            "x64" => ProcessorArchitecture::X64,
            "arm64" => ProcessorArchitecture::Arm64,
            "x86" => ProcessorArchitecture::X86,
            "arm" => ProcessorArchitecture::Arm,
            other => {
                return Err(ManagerError::Operation(format!(
                    "unsupported architecture: {other}"
                )));
            }
        };
        options.AllowedArchitectures()?.Append(architecture)?;
    }
    if let Some(wanted) = choices.version.as_deref() {
        let versions = package.AvailableVersions()?;
        let mut found = None;
        for index in 0..versions.Size()? {
            let version = versions.GetAt(index)?;
            if hstring_to_string(&version.Version()?) == wanted {
                found = Some(version);
                break;
            }
        }
        let version = found.ok_or_else(|| {
            ManagerError::Operation(format!("version {wanted} is not in the catalog"))
        })?;
        options.SetPackageVersionId(&version)?;
    }
    if let Some(extra) = choices.extra_args.as_deref() {
        options.SetAdditionalInstallerArguments(&HSTRING::from(extra))?;
    }
    Ok(options)
}

const REMOTE_SOURCE: &str = "winget";

/// Package rows carry the catalog they came from; anything unknown falls back to `winget`.
fn remote_source_name(source: &str) -> &str {
    if source.eq_ignore_ascii_case("msstore") {
        "msstore"
    } else {
        REMOTE_SOURCE
    }
}

fn check_install_status(status: InstallResultStatus, verb: &str) -> Result<(), ManagerError> {
    match status {
        InstallResultStatus::Ok => Ok(()),
        status => Err(ManagerError::Operation(format!(
            "{verb} failed ({status:?})"
        ))),
    }
}

fn await_install<R: RuntimeType + 'static>(
    async_op: &IAsyncOperationWithProgress<R, InstallProgress>,
    cancel: &AtomicBool,
    report: fn(OperationProgress),
) -> Result<R, ManagerError> {
    async_op.SetProgress(&AsyncOperationProgressHandler::new(
        move |_, progress: Ref<InstallProgress>| {
            report(install_progress(&progress));
            Ok(())
        },
    ))?;
    await_operation(async_op, cancel)
}

/// Polls instead of blocking so a cancel request reaches WinGet on the owning thread.
fn await_operation<R: RuntimeType + 'static, P: RuntimeType + 'static>(
    async_op: &IAsyncOperationWithProgress<R, P>,
    cancel: &AtomicBool,
) -> Result<R, ManagerError> {
    let mut cancel_sent = false;
    loop {
        match async_op.Status()? {
            AsyncStatus::Started => {
                if !cancel_sent && cancel.load(Ordering::Acquire) {
                    async_op.Cancel()?;
                    cancel_sent = true;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            AsyncStatus::Canceled => return Err(ManagerError::Cancelled),
            _ => return Ok(async_op.GetResults()?),
        }
    }
}

fn install_progress(progress: &InstallProgress) -> OperationProgress {
    match progress.State {
        PackageInstallProgressState::Downloading => {
            OperationProgress::new(ProgressStage::Downloading, progress.DownloadProgress)
        }
        PackageInstallProgressState::Installing => {
            OperationProgress::new(ProgressStage::Installing, progress.InstallationProgress)
        }
        PackageInstallProgressState::Queued => OperationProgress::new(ProgressStage::Queued, 0.0),
        _ => OperationProgress::new(ProgressStage::Finishing, 0.0),
    }
}

fn uninstall_progress(progress: &UninstallProgress) -> OperationProgress {
    match progress.State {
        PackageUninstallProgressState::Uninstalling => {
            OperationProgress::new(ProgressStage::Uninstalling, progress.UninstallationProgress)
        }
        PackageUninstallProgressState::Queued => OperationProgress::new(ProgressStage::Queued, 0.0),
        _ => OperationProgress::new(ProgressStage::Finishing, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn stage_winmd_beside_test_exe() {
        let exe = std::env::current_exe().expect("current test exe");
        let dest = exe.with_file_name(WINMD_NAME);
        if dest.is_file() {
            return;
        }
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../third_party/winget")
            .join(WINMD_NAME);
        std::fs::copy(&src, &dest).unwrap_or_else(|error| {
            panic!("copy {} -> {}: {error}", src.display(), dest.display())
        });
    }

    #[test]
    #[ignore = "touches the live WinGet COM server; run with --ignored on a machine with App Installer"]
    fn live_installed_listing() {
        stage_winmd_beside_test_exe();
        let source = WinGetSource::connect().expect("WinGet COM connect");
        let installed = source.list_installed().expect("list installed");
        assert!(
            !installed.is_empty(),
            "expected at least one installed package"
        );
    }
}
