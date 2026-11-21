# PowerKit

![PowerKit](brand/social/readme-header-1280x240.png)

Lightweight native Windows utilities written in Rust. **Windows 11 ARM64 first**;
x64 is also built. PowerKit is an independent project inspired by PowerToys,
not affiliated with Microsoft.

[Türkçe](docs/tr/README.md) · [Contributing](CONTRIBUTING.md) ·
[Documentation](docs/README.md) · [MIT license](LICENSE)

## Project status

Early development: **AlwaysOn**, **AlwaysTop**, **ScreenRuler**, **ImageResizer**
and **MediaRenamer** are implemented. Product distribution is **Microsoft Store** (primary) with
**winget** pointing at the Store package. The MSIX package template and local
packaging script are in place; the Store listing is not published yet. GitHub
Releases is not a product download path.
The interface currently uses Turkish. Store install, uninstall and update still
require physical acceptance testing.

| Tool | Purpose | Status |
| --- | --- | --- |
| AlwaysOn | Keep Windows awake, optionally keep the display on | Implemented |
| AlwaysTop | Pin a window above other windows | Implemented |
| ScreenRuler | Measure on-screen pixels with edge detection | Implemented |
| ImageResizer | Resize images in batches | Implemented |
| MediaRenamer | Rename photos and videos in place from capture-time templates, with preview and undo | Implemented; physical acceptance pending |

PowerMouse, a device-independent mouse button remapper, is designed but not
implemented. See the [roadmap](docs/roadmap.md).

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
The bar matches PowerToys: a compact icon row centered on the primary work area.
Drag the dotted grip at its left edge to move it; it reopens there until PowerKit exits.
Ctrl+1 through Ctrl+4 select bounds, spacing, horizontal spacing and vertical
spacing; choosing the active tool again clears it. Drag a bounds rectangle, or
move the pointer for spacing; the primary button copies the measurement. Shift
keeps the mark on screen. Escape or the
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

Choose indefinite mode, a 30/60/120-minute timer, a custom interval in hours and
minutes, or keep awake until a chosen date and time, like PowerToys Awake. The
display option also prevents idle display sleep. "Keep on while an app runs" binds
to the app of the last active window and stops when it exits. Win+Ctrl+Shift+A
(Win+Alt+Shift+A if taken) toggles AlwaysOn, restarting the last mode chosen in this
session. Five minutes before a timed session ends a notification offers 30 more
minutes; the tray has the same "Extend by 30 minutes" item. Optional power
conditions keep AlwaysOn to AC power or stop it below a battery level.
`powercfg /requests` lists the request as "PowerKit AlwaysOn".
From a terminal, `powerkit --always-on 90` (minutes), `--always-on on`,
`--always-on off` or `--always-on-pid <pid>` controls the running instance or
starts one hidden. The main window is a WinUI 3 settings shell for AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer and Home.
Home follows a PowerToys-style layout: updates, optional start with Windows,
theme/language, tray visibility, which tools the tray menu lists and their
order, package facts and exit.
The X button hides the window while the tray icon is available;
use the explicit exit control on the Home page to stop the application.

Manual sleep, closing the lid, and screensavers are not blocked. Timers do not
extend across sleep. Every launch starts with AlwaysOn off; display sleep, the custom interval,
the AlwaysOn shortcut, end notice and power conditions, AlwaysTop sound, General preferences and related options persist in
%LOCALAPPDATA%\PowerKit\settings.txt.

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
[development guide](docs/development.md) for the portable toolchain and validation.

## Participate

Bug reports, documentation and focused pull requests are welcome in English or Turkish.
Read [CONTRIBUTING](CONTRIBUTING.md), [the code of conduct](CODE_OF_CONDUCT.md)
and [security reporting](SECURITY.md). Automated contributors must also read [AGENTS.md](AGENTS.md).
