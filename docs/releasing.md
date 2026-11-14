# Release engineering

## Identity and tools

Package identity: Pirimera.PowerKit; display name: PowerKit.
ARM64: aarch64-pc-windows-msvc / win-arm64.
x64: x86_64-pc-windows-msvc / win-x64.
Minimum Windows: build 22000 (Windows 11).
Rust is pinned in rust-toolchain.toml; Velopack SDK/CLI are both 1.2.161.

Per-user installation adds a Start menu shortcut, not automatic Windows startup.
Settings remain at %LOCALAPPDATA%\PowerKit\settings.txt outside the install root.
Do not change identity/channels after publication without a migration.
.NET 10 SDK is a packaging prerequisite only; the app uses static CRT.

## Prepare

A normal commit bumps Cargo.toml, the powerkit version in Cargo.lock,
app.manifest's four-part version, CHANGELOG.md and docs/releases/X.Y.Z.md.
See [versioning](development.md#versioning). Use numeric stable versions.
Published versions must not be overwritten; the hook refuses to replace
existing release notes.

Run [development checks](development.md) and release builds for both architectures:
~~~powershell
dotnet tool install vpk --version 1.2.161 --tool-path .tools
./scripts/Package.ps1 -Architecture arm64 -VpkPath "$PWD/.tools/vpk.exe"
$env:POWERKIT_TEST_FEED = (Resolve-Path dist/0.2.0/arm64/releases).Path
cargo test --locked --target aarch64-pc-windows-msvc packaged_feed_download_cache_and_corruption -- --ignored
~~~
Substitute the current source version and architecture.
Package.ps1 checks tag/version, PE architecture and feed hashes and refuses reused
output directories. It passes brand/windows/PowerKit.ico to Velopack as the
Setup.exe icon. The same file is the powerkit.exe icon resource read by the
Start menu and taskbar. MIT license, end-user README and dependency notices are included.
Upload all generated feed/package assets, not just Setup.exe.
The package test checks download, offline cache and corrupt-payload rejection;
it never installs or executes the updater.

## Actions and publication

The workflow runs only from a manual `workflow_dispatch`. Pushes to main, pull
requests and tag pushes do not build or publish. Leave `release_tag` empty and
`publish` off to build the selected ref. Set `release_tag` to a pushed `vX.Y.Z`
tag to build that commit. Turn `publish` on only when that tag should become a
GitHub Release. The tag must match Cargo.toml exactly, and the checkout must be
the commit the tag points at. Publication creates a draft, uploads both
architectures, verifies hashes, then publishes. A failed draft can be retried.
A published release cannot be overwritten.

Actions are SHA-pinned; Dependabot proposes updates without auto-merge.
Only the publish job receives contents:write. PR jobs never use publishing secrets
or pull_request_target.

After local acceptance, push the tag, then start the workflow yourself.
Pushing the tag does not build or publish:
~~~powershell
git tag v0.2.0
git push origin v0.2.0
gh workflow run "Build and release PowerKit" --ref main -f release_tag=v0.2.0 -f publish=true
~~~
Use the intended version. The workflow command publishes; do not run it to test docs.
A build without publication omits `-f publish=true`.

## Private to public

Actions can publish privately with its job token. Authorized users download while
signed in. The app uses anonymous GitHubSource at https://github.com/Pirimera/PowerKit.
It contains no token; automatic downloads become available when that repo is public.

Before public launch, the maintainer should:
- Review history, artifacts and assets for secrets and personal information.
- Enable private vulnerability reporting and verify SECURITY.md's contact route.
- Configure main/tag rulesets. Required jobs: Repository checks, Windows arm64,
  Windows x64. Require review appropriate to the maintainer model.
- Enable dependency alerts/security updates and secret scanning where available.
- Decide on Authenticode signing; initial packages are unsigned.
- Verify physical installation, uninstallation and an installed-version upgrade.

Repo files do not activate these GitHub/account settings. Do not claim they are
enabled without evidence. Signing keys belong in restricted CI secrets.

## Runtime update contract

Checks run at launch and every six hours; manual checks are limited to once per minute.
Only stable releases are used; downgrade is disabled. Networking runs off the UI thread.
Updates apply after normal exit or via update/restart when AlwaysOn is off.
Closing to tray does not apply them; session end does not start the updater.
Startup auto-apply is disabled so duplicate launches cannot interrupt a session.
Failures leave the current version running. Raw developer binaries do not fetch updates.
Full packages are used initially.

See [testing](testing.md) and [versioned evidence](validation/README.md).
References: [Velopack Rust](https://docs.velopack.io/getting-started/rust),
[CLI](https://docs.velopack.io/reference/cli/content/vpk-windows).
