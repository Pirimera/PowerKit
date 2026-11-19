//! Package manager icons, embedded from `brand/package-managers`.
//!
//! The PNGs come from UniGetUI (MIT); see the README beside them for provenance and
//! the logos' own trademark ownership.
use super::super::power_store::ManagerKind;
use windows_reactor::*;

const WINGET: &[u8] = include_bytes!("../../../../brand/package-managers/winget.png");
const SCOOP: &[u8] = include_bytes!("../../../../brand/package-managers/scoop.png");
const CHOCOLATEY: &[u8] = include_bytes!("../../../../brand/package-managers/chocolatey.png");
const NPM: &[u8] = include_bytes!("../../../../brand/package-managers/npm.png");
const PIP: &[u8] = include_bytes!("../../../../brand/package-managers/pip.png");
const CARGO: &[u8] = include_bytes!("../../../../brand/package-managers/cargo.png");
const DOTNET: &[u8] = include_bytes!("../../../../brand/package-managers/dotnet.png");
const POWERSHELL: &[u8] = include_bytes!("../../../../brand/package-managers/powershell.png");
const VCPKG: &[u8] = include_bytes!("../../../../brand/package-managers/vcpkg.png");

/// Windows PowerShell and PowerShell 7 share one logo, as they do in UniGetUI.
fn png(kind: ManagerKind) -> &'static [u8] {
    match kind {
        ManagerKind::WinGet => WINGET,
        ManagerKind::Scoop => SCOOP,
        ManagerKind::Chocolatey => CHOCOLATEY,
        ManagerKind::Npm => NPM,
        ManagerKind::Pip => PIP,
        ManagerKind::Cargo => CARGO,
        ManagerKind::DotnetTool => DOTNET,
        ManagerKind::PowerShell | ManagerKind::PowerShell7 => POWERSHELL,
        ManagerKind::Vcpkg => VCPKG,
    }
}

/// Square icon of `size` pixels for `kind`.
pub(super) fn manager_icon(kind: ManagerKind, size: f64) -> View {
    Image::new()
        .source_data(EncodedImage::from_static(png(kind)))
        .width(size)
        .height(size)
        .stretch(Stretch::Uniform)
        .vertical_alignment(VerticalAlignment::Center)
        .into()
}
