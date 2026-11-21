# Architecture

PowerKit is one Rust binary crate. Keep it this way until independently reusable
components or independently deployable programs justify a workspace. There is no
plugin loader, background service or web runtime.

## Boundaries

| Location | Responsibility |
| --- | --- |
| src/main.rs | Platform entry point |
| src/always_on.rs | Pure session, interval and expiry deadline transitions |
| src/always_top.rs | Pure pin session (none, one window or a count) and opacity/frame bounds |
| src/hotkey.rs | Shared keyboard chords: storage form, labels, defaults and the Win+Alt fallback |
| src/platform/windows/mod.rs | UI thread, hidden app window, tray, commands, lifecycle |
| src/platform/windows/command.rs | Typed `AppCommand` plus WM_COMMAND id adapter shared by tray and settings |
| src/platform/windows/window/ | WinUI 3 settings shell (windows-reactor): navigation, bridge and command dispatch |
| src/platform/windows/window/pages.rs | Per-tool settings pages composed from shared surfaces |
| src/platform/windows/window/store_page.rs | Power Store workspace: sticky toolbar, unified package table and Discover dialog |
| src/platform/windows/window/view_helpers.rs | Fluent helpers shared by the Power Store and MediaRenamer pages: icon labels, button rows, sized icons and the virtualized item width filler |
| src/platform/windows/window/store_drawer.rs | Power Store right-side overlay drawers (package inspector, history, Settings) |
| src/platform/windows/settings_ui.rs | Shared Fluent card/stack/stepper/hotkey/color surfaces for shell and dialogs |
| src/platform/windows/winui_host.rs | Claims WinUI HWNDs by explicit `WindowRole`; hide-on-close, start hidden, exit and key capture |
| src/platform/windows/icons.rs | Branded window and notify-area icons from brand/windows ICOs |
| src/platform/windows/theme.rs | Taskbar popup theme plus app light/dark colors for Win32 overlays |
| src/platform/windows/startup.rs | Start with Windows: HKCU Run key unpackaged, manifest `StartupTask` when packaged |
| src/platform/windows/package.rs | Detects MSIX package identity; packaged builds skip runtime shell/startup registration |
| src/locale.rs | Turkish/English/Arabic string catalogs and Windows UI language resolution |
| src/platform/windows/menu.rs | App-grouped tray menu spec and native popup construction |
| src/platform/windows/menu_icon.rs | Segoe Fluent glyphs rendered to 32-bit bitmaps for tray menu items |
| src/tray_layout.rs | Pure tray menu layout: tool order, per-tool visibility and the `tray_tools` settings line |
| src/platform/windows/power.rs | Execution-state flags, named power request, battery status and RAII cleanup |
| src/platform/windows/awake.rs | AlwaysOn session, deadline-aligned timer, toggle hotkey, end notice, battery policy and bound process |
| src/platform/windows/clock.rs | Local date/time to UTC FILETIME for AlwaysOn expiry, end-time labels |
| src/platform/windows/top/ | PowerToys-aligned pin, border z-order and last eligible window (module map in always-top.md) |
| src/platform/windows/frame.rs | PowerToys FrameDrawer D2D ring for the AlwaysTop border |
| src/screen_ruler.rs | Pure Screen Ruler modes, edge detection, measurement text and per-monitor `Marks` (press, drag, release, right click, copied spans); the overlay only carries out their decisions |
| src/platform/windows/clipboard.rs | Plain-text clipboard writes (ScreenRuler measurements, PowerStore ids) with a short retry while another app holds the clipboard |
| src/platform/windows/ruler/ | ScreenRuler on the UI thread: `mod.rs` session lifecycle (a session keeps the current `Config` plus its own tool and tolerance), hotkey, tool choice and copied text; `capture.rs` GDI surfaces and screen capture; `paint.rs` layered overlays; `input.rs` overlay mouse and keys; `toolbar.rs` toolbar window, placement and window procedure; `bar.rs` toolbar drawing, hit test and tooltips |
| src/image_resizer.rs | Pure Image Resizer sizes, names and batch plan |
| src/power_store.rs | Pure PowerStore package model, update filters, queue and export extras |
| src/json.rs | Bounded-depth JSON reader and string encoder shared by PowerStore parsers, export/import and the icon database |
| src/platform/windows/power_store/ | WinGet worker child, Packaged COM adapter, Scoop/Chocolatey/npm/pip/Cargo/.NET Tool/PowerShell/vcpkg CLI adapters, catalog fan-out, serial queue, periodic checks, history file and export/import dialogs (phase 3) |
| src/media_renamer.rs, src/media_renamer/ | Pure MediaRenamer formats, capture times, templates, grouping, collision planning, BMFF reader and rename history |
| src/power_mouse.rs | Pure PowerMouse triggers, actions, bindings and press state machine |
| src/platform/windows/power_mouse/ | WH_MOUSE_LL hook thread, SendInput injector, elevation pass-through and UI-thread Session |
| src/locale/power_mouse.rs | PowerMouse page and action picker strings (TR/EN/AR) |
| src/media_extensions.rs | MediaRenamer extension lists shared by the app and the shell DLL |
| src/platform/windows/media_renamer/ | MediaRenamer session, WIC/Property System probe, folder scan, journalled commit and undo, pickers, Explorer handoff and page snapshots |
| third_party/winget/ | Vendored `Microsoft.Management.Deployment.winmd` for WinGet marshaling |
| src/platform/windows/resize.rs | Image Resizer Explorer verb, second-instance handoff and resize jobs (the dialog is WinUI only) |
| src/platform/windows/resize_dialog.rs | WinUI 3 Image Resizer dialog (windows-reactor), opened beside the settings shell |
| src/platform/windows/shell_package.rs | Per-user sparse package for the Windows 11 menu (unpackaged builds only) |
| src/shell_menu.rs | Explorer command DLL for the compact menu and classic multi-select |
| src/platform/windows/resize_wic.rs | WIC decode, scale, crop and encode on a worker thread |
| src/platform/windows/resize_io.rs | Temp-file commit, replace backup and recycle |
| src/platform/windows/smoke.rs | Isolated native message-loop checks |
| src/settings.rs | Bounded settings parsing and atomic Windows replacement |
| brand/ | Product mark, tokens and platform artwork; Windows ICOs are embedded at build, `brand/windows/msix` feeds the package assets |
| packaging/msix/ | MSIX `AppxManifest.xml` template filled by `scripts/Package.ps1` |
| packaging/windows/ | End-user files included with the package |
| scripts/ | Build, checks and packaging helpers; no product behavior. Repository contracts and license notices live in the scripts crate so they do not compile Windows dependencies. |
| .github/ | Automation and contributor entry points |
| .agents/skills/ | Task-specific guidance for automated contributors |

