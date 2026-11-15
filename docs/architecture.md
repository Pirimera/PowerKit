# Architecture

PowerKit is one Rust binary crate. Keep it this way until independently reusable
components or independently deployable programs justify a workspace. There is no
plugin loader, background service or web runtime.

## Boundaries

| Location | Responsibility |
| --- | --- |
| src/main.rs | Platform entry point (transitional Velopack hook until Store MSIX lands) |
| src/always_on.rs | Pure session, interval and expiry deadline transitions |
| src/always_top.rs | Pure pin session and status title |
| src/platform/windows/mod.rs | UI thread, hidden app window, tray, commands, lifecycle |
| src/platform/windows/command.rs | Typed `AppCommand` plus WM_COMMAND id adapter shared by tray and settings |
| src/platform/windows/window/ | WinUI 3 settings shell (windows-reactor): navigation, bridge and command dispatch |
| src/platform/windows/window/pages.rs | Per-tool settings pages composed from shared surfaces |
| src/platform/windows/settings_ui.rs | Shared Fluent card/stack/stepper/hotkey/color surfaces for shell and dialogs |
| src/platform/windows/winui_host.rs | Claims WinUI HWNDs by explicit `WindowRole`; hide-on-close, start hidden, exit and key capture |
| src/platform/windows/icons.rs | Branded window and notify-area icons from brand/windows ICOs |
| src/platform/windows/theme.rs | Taskbar popup theme plus app light/dark colors for Win32 overlays |
| src/platform/windows/startup.rs | Optional HKCU Run registration for start with Windows |
| src/locale.rs | Turkish/English/Arabic string catalogs and Windows UI language resolution |
| src/platform/windows/menu.rs | App-grouped tray menu spec and native popup construction |
| src/platform/windows/power.rs | Execution-state flags, named power request, battery status and RAII cleanup |
| src/platform/windows/awake.rs | AlwaysOn session, deadline-aligned timer, toggle hotkey, end notice, battery policy and bound process |
| src/platform/windows/clock.rs | Local date/time to UTC FILETIME for AlwaysOn expiry, end-time labels |
| src/platform/windows/top.rs | PowerToys-aligned pin, border z-order and last eligible window |
| src/platform/windows/frame.rs | PowerToys FrameDrawer D2D ring for the AlwaysTop border |
| src/screen_ruler.rs | Pure Screen Ruler modes, edge detection and measurement text |
| src/platform/windows/ruler.rs | ScreenRuler overlay, capture and toolbar |
| src/image_resizer.rs | Pure Image Resizer sizes, names and batch plan |
| src/platform/windows/resize.rs | Image Resizer Explorer verb, second-instance handoff, Win32 dialog fallback and resize jobs |
| src/platform/windows/resize_dialog.rs | WinUI 3 Image Resizer dialog (windows-reactor), opened beside the settings shell |
| src/platform/windows/shell_package.rs | Per-user sparse package for the Windows 11 menu |
| src/shell_menu.rs | Explorer command DLL for the compact menu and classic multi-select |
| src/platform/windows/resize_wic.rs | WIC decode, scale, crop and encode on a worker thread |
| src/platform/windows/resize_io.rs | Temp-file commit, replace backup and recycle |
| src/platform/windows/smoke.rs | Isolated native message-loop checks |
| src/settings.rs | Bounded settings parsing and atomic Windows replacement |
| src/updates.rs | Transitional in-app update worker; Store builds prefer Store/winget updates |
| brand/ | Product mark, tokens and platform artwork; Windows ICOs are embedded at build |
| packaging/windows/ | End-user files included with the package (migrate with MSIX) |
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
- Network/download work stays off the UI thread. A pending update must not interrupt
  an active session. Start the update helper only after power cleanup.
- Stable package identity is Pirimera.PowerKit (Partner Center identity once
  assigned). Product distribution is Microsoft Store plus winget pointing at the
  Store package; do not ship GitHub Releases or Velopack feeds as an install
  channel. No Store, winget, signing or GitHub credentials ship in the app.
- Settings are outside the installation directory. Preserve malformed settings until
  an explicit user change replaces them. Persist AlwaysOn display sleep, the
  custom interval length, toggle hotkey, end notice and battery policy (not a
  running session, an expiry target, a bound process or the last mode), AlwaysTop
  sound, activation/opacity hotkeys and frame color/opacity/thickness, and
  ScreenRuler activation, default mode, capture, edge detection, units and line
  color, and Image Resizer presets, custom size, filename, encoder and quality
  options. Do not persist pins, pinned-window opacity, or an open measurement.
