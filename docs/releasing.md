# Release engineering

## Product channels

Microsoft Store is the primary distribution channel. winget points at the Store
package. Do not ship GitHub Releases, Setup.exe or self-update feeds as a product
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

Package identity: PiriMedya.PowerKit (Partner Center assigned; publisher
`CN=1D60B9B0-5B58-4FDF-B209-1FEC64FABD24`, display name "Piri Medya").
Display name: PowerKit.
ARM64: aarch64-pc-windows-msvc.
x64: x86_64-pc-windows-msvc.
Minimum Windows: build 22000 (Windows 11).
Rust is pinned in rust-toolchain.toml.

CRT is statically linked. No Visual C++ Redistributable or .NET runtime is
required for the app itself. Start-with-Windows in a packaged build is the
manifest's `PowerKitStartup` startup task; do not invent a second Run-key path
for Store builds. Settings remain at `%LOCALAPPDATA%\PowerKit\settings.txt` outside the
install root unless a deliberate migration is designed and documented.
Do not change package identity after publication without a migration.

## Development and local packaging

Day-to-day work runs unpackaged through the normal Rust toolchain. Unpackaged
WinUI needs an installed Windows App Runtime 2.4 or later on the developer
machine (`Get-AppxPackage Microsoft.WindowsAppRuntime.2*`).

Unpackaged builds have no in-app updater; rebuild to pick up changes. They
register the Run key and the Explorer verbs (classic verb plus a per-user sparse
package) at runtime. Packaged builds skip that registration: the MSIX manifest
declares the `PowerKitStartup` startup task and the Explorer commands.

### Local MSIX

[`packaging/msix/AppxManifest.xml`](../packaging/msix/AppxManifest.xml) is the
package template; `scripts/Package.ps1` fills version, architecture, publisher
and shell CLSIDs, generates `resources.pri` with MakePri and packs with MakeAppx
from the newest installed Windows SDK. It stages existing release binaries, so
build `--release` first. Output goes to `dist/<version>/<arch>/`.

~~~powershell
cargo build --locked --release --target aarch64-pc-windows-msvc
# Once per machine: certificate in CurrentUser\My, public .cer in dist/.
./scripts/New-TestCertificate.ps1
# Once, elevated: trust the public certificate machine-wide.
Import-Certificate -FilePath dist/PowerKit-test.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
./scripts/Package.ps1 -Architecture arm64 -CertificateThumbprint <thumbprint>
Add-AppxPackage dist/<version>/arm64/PowerKit_<version>.0_arm64.msix
Remove-AppxPackage (Get-AppxPackage PiriMedya.PowerKit).PackageFullName
~~~

Windows 11 sideloads a signed package whose certificate is trusted without
Developer Mode; the unsigned sparse package used by unpackaged builds still
needs Developer Mode. Without `-CertificateThumbprint` the package is unsigned,
which is the form uploaded to Partner Center. `-Publisher` must equal the certificate subject;
use the Partner Center publisher for Store uploads. Remove an output folder
before rebuilding the same version.

The manifest disables file-system write virtualization so packaged and
unpackaged builds share `%LOCALAPPDATA%\PowerKit`. While a local MSIX is
installed, an unpackaged build on the same machine also registers its own
Explorer verbs, so Explorer can show the commands twice.

A local test package and a Store package are different installs: Store re-signs
and may change the publisher identity.

## Prepare

In one commit: raise the version with `bump-version`
(see [versioning](development.md#versioning)), which updates
apps/powerkit/Cargo.toml, the powerkit version in Cargo.lock and the four-part
version in apps/powerkit/app.manifest; then, in CHANGELOG.md, rename
"Unreleased" to that version and add a new, empty "Unreleased" above it. Use
numeric stable versions. Tag that commit `vX.Y.Z` once it is on main. A
published version's section is not edited afterwards.

Run [development checks](development.md) and release builds for both
architectures. Packaging or Store-submission changes also need a real local
MSIX install/uninstall check. MIT license, end-user README and dependency
notices ship with the package.

## Actions and publication

Packaging and Store/winget publication start only from an authorized manual
workflow. Pushes to main and pull requests run the checks in
`.github/workflows/ci.yml` and nothing else; they and tag pushes do not
package or publish.
A matching `vX.Y.Z` tag remains the release identity and must equal apps/powerkit/Cargo.toml.

`.github/workflows/release.yml` is manual and only builds: checks, tests, the
AlwaysOn self-test and unsigned ARM64/x64 MSIX workflow artifacts. It has no
write permission and never creates a GitHub Release. Partner Center submission
is not automated yet.

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

Missing Windows App Runtime on unpackaged runs is an install gap, not a
supported tray-only product mode.

## Runtime update contract (Store builds)

Store and winget deliver updates; PowerKit has no in-app download helper.
An active AlwaysOn session must never be
force-restarted for an update. Closing to tray must not apply a disruptive
update. Failures leave the current version running.

See [testing](testing.md).
References: [Choose a distribution path](https://learn.microsoft.com/windows/apps/package-and-deploy/choose-distribution-path),
[Windows App SDK downloads](https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads),
[winget](https://learn.microsoft.com/windows/package-manager/winget/).