New tools should first get a small domain module and explicit platform adapter.
Keep domain logic independently testable; avoid duplicating a shell or creating
empty modules for planned tools. Prefer narrow visibility over shared mutable state.

## Non-negotiable behavior

- Power requests must be removed on their owning UI thread, including error paths.
- Do not persist an active AlwaysOn session. Resume reconciles deadlines without extension.
- Do not persist an AlwaysTop pin. HWND values die with the target; every launch
  starts unpinned. Restore `HWND_NOTOPMOST` on the owning UI thread unless the
  window was already topmost when claimed.
- FFI belongs in the Windows adapter or the existing atomic settings writer.
  Explain pointer validity, ownership and thread affinity around unsafe operations.
- Network/download work stays off the UI thread. PowerKit has no in-app
  updater; Store/winget updates must not interrupt an active session.
- Stable package identity is PiriMedya.PowerKit (Partner Center identity once
  assigned). Product distribution is Microsoft Store plus winget pointing at the
  Store package; do not ship GitHub Releases or Setup.exe feeds as an install
  channel. No Store, winget, signing or GitHub credentials ship in the app.
- Settings are outside the installation directory. Preserve malformed settings until
  an explicit user change replaces them. A file without the `version=1` header is
  rejected as a whole; inside it, a known key with an invalid or repeated value is
  skipped (the user sees a partial-read notice) and the other lines still apply,
  and a key this build does not know is kept verbatim and written back on save,
  so running an older build does not erase a newer build's settings.
  Failures raised below the UI that the user sees travel as a typed `Fault` in
  `io::Error` (`platform/windows/fault.rs`) and are worded in the current
  language only when shown; OS errors keep the system's text. Persist AlwaysOn display sleep, the
  custom interval length, toggle hotkey, end notice and battery policy (not a
  running session, an expiry target, a bound process or the last mode), AlwaysTop
  sound, activation/opacity hotkeys and frame color/opacity/thickness, and
  ScreenRuler activation, default mode, capture, edge detection, units and line
  color, Image Resizer presets, custom size, filename, encoder and quality
  options, PowerStore update-check interval, update notification,
  ignored/skipped packages and per-package install options, and MediaRenamer
  default template, subfolder, modified-date fallback and lowercase-extension
  defaults, and PowerMouse enable plus per-trigger bindings (absent triggers
  are pass-through). PowerStore operation history and the MediaRenamer rename
  journal live in their own bounded files beside the settings.
  Do not persist pins, pinned-window opacity, an open measurement, the
  PowerStore operation queue, the MediaRenamer workspace list and selection or
  PowerMouse holds in progress.
