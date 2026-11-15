# PowerKit

![PowerKit](brand/social/readme-header-1280x240.png)

Lightweight native Windows utilities written in Rust. **Windows 11 ARM64 first**;
x64 is also built. PowerKit is an independent project inspired by PowerToys,
not affiliated with Microsoft.

[Türkçe](docs/tr/README.md) · [Contributing](CONTRIBUTING.md) ·
[Documentation](docs/README.md) · [MIT license](LICENSE)

## Project status

Early development: **AlwaysOn**, **AlwaysTop**, **ScreenRuler**, **ImageResizer** and the Velopack installer/updater
are implemented.
The interface currently uses Turkish. Setup, uninstall and a complete installed
version-to-version update still require physical acceptance testing.

| Tool | Purpose | Status |
| --- | --- | --- |
| AlwaysOn | Keep Windows awake, optionally keep the display on | Implemented |
| AlwaysTop | Pin a window above other windows | Implemented |
| ScreenRuler | Measure on-screen pixels with edge detection | Implemented |
| ImageResizer | Resize images in batches | Implemented |
| MediaRenamer | Rename media using selectable metadata templates | Planned |

MouseMapper is outside the current scope. See the [roadmap](docs/roadmap.md).

## Install and update

Download the matching **ARM64** or **x64** Setup.exe from
[GitHub Releases](https://github.com/Pirimera/PowerKit/releases), when a release is available.
Installation is per-user; no .NET runtime or separate Visual C++ runtime is required.
The settings window and Image Resizer dialog use WinUI 3 and need the Windows App
Runtime 2.4 or later, which the installer does not add. Without it PowerKit offers
the runtime download and keeps working from the notification area with a Win32
Image Resizer dialog.
Initial installers are unsigned. The application does not register itself to start with Windows.

While the repository is private, downloading releases requires GitHub access;
the application intentionally contains no GitHub token. Anonymous automatic updates
become available once the repository is public.

Updates download in the background and apply on normal application exit.
An active AlwaysOn session is never automatically interrupted. To restart for an
update immediately, first stop AlwaysOn, then choose the update/restart button.

## Use AlwaysTop

The tray AlwaysTop submenu pins the last focused eligible window. The AlwaysTop
settings page can assign the activation and opacity shortcuts and the frame
color, opacity and thickness. Defaults are Win+Ctrl+T and Win+Ctrl++ / Win+Ctrl+-
(Win+Alt fallbacks if those defaults are taken). The activation shortcut toggles
the foreground window, like PowerToys Always On Top, without unpinning the others.
The configured frame sits immediately behind each pinned window, including
File Explorer. The submenu label, check and tray tip show what is pinned.
The same check or shortcut unpins it.
Opacity shortcuts change the pinned window's opacity; unpin restores it.
A tray check plays the Windows pin and opacity sounds. Closing the window or
exiting PowerKit also clears the pin. Pins and window opacity are not saved;
hotkeys and frame appearance are.
PowerKit's own window, tool windows and the taskbar cannot be pinned.

## Use ScreenRuler

Win+Ctrl+Shift+M, or the tray ScreenRuler command, opens the measure toolbar.
Ctrl+1 through Ctrl+4 select bounds, spacing, horizontal spacing and vertical
spacing. Drag a bounds rectangle, or move the pointer for spacing; the primary
button copies the measurement. Shift keeps the mark on screen. Escape or the
toolbar close button exits. The wheel changes edge tolerance by 15 for that
session. Settings on the ScreenRuler page match PowerToys: shortcut, default
mode, continuous capture, per-channel edge detection, tolerance, extra units,
cross feet and line color. The default line color is #FF4500 and the default
mode is none. An open measurement is not saved.

## Use ImageResizer

The tray command, the ImageResizer settings page, or the Explorer menu on an
image opens the resize dialog. Several selected images open one dialog. On
Windows 11 the compact menu shows the command after the per-user shell package
registers; that unsigned package is accepted when Developer Mode is on. The
classic verb remains under Show more options either way. Small, Medium, Large and Phone match PowerToys, and
custom size supports fit, fill and stretch in pixels, percent, inches or
centimetres. Preview the batch before writing. Copy mode keeps the original
and avoids its filename; replace keeps a `.bak` and then tries to recycle it.
JPEG quality, PNG interlace, TIFF compression, metadata, filename tokens and
the fallback encoder are on the settings page. Only the first frame of a
multi-frame file is resized. AI upscaling is not included.

## Use AlwaysOn

Choose indefinite mode or a 30/60/120-minute timer. The display option also prevents
idle display sleep. The main window is a WinUI 3 settings shell for AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer and General.
General follows a PowerToys-style layout: updates, optional start with Windows,
theme/language, tray visibility, package facts and exit.
The X button hides the window while the tray icon is available;
use the explicit exit control on the General page to stop the application.

Manual sleep, closing the lid, and screensavers are not blocked. Timers do not
extend across sleep. Every launch starts with AlwaysOn off; display sleep,
AlwaysTop sound, General preferences and related options persist in
%LOCALAPPDATA%\PowerKit\settings.txt.

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
