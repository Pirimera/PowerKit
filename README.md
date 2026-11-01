# PowerKit

![PowerKit](brand/social/readme-header-1280x240.png)

Lightweight native Windows utilities written in Rust. **Windows 11 ARM64 first**;
x64 is also built. PowerKit is an independent project inspired by PowerToys,
not affiliated with Microsoft.

[Türkçe](docs/tr/README.md) · [Contributing](CONTRIBUTING.md) ·
[Documentation](docs/README.md) · [MIT license](LICENSE)

## Project status

Early development: **AlwaysOn**, **AlwaysTop** and the Velopack installer/updater
are implemented.
The interface currently uses Turkish. Setup, uninstall and a complete installed
version-to-version update still require physical acceptance testing.

| Tool | Purpose | Status |
| --- | --- | --- |
| AlwaysOn | Keep Windows awake, optionally keep the display on | Implemented |
| AlwaysTop | Pin a window above other windows | Implemented |
| ScreenRuler | Measure on-screen dimensions | Planned |
| ImageResizer | Resize images in batches | Planned |
| MediaRenamer | Rename media using selectable metadata templates | Planned |

MouseMapper is outside the current scope. See the [roadmap](docs/roadmap.md).

## Install and update

Download the matching **ARM64** or **x64** Setup.exe from
[GitHub Releases](https://github.com/Pirimera/PowerKit/releases), when a release is available.
Installation is per-user; no .NET runtime or separate Visual C++ runtime is required.
Initial installers are unsigned. The application does not register itself to start with Windows.

While the repository is private, downloading releases requires GitHub access;
the application intentionally contains no GitHub token. Anonymous automatic updates
become available once the repository is public.

Updates download in the background and apply on normal application exit.
An active AlwaysOn session is never automatically interrupted. To restart for an
update immediately, first stop AlwaysOn, then choose the update/restart button.

## Use AlwaysTop

The tray AlwaysTop submenu pins the last focused eligible window. Win+Ctrl+T
(or Win+Alt+T if that shortcut is taken) toggles the window that is in the
foreground, like PowerToys Always On Top, without unpinning the others. A
rounded accent frame sits immediately behind each pinned window, including
File Explorer. The submenu label, check and tray tip show what is pinned.
The same check or shortcut unpins it.
Win+Ctrl++ and Win+Ctrl+- change the pinned window's opacity; unpin restores it.
A tray check plays the Windows pin and opacity sounds. Closing the window or
exiting PowerKit also clears the pin. Pins and opacity are not saved.
PowerKit's own window, tool windows and the taskbar cannot be pinned.

## Use AlwaysOn

Choose indefinite mode or a 30/60/120-minute timer. The display option also prevents
idle display sleep. The X button hides the window while the tray icon is available;
use the explicit exit button to stop the application.

Manual sleep, closing the lid, and screensavers are not blocked. Timers do not
extend across sleep. Every launch starts with AlwaysOn off; display sleep and
AlwaysTop sound persist in %LOCALAPPDATA%\PowerKit\settings.txt.

## Build

On Windows, install Rust, MSVC C++ build tools, Windows SDK and LLVM clang.
The repository pins the Rust toolchain. Packaging additionally requires .NET 10 SDK.

~~~powershell
rustup target add aarch64-pc-windows-msvc
cargo build --locked --release --target aarch64-pc-windows-msvc
./scripts/Check.ps1 -Architecture arm64
~~~

Use x86_64-pc-windows-msvc / x64 for x64. See the
[development guide](docs/development.md) for the portable toolchain and validation.

## Participate

Bug reports, documentation and focused pull requests are welcome in English or Turkish.
Read [CONTRIBUTING](CONTRIBUTING.md), [the code of conduct](CODE_OF_CONDUCT.md)
and [security reporting](SECURITY.md). Automated contributors must also read [AGENTS.md](AGENTS.md).