- Window and notify-area icons come from brand/windows. powerkit.exe links
  PowerKit.ico as icon resource 1, which the Start menu shortcut and taskbar
  read as icon index 0. Tray glyphs follow the
  taskbar theme and AlwaysOn paused state. The tray popup stays a native Win32
  menu grouped by implemented tool: the root holds one entry per tool, then
  "Open PowerKit" and Exit. A tool with several commands is a submenu with a
  status label; a tool with one command is that command at the root. AlwaysOn
  uses a radio group for mode and a check for display sleep. AlwaysTop uses a
  check to pin the last eligible foreground window and a check for pin/opacity
  sound. ScreenRuler is a root item whose label and check show whether it is
  measuring. ImageResizer is a root "Resize images" item.
  PowerStore's submenu shows the update count in its label, a disabled status
  row, "Update all", "Check for updates" and "Open PowerStore". MediaRenamer's
  submenu shows the ready count in its label, a disabled status row and
  "Open MediaRenamer".
  `TrayLayout` (the `tray_tools` settings line) holds the order of the tool
  entries and whether each is shown; Settings edits it. A tool switched off on
  Home is absent whatever the layout says. Each tool entry carries the Segoe
  Fluent glyph of its rail page: `menu_icon.rs` draws it with GDI into a 32-bit
  premultiplied bitmap in the menu text color, owned until the popup is
  destroyed. A native menu row is as tall as its bitmap, so every item (also
  in submenus) gets a bitmap slightly taller than the icon, transparent when it
  has no icon; that is the only row spacing a non-owner-drawn menu allows.
  Every popup uses `MNS_CHECKORBMP`, so a checked item shows its check or radio
  mark in place of the bitmap. The menu shows no keyboard shortcut on root
  items, which keeps the root narrow; AlwaysTop's submenu still lists its
  shortcuts.
  The popup follows the same taskbar light/dark value, skips that
  under high contrast, and asks DWM for the small Windows 11 corner radius.
  The settings window is WinUI 3, rendered by windows-reactor: Desktop Acrylic
  with an extended `TitleBar` so chrome and content share one backdrop,   a fixed
  icon-over-label navigation rail (Home, PowerStore, MediaRenamer,
  AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer, PowerMouse) with Settings pinned at the rail foot, and card
  rows with ToggleSwitch (title on the left, no On/Off text on the track),
  full-width Accent/Subtle choice rows for exclusive options, shared HotkeyField
  capture buttons and WinUI ColorPicker flyouts that follow the Windows app theme.
  Hotkey capture
  sessions are typed `AppCommand` values; the Win32 key hook still lives in
  `winui_host`. Home (General page) is the landing surface: welcome hero, tool
  enable tiles, application info and embedded changelog (including whether the
  build runs unpackaged or from the MSIX). Settings holds language, theme, tray, startup,
  acrylic tint strength (`shell_opacity` overlay on Desktop Acrylic, not layered
  HWND alpha) and exit. The navigation rail matches
  Microsoft Store: near-square icon-over-label items (~64×58 DIP), two-line
  labels when needed, and the selected item shows only a filled icon over a
  fading chip with a 3×16 Accent indicator.   The main content keeps acrylic too,
  with an extra 50% SolidBackground over the shared shell tint (sibling layer so
  cards stay opaque) and a large top-left corner radius against the chrome.
  Page ScrollViewers disable horizontal scroll and stretch across the STAR column
  without `MaxWidth` (WinUI 3 shifts stretched `MaxWidth` children right once the
  viewport is wider than the cap, clipping controls on the right).
  windows-reactor sizes the client area with `ResizeClient` but keeps the
  `CW_USEDEFAULT` corner, so the `winui_host` subclass centres (and clamps) the
  settings window in its monitor work area on its first `SWP_SHOWWINDOW`.
  Chip and indicator stay mounted so opacity transitions run; the item
  button is keyed by selection. Scale, negative margins and Subtle foreground
  resource overrides are avoided — they crash WinUI (`0xc000027b`) with
  windows-reactor 0.100. The PowerKit logo and name sit over the TitleBar,
  left-aligned on its standard metrics.
  UI strings live in `src/locale.rs`. Do not restyle WinUI controls with brand
  colors or Inter Tight. Do not add empty submenus for planned tools.
