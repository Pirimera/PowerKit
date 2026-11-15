# Release engineering

## Product channels

Microsoft Store is the primary distribution channel. winget points at the Store
package. Do not ship GitHub Releases, Setup.exe or Velopack feeds as a product
download path. The source repository may remain on GitHub; Releases are not an
end-user install channel.

Ship **MSIX** for ARM64 and x64. Declare **Windows App Runtime 2.4 or later** as
a Store package dependency so the Store installs the runtime with the app. Do
not redistribute `WindowsAppRuntimeInstall.exe` on the product channel and do
not keep a tray-only WinUI fallback once packaged Store builds are the supported
install.

Store submission re-signs the package. Local packaging trials use a self-signed
test certificate and Windows Developer Mode. Paid Authenticode or Trusted
Signing is not required for developer-machine sideload, and is not required for
Store publication when the Store re-signs the package.

## Identity and tools

Package identity: Pirimera.PowerKit (Partner Center identity once assigned).
Display name: PowerKit.
ARM64: aarch64-pc-windows-msvc.
x64: x86_64-pc-windows-msvc.
Minimum Windows: build 22000 (Windows 11).
Rust is pinned in rust-toolchain.toml.

CRT is statically linked. No Visual C++ Redistributable or .NET runtime is
required for the app itself. Optional start-with-Windows should use a package
startup task once MSIX lands; do not invent a second Run-key path for Store
builds. Settings remain at `%LOCALAPPDATA%\PowerKit\settings.txt` outside the
install root unless a deliberate migration is designed and documented.
Do not change package identity after publication without a migration.

## Development and local packaging

Day-to-day work runs unpackaged through the normal Rust toolchain. Unpackaged
WinUI needs an installed Windows App Runtime 2.4 or later on the developer
machine (`Get-AppxPackage Microsoft.WindowsAppRuntime.2*`).

To exercise install, uninstall, package identity or Explorer registration:

1. Enable Developer Mode.
2. Create or reuse a local self-signed test certificate.
3. Trust that certificate for the current user.
4. Build and sign an MSIX, then install it with App Installer or `Add-AppxPackage`.

A local test package and a Store package are different installs: Store re-signs
and may change the publisher identity, so settings and update state do not merge.

## Prepare

A normal commit bumps Cargo.toml, the powerkit version in Cargo.lock,
app.manifest's four-part version, CHANGELOG.md and docs/releases/X.Y.Z.md.
See [versioning](development.md#versioning). Use numeric stable versions.
Published versions must not be overwritten; the hook refuses to replace
existing release notes.

Run [development checks](development.md) and release builds for both
architectures. Packaging or Store-submission changes also need a real local
MSIX install/uninstall check. MIT license, end-user README and dependency
notices ship with the package.

## Actions and publication

Packaging and Store/winget publication start only from an authorized manual
workflow. Pushes to main, pull requests and tag pushes alone do not publish.
A matching `vX.Y.Z` tag remains the release identity and must equal Cargo.toml.

Publication order:

1. Build ARM64 and x64 MSIX packages for the tagged commit.
2. Submit to Partner Center (draft/staged when available).
3. After the Store listing is live, publish or update the winget manifest so it
   references the Store package rather than a GitHub asset.
4. Do not attach product installers to a GitHub Release.

Only the jobs that submit or upload packages receive publishing permissions.
PR jobs never use publishing secrets or `pull_request_target`.

Repo files do not activate Partner Center, winget or GitHub account settings.
Do not claim they are enabled without evidence. Signing keys and Partner Center
credentials belong in restricted CI secrets or the maintainer's account, never
in source.

## Transition from Velopack

The repository still contains Velopack packaging helpers and GitHub release
automation from the earlier channel. Treat them as transitional scaffolding.
New work must not extend GitHub Releases, Setup.exe or architecture-specific
Velopack feeds as a supported product install path. Remove that channel once
MSIX + Store + winget is operational.

Until that migration lands, do not document Velopack as the product
distribution model. Physical acceptance of AlwaysOn sessions and update
apply-on-exit remains relevant for transitional builds still produced by the
old helpers. Missing Windows App Runtime on unpackaged runs is an install gap,
not a supported tray-only product mode.

## Runtime update contract (Store builds)

Store and winget deliver updates. Prefer Store-managed updates over an in-app
download helper. An active AlwaysOn session must never be force-restarted for an
update. Closing to tray must not apply a disruptive update. Failures leave the
current version running.

See [testing](testing.md) and [versioned evidence](validation/README.md).
References: [Choose a distribution path](https://learn.microsoft.com/windows/apps/package-and-deploy/choose-distribution-path),
[Windows App SDK downloads](https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads),
[winget](https://learn.microsoft.com/windows/package-manager/winget/).
