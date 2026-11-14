//! Per-user sparse package that exposes Image Resizer in the Windows 11 menu.
//!
//! PackageManager calls stay on the background thread that initialized WinRT.
//! The WinRT apartment guard is created first so it is dropped after the
//! package objects. Failure leaves the classic registry verb in place.

use std::{
    io,
    path::{Path, PathBuf},
};
use windows::{
    ApplicationModel::Package,
    Foundation::Uri,
    Management::Deployment::{DeploymentResult, PackageManager, RegisterPackageOptions},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::HSTRING,
};

const PACKAGE_NAME: &str = "Pirimera.PowerKit.Shell";
const PUBLISHER: &str = "CN=Pirimera";
const LOGO_44: &[u8] = include_bytes!("../../../brand/windows/msix/Square44x44Logo.scale-100.png");
const LOGO_150: &[u8] =
    include_bytes!("../../../brand/windows/msix/Square150x150Logo.scale-100.png");
const LOGO_STORE: &[u8] = include_bytes!("../../../brand/windows/msix/StoreLogo.scale-100.png");

pub(super) fn start(install_dir: PathBuf) {
    std::thread::spawn(move || {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let message = match register(&install_dir) {
                Ok(Registration::Current) => format!("ok current {}", install_dir.display()),
                Ok(Registration::Added) => format!("ok added {}", install_dir.display()),
                Err(error) => format!("error {error}"),
            };
            note(&message);
        }));
    });
}

enum Registration {
    Current,
    Added,
}

fn register(install_dir: &Path) -> io::Result<Registration> {
    if !install_dir.join("powerkit_shell.dll").is_file()
        || !install_dir.join("powerkit.exe").is_file()
    {
        return Err(io::Error::other(
            "shell extension is not beside the executable",
        ));
    }
    write_asset(
        &install_dir.join("Assets/Shell/Square44x44Logo.png"),
        LOGO_44,
    )?;
    write_asset(
        &install_dir.join("Assets/Shell/Square150x150Logo.png"),
        LOGO_150,
    )?;
    write_asset(&install_dir.join("Assets/Shell/StoreLogo.png"), LOGO_STORE)?;
    let manifest = manifest_path()?;
    std::fs::write(&manifest, appx_manifest())?;
    // The apartment must outlive every WinRT object created below.
    let _apartment = WinRt::new()?;
    let manager = PackageManager::new().map_err(step("PackageManager"))?;
    if package_matches(&manager, install_dir).map_err(step("FindPackages"))? {
        return Ok(Registration::Current);
    }
    let options = RegisterPackageOptions::new().map_err(step("RegisterOptions"))?;
    options
        .SetAllowUnsigned(true)
        .map_err(step("AllowUnsigned"))?;
    options
        .SetForceUpdateFromAnyVersion(true)
        .map_err(step("ForceUpdate"))?;
    let external =
        Uri::CreateUri(&HSTRING::from(file_uri(install_dir))).map_err(step("ExternalUri"))?;
    options
        .SetExternalLocationUri(&external)
        .map_err(step("ExternalLocation"))?;
    let package =
        Uri::CreateUri(&HSTRING::from(file_uri(&manifest))).map_err(step("ManifestUri"))?;
    let operation = manager
        .RegisterPackageByUriAsync(&package, &options)
        .map_err(step("RegisterPackage"))?;
    let result = operation.join().map_err(step("Deployment"))?;
    deployment_error(&result)?;
    // SAFETY: no item identifiers. This asks Explorer to reload associations.
    unsafe {
        windows::Win32::UI::Shell::SHChangeNotify(
            windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED,
            windows::Win32::UI::Shell::SHCNF_IDLIST,
            None,
            None,
        );
    }
    Ok(Registration::Added)
}

struct WinRt;

impl WinRt {
    fn new() -> io::Result<Self> {
        // SAFETY: this thread owns the apartment. Drop calls RoUninitialize on the same thread.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(winrt_error)?;
        Ok(Self)
    }
}