- The shell and tray share `AppCommand`; both post the same `WM_COMMAND` ids
  through that adapter to a hidden Win32 app window. Preference writes, power
  requests and hotkeys stay there. The reactor loop replaces the Win32 message loop on the same UI thread.
  Closing the window hides it while the tray is available. To exit, the app
  window cleans up in `WM_DESTROY`, then closes the WinUI window, which ends the
  reactor loop; never post `WM_QUIT` while that window is alive. Product builds
  are MSIX and declare Windows App Runtime 2.4 or later as a Store package
  dependency; do not redistribute the runtime installer on the product channel.
  Prefer WinUI 3 for settings, dialogs and on-screen toolbars. The tray menu,
  hotkeys, AlwaysTop frame and ScreenRuler measurement overlays remain
  Win32/Direct2D. Unpackaged developer builds still need an installed runtime
  2.4+. The smoke test uses a plain Win32 loop and does not exercise WinUI. If
  WinUI fails to start (missing runtime or reactor error), PowerKit reports that
  and exits rather than continuing as a tray-only product session. Image
  Resizer has no Win32 dialog, so a pending `--resize` ends with that report.
- Future batch media operations require preview, deterministic collision handling
  and recoverable changes before touching original files.

## Design records

Document meaningful decisions here or in a focused new design document when needed.
Avoid creating an ADR for routine refactoring. Keep behavior and limitations explicit;
do not infer support for unimplemented tools.

Tool pages follow the utility or workspace pattern in
[Tool UI structure](ui-structure.md); new tools pick one before adding UI.

