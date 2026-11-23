# PowerKit

![PowerKit](brand/social/readme-header-1280x240.png)

Lightweight native Windows utilities written in Rust. **Windows 11 ARM64 first**;
x64 is also built. macOS and Linux are on the [roadmap](docs/roadmap.md#platforms)
and not implemented. PowerKit is an independent open-source project inspired by
PowerToys, not affiliated with Microsoft.

[Türkçe](docs/tr/README.md) · [User guide](docs/user/README.md) · [Contributing](CONTRIBUTING.md) ·
[Documentation](docs/README.md) · [MIT license](LICENSE)

## Project status

Early development: **AlwaysOn**, **AlwaysTop**, **ScreenRuler**, **ImageResizer**,
**MediaRenamer**, **PowerStore** and **PowerMouse** are implemented. Product distribution is **Microsoft Store** (primary) with
**winget** pointing at the Store package. The MSIX package template and local
packaging script are in place; the Store listing is not published yet. GitHub
Releases is not a product download path.
The interface is available in Turkish, English and Arabic. Store install, uninstall and update still
require physical acceptance testing.

| Tool | Purpose | Status |
| --- | --- | --- |
| AlwaysOn | Keep Windows awake, optionally keep the display on | Implemented |
| AlwaysTop | Pin a window above other windows | Implemented |
| ScreenRuler | Measure on-screen pixels with edge detection | Implemented |
| ImageResizer | Resize images in batches | Implemented |
| MediaRenamer | Rename photos and videos in place from capture-time templates, with preview and undo | Implemented; physical acceptance pending |
| PowerStore | Install, update and remove packages from WinGet, Scoop, Chocolatey and developer package managers in one table | Implemented; interactive installer checks pending |
| PowerMouse | Remap the buttons of any mouse, with an Actions Ring; for MX Master 4 over Bluetooth also its extra buttons, battery, DPI and wheel settings | Implemented |

See the [roadmap](docs/roadmap.md) for what is open.

## Install and update

Install from the **Microsoft Store** when the listing is live, or with **winget**
once the Store package is published. Do not use GitHub Releases as an install
channel. Builds are **MSIX** for ARM64 and x64 and declare Windows App Runtime
2.4 or later as a Store package dependency; no separate Visual C++ Redistributable
or .NET runtime is required.

Until the Store listing ships, developers run unpackaged builds locally and may
sideload a self-signed test MSIX with Developer Mode enabled. Store publication
re-signs the package.

Updates come from the Store (and winget against that package). An active AlwaysOn
session is never automatically interrupted for an update.

## Use PowerKit

The [user guide](docs/user/README.md) explains every tool, the tray menu, the
shortcuts and the settings, in English and [Turkish](docs/user/tr/README.md).

| Shortcut | Action |
| --- | --- |
| Win+Ctrl+Shift+A | Turn AlwaysOn on or off |
| Win+Ctrl+T | Pin or unpin the window in front |
| Win+Ctrl++ / Win+Ctrl+- | Change the opacity of a pinned window |
| Win+Ctrl+Shift+M | Start or stop ScreenRuler |

The X button hides the window to the tray; use Exit PowerKit in the tray menu
or on the Settings page to quit. Settings are kept in
`%LOCALAPPDATA%\PowerKit\settings.txt`.

## Build

On Windows, install Rust, MSVC C++ build tools and the Windows SDK.
The repository pins the Rust toolchain. Development builds run unpackaged from
`target/`; `scripts/Package.ps1` builds an MSIX (see [releasing](docs/releasing.md#local-msix)).

~~~powershell
rustup target add aarch64-pc-windows-msvc
cargo build --locked --release --target aarch64-pc-windows-msvc
./scripts/Check.ps1 -Architecture arm64
~~~

Use x86_64-pc-windows-msvc / x64 for x64. See the
[development guide](docs/development.md) for the toolchain, the workspace and the checks.

## Participate

Bug reports, documentation and focused pull requests are welcome in English or Turkish.
Read [CONTRIBUTING](CONTRIBUTING.md), [the code of conduct](CODE_OF_CONDUCT.md)
and [security reporting](SECURITY.md). Automated contributors must also read [AGENTS.md](AGENTS.md).