- Window and notify-area icons come from brand/windows. powerkit.exe links
  PowerKit.ico as icon resource 1, which the Start menu shortcut and taskbar
  read as icon index 0. Tray glyphs follow the
  taskbar theme and AlwaysOn paused state. The tray popup stays a native Win32
  menu grouped by implemented tool: the root holds PowerKit actions and a status
  label on each tool submenu. AlwaysOn uses a radio group for mode and a check
  for display sleep. AlwaysTop uses a check to pin the last eligible foreground
  window and a check for pin/opacity sound. ScreenRuler uses a check to start or
  stop measuring. ImageResizer has its own submenu with one resize command.
  The popup follows the same taskbar light/dark value, skips that
  under high contrast, and asks DWM for the small Windows 11 corner radius.
  The settings window is WinUI 3, rendered by windows-reactor: Desktop Acrylic
  with an extended `TitleBar` so chrome and content share one backdrop, a fixed
  icon-over-label navigation rail (General/Home, AlwaysOn, AlwaysTop,
  ScreenRuler, ImageResizer) with Settings pinned at the rail foot, and card
  rows with ToggleSwitch (title on the left, no On/Off text on the track),
  full-width Accent/Subtle choice rows for exclusive options, shared HotkeyField
  capture buttons and WinUI ColorPicker flyouts that follow the Windows app theme.
  Hotkey capture
  sessions are typed `AppCommand` values; the Win32 key hook still lives in
  `winui_host`. General is the home surface (version, update action, release
  notes, package identity). Settings holds language, theme, tray, startup,
  acrylic tint strength (`shell_opacity` overlay on Desktop Acrylic, not layered
  HWND alpha), update preference toggles and exit. The navigation rail matches
  Microsoft Store: near-square icon-over-label items (~64×58 DIP), two-line
  labels when needed, and the selected item shows only a filled icon over a
  fading chip with a 3×16 Accent indicator. The main content keeps acrylic too,
  with an extra 50% SolidBackground over the shared shell tint (sibling layer so
  cards stay opaque) and a large top-left corner radius against the chrome.
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
  and exits rather than continuing as a tray-only product session; a pending
  `--resize` may still open the Win32 Image Resizer once before teardown.
- Future batch media operations require preview, deterministic collision handling
  and recoverable changes before touching original files.

## Design records

Document meaningful decisions here or in a focused new design document when needed.
Avoid creating an ADR for routine refactoring. Keep behavior and limitations explicit;
do not infer support for unimplemented tools.

Distribution is Microsoft Store as the primary channel, with winget pointing at
the Store package. Packages are MSIX; Windows App Runtime 2.4+ is a Store
dependency rather than a redistributed installer. GitHub may host source, but
GitHub Releases / Velopack Setup.exe are not product install paths. Local MSIX
trials use a self-signed certificate and Developer Mode; Store publication
re-signs the submitted package. Prefer WinUI 3 for settings, dialogs and
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
drops the flag (PowerToys issues 17332 and 19168). Excluded-app lists,
game-mode blocking, system-menu items and UIAccess for elevated windows are
not implemented. PowerKit's own window, owned dialogs, tool windows, cloaked
surfaces and the shell taskbar are skipped. Pins and opacity are not persisted.

The notify-area menu is still `CreatePopupMenu` / `TrackPopupMenu`, not a WinUI
flyout. Before it is shown, PowerKit selects uxtheme `SetPreferredAppMode`
(ordinal 135) from `SystemUsesLightTheme`, flushes cached menu colors, and uses a
thread-local `WH_CBT` hook so the `#32768` menu window can receive
`DWMWA_WINDOW_CORNER_PREFERENCE` / `DWMWA_USE_IMMERSIVE_DARK_MODE`. Missing
ordinals or DWM attributes leave the classic menu. High contrast leaves preferred
app mode untouched so system colors stay readable.

ScreenRuler follows the PowerToys Screen Ruler contract. The default activation
chord is Win+Ctrl+Shift+M, with a Win+Alt+Shift+M fallback when that default is
already taken. Opening shows a 216 by 50 DIP toolbar centered on the primary work area,
12 DIP from the top. The bar matches PowerToys: Bounds, Spacing, Horizontal
and Vertical icon toggles, a separator and a close glyph, with hover, the
accent checked state and localized tooltips. The default mode is none until a
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
the UI thread. Capture is GDI BitBlt with the overlay excluded from capture, not
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
only. The running app registers the classic Explorer verb under
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
rejected, the classic verb remains. AI upscaling, multi-frame GIF/TIFF and a preset editor
are not implemented. Uninstall does not remove the per-user verb or the sparse
package.