Distribution is Microsoft Store as the primary channel, with winget pointing at
the Store package. Packages are MSIX; Windows App Runtime 2.4+ is a Store
dependency rather than a redistributed installer. GitHub may host source, but
GitHub Releases / Setup.exe are not product install paths. Local MSIX trials
use a self-signed certificate (see [releasing](releasing.md#local-msix)); Store
publication re-signs the submitted package. The manifest declares the startup
task and the Explorer commands that unpackaged builds register at runtime, and
disables file-system write virtualization so settings stay in
`%LOCALAPPDATA%\PowerKit`. Prefer WinUI 3 for settings, dialogs and
on-screen toolbars; keep tray, hotkeys, measurement overlays and the AlwaysTop
frame on Win32/Direct2D.

AlwaysOn follows the PowerToys Awake modes except persistence: off, indefinite,
interval (30/60/120-minute presets plus a custom 1 minute to 99 h 59 min
interval) and expiry (keep awake until a local date and time). `Session` records
how a deadline was chosen so the tray and settings select the matching choice;
`Mode` alone drives `SetThreadExecutionState`. Deadlines are `GetTickCount64`
projections. An expiry also keeps its UTC FILETIME target, so `WM_TIMECHANGE`
and resume re-project it to follow the wall clock; intervals are never extended.
Without a date, an expiry time that already passed today means tomorrow; an
explicit past date is rejected. Editing the custom interval while it runs
restarts the countdown. Unlike Awake, PowerKit never resumes a session at
launch, and the tray only reports a running expiry rather than starting one.

`awake.rs` owns the running session on the UI thread. One timer fires when the
displayed minute changes or the deadline passes (every 5 s for a bound
process), instead of a 1 Hz tick. A process-bound session keeps an
`OpenProcess(SYNCHRONIZE)` handle so a reused pid cannot extend it; PowerKit
refuses to bind to itself. Battery policy (AC only, optional floor) blocks a
start and stops a running session on `PBT_APMPOWERSTATUSCHANGE` or a timer
tick; unknown AC status counts as AC. The end notice fires once per deadline
at 5 minutes and is suppressed when a session starts with less left; clicking
it extends by 30 minutes. A named `PowerCreateRequest` mirrors the
execution-state flags only so `powercfg /requests` shows a reason; failure to
create it is not an error. `--always-on` and `--always-on-pid` are parsed in the
domain module; a second instance forwards them through the registered
`PowerKit.AlwaysOn.Request` message (kind in `wParam`, value in `lParam`).

AlwaysTop follows the PowerToys Always On Top pin/border contract for any
number of windows: `SetProp` plus `SetWindowPos(HWND_TOPMOST, SWP_NOMOVE |
SWP_NOSIZE)`, a layered D2D frame sized from `DWMWA_EXTENDED_FRAME_BOUNDS`
inflated by 4 px, drawn with the FrameDrawer ring (half-thickness + 1 px
inset, accent color, DWM corner radius) and placed immediately behind each
tracked window. Pinning another window does not unpin the others.
The AlwaysTop page can rebind the activation and opacity hotkeys; defaults are
Win+Ctrl+T and Win+Ctrl++ / Win+Ctrl+- with Win+Alt fallbacks if those defaults
are taken. Opacity still steps between 20% and 100% in 10% increments; unpin
restores the original layered style. Frame color follows the system accent unless
the user picks a color; frame opacity and thickness are applied to the D2D ring
and persisted. Pin and opacity changes play the same Windows Media cues as
PowerToys when sound is enabled. A WinEvent hook records the last eligible
foreground window for the tray and reapplies topmost when focus or restore
drops the flag (PowerToys issues 17332 and 19168). The ring is shown only
while its window is visible, not minimized and not cloaked (hide, show, cloak,
minimize and uncloak events plus a 500 ms backup timer). Minimizing hides the
ring window and restoring shows the same one; it is destroyed only when the pin
ends. Its D2D frame repaints only when size, color, opacity, thickness or radius
change. Destroy events match the pinned HWND exactly. Unpinning drops the pin before restoring the window, and releasing
all pins tries every window. Shutdown releases the D2D factory on the UI
thread rather than in a thread-local destructor at process exit. Excluded-app lists,
game-mode blocking, system-menu items and UIAccess for elevated windows are
not implemented. Every window of PowerKit's own process (including the unowned
WinUI settings window), tool windows, cloaked surfaces and the shell taskbar
are skipped. Pins and opacity are not persisted.
Parity matrix, technical debt, the `top/` module map and optional
backlog live in [always-top](always-top.md).

The notify-area menu is still `CreatePopupMenu` / `TrackPopupMenu`, not a WinUI
flyout. Before it is shown, PowerKit selects uxtheme `SetPreferredAppMode`
(ordinal 135) from `SystemUsesLightTheme`, flushes cached menu colors, and uses a
thread-local `WH_CBT` hook so the `#32768` menu window can receive
`DWMWA_WINDOW_CORNER_PREFERENCE` / `DWMWA_USE_IMMERSIVE_DARK_MODE`. Missing
ordinals or DWM attributes leave the classic menu. High contrast leaves preferred
app mode untouched so system colors stay readable.

ScreenRuler follows the PowerToys Screen Ruler contract. The default activation
chord is Win+Ctrl+Shift+M, with a Win+Alt+Shift+M fallback when that default is
already taken. Opening shows a 240 by 50 DIP toolbar centered on the primary work area,
12 DIP from the top. The bar matches PowerToys (Bounds, Spacing, Horizontal
and Vertical icon toggles, a separator and a close glyph, with hover, the
accent checked state and localized tooltips) and adds a 16 DIP move grip of
six dots at the left edge, the only part that drags the bar. Pressing the grip
(full bar height up to half the following gap) hands the press to the system
move loop with `WM_NCLBUTTONDOWN`/`HTCAPTION`; the grip stays client area, so
it has a hover fill, a move cursor and a "Move" tooltip. When the drag ends
the bar is pulled fully into the nearest work area and that position is reused
by later sessions until PowerKit exits; it is not persisted. Crossing to a
monitor with another DPI resizes the bar on `WM_DPICHANGED` and moves the
tooltip areas. Overlays created after opening are put back below the bar.
Choosing a tool recaptures every monitor first, so the frozen image shows the
screen at that moment rather than when the bar opened. A display change
(`WM_DISPLAYCHANGE`, posted to the app as `ruler::DISPLAY_CHANGED`) rebuilds the
session with the same tool and drops marks. A failure inside a ruler window
procedure, such as an overlay that cannot be created, ends the session and is
reported by the app on `ruler::ENDED`, after the topmost overlays are gone;
`ENDED` also refreshes the tray status whenever a session ends on its own. A
Windows theme, high contrast or accent change repaints the open bar. The default mode is none until a
tool is chosen (Ctrl+1 through Ctrl+4). Choosing the active tool again clears it. Bounds is a
drag rectangle. The spacing tools walk each axis from the cursor until a pixel
is no longer close to the starting pixel: per channel means every channel is
within tolerance, otherwise the sum of channel differences is. Inclusive edges
make a 5×5 region report 5. Primary click copies the measurement and, unless
Shift is held, closes a spacing tool; Shift keeps the mark. Bounds stays open,
and Shift keeps the rectangle. Escape closes; in Bounds it also copies. Right
click cancels a drag, clears kept bounds, or closes. The wheel changes tolerance
by 15 for that session only. The label always includes pixels; an optional inch,
centimetre, millimetre or DIP value is shown beside it. Clipboard text uses only
the selected unit and omits the px suffix. The default line color is #FF4500.
Single-shot capture is the default; continuous capture refreshes the bitmap on
the UI thread every 33 ms, only while an edge tool is active and only for the
monitor under the cursor, reusing that monitor's capture surface (a click
recaptures it first). Pointer, wheel and capture updates repaint only the
monitor under the cursor, the monitor that last showed the crosshair and the
monitor whose marks changed; mode, preference and toolbar-position changes
repaint every overlay. Copied text converts each monitor's spans with that
monitor's own DPI and physical size. Capture is GDI BitBlt with the overlay excluded from capture, not
Windows.Graphics.Capture. Physical millimetres come from the display device caps
when Windows reports them. A measurement is not persisted.