impl Drop for WinRt {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

fn package_matches(manager: &PackageManager, install_dir: &Path) -> windows::core::Result<bool> {
    let packages = match manager
        .FindPackagesByNamePublisher(&HSTRING::from(PACKAGE_NAME), &HSTRING::from(PUBLISHER))
    {
        Ok(packages) => packages,
        // Some hosts deny package enumeration. Registration can still proceed.
        Err(error) if error.code().0 as u32 == 0x8007_0005 => return Ok(false),
        Err(error) => return Err(error),
    };
    let iterator = packages.First()?;
    if !iterator.HasCurrent()? {
        return Ok(false);
    }
    let package = iterator.Current()?;
    Ok(version_matches(&package)? && path_matches(&package, install_dir)?)
}

fn version_matches(package: &Package) -> windows::core::Result<bool> {
    let version = package.Id()?.Version()?;
    let (major, minor, build, revision) = version_parts();
    Ok(version.Major == major
        && version.Minor == minor
        && version.Build == build
        && version.Revision == revision)
}

fn path_matches(package: &Package, install_dir: &Path) -> windows::core::Result<bool> {
    let path = package
        .EffectivePath()
        .or_else(|_| package.InstalledPath())?;
    Ok(same_dir(Path::new(&path.to_string()), install_dir))
}

fn deployment_error(result: &DeploymentResult) -> io::Result<()> {
    let code = result.ExtendedErrorCode().map_err(winrt_error)?;
    if code.is_ok() {
        return Ok(());
    }
    let text = result
        .ErrorText()
        .map(|text| text.to_string())
        .unwrap_or_default();
    let detail = if code.0 as u32 == 0x8007_3D2C {
        " Windows allows this unsigned package only when Developer Mode is on."
    } else {
        ""
    };
    Err(io::Error::other(format!(
        "sparse package {:#010x} {text}{detail}",
        code.0 as u32
    )))
}

fn appx_manifest() -> String {
    let clsid = include_str!("../../shell_clsid.txt").trim();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  xmlns:desktop4="http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"
  xmlns:desktop5="http://schemas.microsoft.com/appx/manifest/desktop/windows10/5"
  xmlns:com="http://schemas.microsoft.com/appx/manifest/com/windows10"
  IgnorableNamespaces="uap uap10 rescap desktop4 desktop5 com">
  <Identity Name="{PACKAGE_NAME}" ProcessorArchitecture="{arch}" Publisher="{PUBLISHER}" Version="{version}" />
  <Properties>
    <DisplayName>PowerKit Image Resizer</DisplayName>
    <PublisherDisplayName>Pirimera</PublisherDisplayName>
    <Logo>Assets\Shell\StoreLogo.png</Logo>
    <uap10:AllowExternalContent>true</uap10:AllowExternalContent>
  </Properties>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.22000.0" MaxVersionTested="10.0.28000.0" />
  </Dependencies>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
    <rescap:Capability Name="unvirtualizedResources" />
  </Capabilities>
  <Applications>
    <Application Id="ImageResizer" Executable="powerkit.exe" uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="win32App">
      <uap:VisualElements AppListEntry="none" DisplayName="PowerKit Image Resizer" Description="Image Resizer" BackgroundColor="transparent" Square150x150Logo="Assets\Shell\Square150x150Logo.png" Square44x44Logo="Assets\Shell\Square44x44Logo.png" />
      <Extensions>
        <desktop4:Extension Category="windows.fileExplorerContextMenus">
          <desktop4:FileExplorerContextMenus>
            <desktop5:ItemType Type="*">
              <desktop5:Verb Id="Resize" Clsid="{clsid}" />
            </desktop5:ItemType>
          </desktop4:FileExplorerContextMenus>
        </desktop4:Extension>
        <com:Extension Category="windows.comServer" uap10:RuntimeBehavior="packagedClassicApp">
          <com:ComServer>
            <com:SurrogateServer DisplayName="PowerKit Image Resizer">
              <com:Class Id="{clsid}" Path="powerkit_shell.dll" ThreadingModel="STA" />
            </com:SurrogateServer>
          </com:ComServer>
        </com:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>
"#,
        arch = package_architecture(),
        version = version_text(),
    )
}

fn package_architecture() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "neutral"
    }
}

fn version_parts() -> (u16, u16, u16, u16) {
    let mut parts = [0u16; 4];
    for (index, part) in env!("CARGO_PKG_VERSION").split('.').take(4).enumerate() {
        parts[index] = part.parse().unwrap_or(0);
    }
    (parts[0], parts[1], parts[2], parts[3])
}

fn version_text() -> String {
    let (major, minor, build, revision) = version_parts();
    format!("{major}.{minor}.{build}.{revision}")
}

fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from("file:///");
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(byte as char);
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

fn same_dir(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase()
    };
    normalize(left) == normalize(right)
}

fn write_asset(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.is_file() && std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

fn manifest_path() -> io::Result<PathBuf> {
    let base =
        std::env::var_os("LOCALAPPDATA").ok_or_else(|| io::Error::other("LOCALAPPDATA missing"))?;
    let dir = PathBuf::from(base).join("PowerKit").join("shell");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("AppxManifest.xml"))
}

fn note(text: &str) {
    let Ok(base) = std::env::var("LOCALAPPDATA") else {
        return;
    };
    let dir = PathBuf::from(base).join("PowerKit");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("shell-package.txt"), text);
}

fn step(name: &'static str) -> impl Fn(windows::core::Error) -> io::Error {
    move |error| io::Error::other(format!("{name}: {error}"))
}
fn winrt_error(error: windows::core::Error) -> io::Error {
    step("WinRT")(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_points_at_the_shell_server() {
        let xml = appx_manifest();
        assert!(xml.contains(include_str!("../../shell_clsid.txt").trim()));
        assert!(xml.contains(PACKAGE_NAME));
        assert!(xml.contains("powerkit_shell.dll"));
        assert!(xml.contains("windows.fileExplorerContextMenus"));
        assert!(xml.contains(&format!("Version=\"{}\"", version_text())));
        assert!(xml.contains(&format!(
            "ProcessorArchitecture=\"{}\"",
            package_architecture()
        )));
        assert!(!xml.contains("8F491918-259F-451A-950F-8C3EBF4864AF"));
    }

    #[test]
    fn file_uri_encodes_spaces_and_uses_forward_slashes() {
        assert_eq!(
            file_uri(Path::new(r"D:\Power Kit\AppxManifest.xml")),
            "file:///D:/Power%20Kit/AppxManifest.xml"
        );
    }

    #[test]
    #[ignore = "set POWERKIT_SHELL_DIR to the folder containing powerkit.exe and powerkit_shell.dll"]
    fn register_shell_package_from_env() {
        let dir = std::env::var("POWERKIT_SHELL_DIR").expect("POWERKIT_SHELL_DIR");
        register(Path::new(&dir)).expect("shell package");
    }

    #[test]
    fn directory_match_ignores_case_and_a_trailing_separator() {
        assert!(same_dir(
            Path::new(r"D:\PowerKit\App\"),
            Path::new(r"d:\powerkit\app")
        ));
    }
}
