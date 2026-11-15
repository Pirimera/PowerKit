# Architecture

PowerKit is one Rust binary crate. Keep it this way until independently reusable
components or independently deployable programs justify a workspace. There is no
plugin loader, background service or web runtime.

## Boundaries

| Location | Responsibility |
| --- | --- |
| src/main.rs | Velopack startup hook, then platform entry point |
| src/always_on.rs | Pure mode/deadline transitions |
| src/always_top.rs | Pure pin session and status title |
| src/platform/windows/mod.rs | UI thread, hidden app window, tray, commands, lifecycle |
| src/platform/windows/window.rs | WinUI 3 settings shell (windows-reactor): navigation and per-tool pages |
| src/platform/windows/winui_host.rs | Finds the WinUI HWND; hide-on-close, start hidden, exit and key capture |
| src/platform/windows/icons.rs | Branded window and notify-area icons from brand/windows ICOs |
| src/platform/windows/theme.rs | Taskbar popup theme plus app light/dark colors for Win32 overlays |
| src/platform/windows/menu.rs | App-grouped tray menu spec and native popup construction |
| src/platform/windows/power.rs | Execution-state flags and RAII cleanup |
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
| src/updates.rs | Worker, update state, architecture channel and pending package |
| brand/ | Product mark, tokens and platform artwork; Windows ICOs are embedded at build |
| packaging/windows/ | End-user files included in the installer |
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
- Stable package identity is Pirimera.PowerKit; channels are win-arm64 and win-x64.
  No GitHub credentials ship in the app. Do not weaken integrity checks.
- Settings are outside the installation directory. Preserve malformed settings until
  an explicit user change replaces them. Persist AlwaysOn display sleep, AlwaysTop
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
  The settings window is WinUI 3, rendered by windows-reactor: a NavigationView
  with AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer and General pages, Mica,
  and card rows with ToggleSwitch, RadioButtons, hotkey and color buttons that
  follow the Windows app theme. General shows the running version, CPU
  architecture, update channel, package identity and settings file, then the
  update check and the explicit exit control. Do not restyle WinUI controls with
  brand colors or Inter Tight. Do not add empty submenus for planned tools.
- The shell renders snapshots and posts the same `WM_COMMAND` ids as the tray to
  a hidden Win32 app window; preference writes, power requests and hotkeys stay
  there. The reactor loop replaces the Win32 message loop on the same UI thread.
  Closing the window hides it while the tray is available. To exit, the app
  window cleans up in `WM_DESTROY`, then closes the WinUI window, which ends the
  reactor loop; never post `WM_QUIT` while that window is alive. The app uses the
  installed Windows App Runtime 2.x (2.4 or later, framework-dependent) and does
  not ship or install it. Without it, reactor offers the runtime download and
  PowerKit continues tray-only on a plain Win32 loop. The smoke test uses that
  plain loop and does not exercise WinUI. The tray menu, AlwaysTop frame,
  ScreenRuler overlay and color picker remain Win32. The Image Resizer dialog
  is WinUI 3 when the settings shell is running (Fluent cards and toggles,
  same Mica theme), and falls back to the Win32 dialog in tray-only mode.
- Future batch media operations require preview, deterministic collision handling
  and recoverable changes before touching original files.

## Design records

Document meaningful decisions here or in a focused new design document when needed.
Avoid creating an ADR for routine refactoring. Keep behavior and limitations explicit;
do not infer support for unimplemented tools.

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
already taken. Opening shows a toolbar; the default mode is none until Bounds,
Spacing, Horizontal or Vertical is chosen (Ctrl+1 through Ctrl+4). Bounds is a
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