Image Resizer follows the PowerToys resize contract: Small 854×480, Medium
1366×768, Large 1920×1080 and Phone 320×568, plus a custom size. Fit, Fill and
Stretch, pixel/percent/inch/centimetre units, shrink-only, replace, ignore
orientation, metadata removal, JPEG quality, PNG interlace, TIFF compression,
filename tokens and the JPEG/PNG/BMP/GIF/TIFF fallback encoder match that
contract. PowerKit still requires a complete preview before any write. Copy
mode never overwrites the source. Replace writes a same-directory temp file,
renames the original to `.bak`, then moves the temp into place and tries to
recycle the backup. WIC runs on a worker thread and encodes the first frame
only. An unpackaged build registers the classic Explorer verb under
`HKCU\Software\Classes\SystemFileAssociations\image\shell\PowerKit.ImageResizer`
with `MultiSelectModel=Document`. When `powerkit_shell.dll` is beside the
executable, that verb's `DelegateExecute` class receives every selected file in
one call. Windows 11's compact menu loads the same DLL from the per-user sparse
package `Pirimera.PowerKit.Shell`, whose external location is the install
directory. The command stays hidden unless the first selected item is an image,
then launches `powerkit.exe --resize` with the selected image paths. A command
line that would exceed the process limit is passed with `--resize-list`.
Registration runs off the UI thread and uses an unsigned sparse package.
Windows accepts that package when Developer Mode is on. If registration is
rejected, the classic verb remains. Uninstall of an unpackaged build does not
remove the per-user verb or the sparse package. The MSIX instead declares the
same DLL classes as `windows.fileExplorerContextMenus` with a COM surrogate, so
installing and removing the package adds and removes the Windows 11 commands;
it has no classic verb. AI upscaling, multi-frame GIF/TIFF and a preset editor
are not implemented.
