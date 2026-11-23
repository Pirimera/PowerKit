# Development

## Prerequisites

Windows 11, Rust/rustup, MSVC C++ build tools and the Windows SDK (rc.exe for
the icon resource; MakeAppx, MakePri and SignTool for MSIX packaging).
rust-toolchain.toml pins Rust 1.99.1 including rustfmt and Clippy.
ARM64 is primary; x64 is supported.
MSVC CRT is linked statically. Running WinUI surfaces in unpackaged developer
builds needs the Windows App Runtime 2.x (2.4 or later) installed;
`Get-AppxPackage Microsoft.WindowsAppRuntime.2*` lists the installed versions.
Product MSIX builds declare that runtime as a Store package dependency rather
than redistributing the runtime installer. Day-to-day work runs the unpackaged
executable from `target/`; it has no in-app updater. Local MSIX trials use a
self-signed test certificate; see [releasing](releasing.md#local-msix).

~~~powershell
rustup target add aarch64-pc-windows-msvc
./scripts/Check.ps1 -Architecture arm64
cargo build --locked --release --target aarch64-pc-windows-msvc
./scripts/Test-AlwaysOn.ps1
~~~

A debug build opens the settings shell on the page named by
`POWERKIT_DEBUG_PAGE`, a page tag with an optional PowerMouse tab
(`power-mouse`, `power-mouse:scrolling`, `power-mouse:ring`), so a page can
be looked at without clicking through to it. Release builds ignore it.

For x64, use -Architecture x64 and x86_64-pc-windows-msvc.
An x64 executable running on ARM64 is emulation coverage, not native x64 coverage.
Full package tests are described in [releasing](releasing.md).

`scripts/cargo.ps1` runs one Cargo command for an architecture and puts the
target option where Cargo expects it; `Check.ps1` uses it for tests and Clippy.

~~~powershell
./scripts/cargo.ps1 -Target arm64 -CargoArgs @('test', '--locked')
./scripts/cargo.ps1 -Target x64 -CargoArgs @('build', '--locked', '--release')
~~~

## Resource compiler

The build needs a Windows resource compiler: `rc.exe` from the Windows SDK, or
`llvm-rc.exe`. POWERKIT_LLVM_ROOT may point to an extracted LLVM directory
whose bin/llvm-rc.exe replaces a missing rc.exe. This is a local environment
setting: never commit machine-specific paths.

## Workspace

The root `Cargo.toml` declares the workspace; the application package is
`apps/powerkit`, and `cargo run` from the root starts it.
`crates/core` (`powerkit-core`, domain logic), `crates/app` (`powerkit-app`,
controllers, port traits and the UI contract) and `crates/ui/reactor`
(`powerkit-ui-reactor`, the WinUI 3 settings UI), `crates/platform/windows`
(`powerkit-platform-windows`, the ports), `apps/shell-ext`
(`powerkit-shell-ext`, the Explorer command DLL) and `apps/winget-worker`
(PowerStore's WinGet child process) are default members, so a
plain `cargo test`, `cargo clippy` or `cargo fmt --all` covers them together
with the application. The UI crate, the platform crate, the DLL and the
worker are empty off Windows. The first two build
on any platform:

~~~powershell
cargo test --locked -p powerkit-core -p powerkit-app
~~~

The application's tests enable the crate's `fixtures` feature through
`[dev-dependencies]`; product builds do not. `powerkit-core` keeps version
`0.0.0`: only the application carries the product version. See
[architecture](architecture.md#layers) for where new code belongs.

## Build output size

`target/` is ignored and can always be deleted with `cargo clean`; the next
build takes a few minutes. The development profile keeps only line tables as
debug information. Run `cargo clean` now and then, since compiled
dependencies and test binaries of earlier toolchains and versions accumulate.
Close a PowerKit started from `target/` first, because its executable is
locked while it runs.

## Repository conventions

Use UTF-8 and the checked-in EditorConfig/Git attributes. Rust formatting comes
from rustfmt; avoid hand-formatting that fights it. Keep Cargo.lock committed for
the application. The repository check compares the Rust version pins with each other; CI runs it on every pull request.
Dependencies must have a concrete need, acceptable licensing and ARM64 support.

Generated binaries and package directories stay in the ignored target and dist
directories. Do not commit private media, credentials or personal
paths. Product artwork lives in [brand](../brand/README.md). apps/powerkit/windows_resources.rs (included by the build scripts of the
application and the worker) links
brand/windows/PowerKit.ico into powerkit.exe as icon resource 1. The
application also loads those ICO bytes for the window and tray.
The [repository check](../scripts/check_repo.rs) validates local Markdown
file targets in inline Markdown links, required files, shared version constraints
and that `powerkit-core` and `powerkit-app` declare no dependency outside the
workspace layers below them and no build script, and that the UI crate, the
platform crate, the Explorer DLL and the WinGet worker name only the
workspace crates allowed for them (the UI and platform crates never name
each other).
It does not verify external URL availability or fragment anchors.
Run it with `cargo test --manifest-path scripts/Cargo.toml --locked`.

Unit tests remain beside private code. Add integration tests when testing a real
boundary; do not add empty test folders or tests that merely assert source text.
Describe substantive architecture changes in [architecture](architecture.md).

## Versioning

The product version is the stable SemVer in apps/powerkit/Cargo.toml, with the
same number in the powerkit entry of Cargo.lock and, as four parts, in
apps/powerkit/app.manifest. It changes only when a release is prepared, never
in an ordinary commit or pull request, so two branches do not conflict over
it. Raise it with:

~~~powershell
cargo run --manifest-path scripts/Cargo.toml --locked --bin bump-version -- minor
~~~

`minor` is for a release with new features, `patch` for fixes only and `major`
for a breaking change; `0.y.z` then becomes `1.0.0`. The command rewrites the
three files together and the repository check rejects any drift between them,
so do not edit them by hand.

Write a change users will notice as a bullet under "Unreleased" in
CHANGELOG.md, in their words rather than the commit's; the Home page shows
that section. When a release is prepared, the section gets the version as its
heading and a new empty "Unreleased" goes above it (see
[releasing](releasing.md#prepare)). Commit messages may use a
conventional-commit prefix; nothing parses them.

## Automation

`.github/workflows/ci.yml` runs on every pull request and on pushes to main.
It only checks: the scripts crate (repository contracts, rustfmt and Clippy)
and both workflows with actionlint on Ubuntu; the tests and Clippy of
`powerkit-core` and `powerkit-app` on Linux and macOS, without compiling the
Windows application; and rustfmt, the tests and Clippy of the whole workspace
on native Windows ARM64 and x64. It has read-only access, uses no secret and
builds no package, and the repository check keeps it that way.

Packaging and Store/winget publication start only from an authorized manual
workflow, `.github/workflows/release.yml`, which repeats the checks, builds
`--release`, runs the AlwaysOn self-test and packs unsigned MSIX artifacts for
both architectures. A matching `vX.Y.Z` tag is the release identity; tag
pushes alone do not submit to the Store or winget.
Dependabot proposes Cargo and GitHub Actions updates; it never auto-merges them.
See [CONTRIBUTING](../CONTRIBUTING.md) for review expectations and
[releasing](releasing.md) for the Store + winget channel.
