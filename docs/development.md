# Development

## Prerequisites

Windows 11, Rust/rustup, MSVC C++ build tools, Windows SDK and LLVM clang.
rust-toolchain.toml pins Rust 1.99.1 including rustfmt and Clippy.
ARM64 is primary; x64 is supported.
MSVC CRT is linked statically. Running WinUI surfaces in unpackaged developer
builds needs the Windows App Runtime 2.x (2.4 or later) installed;
`Get-AppxPackage Microsoft.WindowsAppRuntime.2*` lists the installed versions.
Product MSIX builds declare that runtime as a Store package dependency rather
than redistributing the runtime installer. Local MSIX trials use a self-signed
test certificate and Developer Mode; see [releasing](releasing.md).

~~~powershell
rustup target add aarch64-pc-windows-msvc
./scripts/Check.ps1 -Architecture arm64
cargo build --locked --release --target aarch64-pc-windows-msvc
./scripts/Test-AlwaysOn.ps1
~~~

For x64, use -Architecture x64 and x86_64-pc-windows-msvc.
An x64 executable running on ARM64 is emulation coverage, not native x64 coverage.
Full package tests are described in [releasing](releasing.md).

## Portable toolchain

scripts/cargo.ps1 optionally uses POWERKIT_TOOLCHAIN_ROOT with toolchain/rustup,
toolchain/cargo and msvc directories. POWERKIT_LLVM_ROOT may point to an extracted
LLVM directory containing bin/clang.exe. These are local environment settings:
never commit machine-specific paths. The helper supports the existing portable
stable-aarch64-pc-windows-msvc installation; keep its version equal to the pinned toolchain.

~~~powershell
./scripts/cargo.ps1 -Target arm64 -CargoArgs @('test', '--locked')
./scripts/Check.ps1 -Architecture arm64
~~~

## Repository conventions

Use UTF-8 and the checked-in EditorConfig/Git attributes. Rust formatting comes
from rustfmt; avoid hand-formatting that fights it. Keep Cargo.lock committed for
the application. Version pins for Rust and packaging tools are checked by CI.
Dependencies must have a concrete need, acceptable licensing and ARM64 support.

Generated binaries, package directories and local tools stay in ignored target,
dist and .tools directories. Do not commit private media, credentials or personal
paths. Product artwork lives in [brand](../brand/README.md). build.rs links
brand/windows/PowerKit.ico into powerkit.exe as icon resource 1. The
application also loads those ICO bytes for the window and tray.
The [repository check](../scripts/check_repo.rs) validates local Markdown
file targets in inline Markdown links, required files and shared version constraints.
It does not verify external URL availability or fragment anchors.
Run it with `cargo test --manifest-path scripts/Cargo.toml --locked`.

Unit tests remain beside private code. Add integration tests when testing a real
boundary; do not add empty test folders or tests that merely assert source text.
Describe substantive architecture changes in [architecture](architecture.md).

## Versioning

Install the commit hook once with `./scripts/Install-GitHooks.ps1`. A normal
commit then bumps the stable SemVer in Cargo.toml, the powerkit entry in
Cargo.lock, app.manifest, CHANGELOG.md and docs/releases/X.Y.Z.md.
`feat:` raises the minor component. A `!` on the type, or a `BREAKING CHANGE:`
footer, raises the major component; `0.y.z` therefore becomes `1.0.0`.
Every other commit message raises the patch component.
Merge, squash, amend, rebase and cherry-pick do not bump. `POWERKIT_SKIP_VERSION=1`
skips one commit. The generated release note is the commit summary.
Do not edit the version files by hand in a commit the hook will also see.

## Automation

Packaging and Store/winget publication start only from an authorized manual
workflow. The workflow validates both native Windows architectures. A separate
Ubuntu job runs the scripts crate (repository contracts, rustfmt and Clippy)
without compiling the Windows application. A matching `vX.Y.Z` tag is the release
identity; tag pushes alone do not submit to the Store or winget.
Dependabot proposes Cargo and GitHub Actions updates; it never auto-merges them.
See [CONTRIBUTING](../CONTRIBUTING.md) for review expectations and
[releasing](releasing.md) for the Store + winget channel.
