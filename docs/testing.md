# Testing and troubleshooting

## Automated checks

~~~powershell
./scripts/Check.ps1 -Architecture arm64
cargo build --locked --release --target aarch64-pc-windows-msvc
./scripts/Test-AlwaysOn.ps1
~~~

Check.ps1 checks repository metadata/links via the scripts crate, rustfmt, unit tests and Clippy.
`scripts/Package.ps1` builds the MSIX from release binaries; see
[releasing](releasing.md#local-msix).

Smoke checks use a separate mutex/window class, skip user preferences, and
release their power request on their own thread. They may briefly show a test
window. They are not a substitute for real sleep or installed-package testing.

## Physical acceptance

### Install

- Install the signed test MSIX; verify Start menu and Installed apps entries,
  that start-with-Windows toggles the PowerKit entry in Settings > Apps >
  Startup, and that the Windows 11 Explorer menu shows Image Resizer and
  MediaRenamer.

### AlwaysOn

- Exercise indefinite/timed AlwaysOn with display option on and off, including a
  custom interval (edit it while running: the countdown restarts) and an expiry
  with and without a date. Change the system clock or time zone during an expiry:
  the wall-clock end time must hold.
- Verify stop/exit restores idle sleep; test battery and AC independently.
  With "Only when plugged in", unplugging must stop AlwaysOn and starting on
  battery must be refused; with a battery floor, AlwaysOn must stop below it.
- Bind AlwaysOn to an app, close the app, and confirm the session ends within
  a few seconds. Start a short custom interval with the end notice on and click
  the notification: the deadline moves by 30 minutes. Press Win+Ctrl+Shift+A to
  toggle. While a session runs, an elevated `powercfg /requests` lists
  "PowerKit AlwaysOn". Run `powerkit --always-on 5` with and without a running
  instance.

### Window, settings and tray

- Test manual sleep/resume, tray hide/show, Explorer restart and different monitor DPI.
  Confirm the branded icon on the window, in the Start menu shortcut and on the
  taskbar, including after a second launch, and the light/dark/paused tray glyphs.
- Open the main window in light and dark app themes, including a live switch
  and high contrast: title-bar chrome and the settings pages should follow
  AppsUseLightTheme, with Home first in the navigation rail, then the tool pages, and Settings at its foot.
  Home shows the welcome hero, tool cards with enable toggles, package facts and
  the changelog; Settings holds language/theme, tray visibility, startup,
  transparency and exit. Switching theme should restyle the
  settings window; disabling the tray icon should remove it and still allow
  reopen via a second Start menu launch. With Start with Windows on and the
  tray icon shown, signing in (or launching with `--background`) must leave
  PowerKit in the tray without opening settings and without starting WinUI yet
  (no `WinUIDesktopWin32WindowClass`); a Start or tray open then loads the shell.
  A normal Start-menu launch still opens settings immediately.
  Navigation should use soft selected pills; toggles and preference rows should read as Settings cards.
- Open the tray menu in light and dark taskbar themes, including a live switch;
  it should follow the taskbar colors, and each tool entry should show its rail
  icon in the menu text color (at 100% and a scaled DPI). ScreenRuler and
  ImageResizer are single root items; while measuring, ScreenRuler shows a check
  in place of its icon. In Settings, the tray menu list should hide a tool with
  its switch and reorder with the up/down buttons; reopen the menu to confirm,
  and restart PowerKit to confirm the layout was saved. The Shortcuts submenu
  must list the registered shortcuts of enabled tools with their keys, follow a
  changed shortcut, drop a tool switched off on Home, and open the matching
  tool page when an entry is chosen; its Settings switch must hide it while the
  shortcuts keep working. Resting the pointer on AlwaysOn, the AlwaysTop pin
  and opacity commands or ScreenRuler must show the shortcut in a tooltip above
  the menu, which disappears when the pointer moves on or the menu closes.
  Recording a chord that another PowerKit shortcut uses must keep the old chord
  and name the holder under the card. With a chord held by another app (for
  example a second instance of a hotkey tool), the card must say the shortcut is
  inactive. After switching ScreenRuler off and on on Home, its shortcut must
  work without a restart; a tool started switched off must not hold its chord.
  With language, theme,
  transparency, tray icon, tray layout and startup changed, "Restore default
  settings" must do nothing on Cancel; on Reset it must restore those values
  (startup leaves Settings > Apps > Startup) and leave tool settings untouched.
- AlwaysOn is a submenu whose root label
  shows remaining time (or the end time of an expiry); mode uses a radio group
  that includes the custom interval and, while one runs, a disabled expiry item;
  display sleep uses a check.

### AlwaysTop

- AlwaysTop is a submenu whose root label names the pinned window; the check
  pins the last focused eligible window, not the top z-order Explorer window.
  Pinning a second window must leave the first pinned.
  The AlwaysTop page can assign activation and opacity hotkeys and set frame
  color, opacity and thickness. Defaults are Win+Ctrl+T and Win+Ctrl++ /
  Win+Ctrl+- (or the fallback shown in the menu). The shortcut pins the
  foreground window and draws the configured frame immediately behind it,
  matching PowerToys.
  File Explorer must stay
  above other apps with that frame; the pin is reapplied if the shell drops
  topmost. Unpin with the same check or shortcut, by closing that window, or
  by exiting PowerKit. A new launch must start unpinned. Win+Ctrl++ / Win+Ctrl+-
  must change only the pinned window's opacity and restore it on unpin. The
  Ses çal check should play or silence the pin and opacity cues and persist.

### ScreenRuler

- ScreenRuler opens from Win+Ctrl+Shift+M or its tray command. The toolbar is the
  compact PowerToys icon bar with a dotted move grip at its left edge. Only the
  grip moves it (move cursor, hover fill and a Move tooltip); pressing a gap
  between buttons must not move it. Open the ruler from the settings page,
  switch to another app, then choose a tool: the frozen image must show that
  app, not the PowerKit window. With the ruler open, connecting or removing a
  monitor (or changing resolution) must reopen it with the same tool on the new
  layout, and switching Windows light/dark or high contrast must recolor the
  bar at once; closing the ruler from its own close button must update the tray
  status. A drop
  past a screen edge or under the taskbar snaps back inside the work area,
  dragging onto a monitor with another scale keeps the bar's DIP size and
  tooltips, choosing a tool after opening with no mode keeps the bar visible,
  and the next session opens where the bar was left. With a DIP or physical
  extra unit, Escape in Bounds on a scaled (for example 150%) monitor must copy
  the same values the labels show; with continuous capture the crosshair must
  follow changing content and leave no stale cross on the monitor the cursor
  left. Ctrl+1 through
  Ctrl+4 select bounds, spacing, horizontal and vertical; choosing the active tool
  again clears it. A primary click copies
  the measurement; Shift keeps it; Escape closes. The wheel changes tolerance by
  15 for that session. Confirm the settings page shortcut, default mode,
  continuous capture, per-channel detection, tolerance, extra units, feet and
  line color, then launch again with no measurement still open.

### ImageResizer

- Image Resizer opens from its tray command, the Windows 11 compact Explorer
  menu, or Show more options. Several selected images open one dialog. Changing
  a size invalidates the preview; Resize stays disabled until every file
  previews without error. Copy mode must not overwrite the source. Replace must
  leave the new file in place and the previous bytes in a `.bak` that is then
  sent to the recycle bin when that succeeds. Confirm the settings page
  filename, JPEG quality, fallback encoder, PNG interlace, TIFF compression,
  metadata removal and keep-date, then launch again with those
  values still saved. With Developer Mode on, the compact command appears after
  registration; Explorer may need a restart. If registration is rejected, the
  classic verb still opens one dialog for the whole selection.

### MediaRenamer

- MediaRenamer: work only on copies of test media, never on originals. Add
  files, a folder (with and without subfolders) and dropped items; confirm HEIC
  plus Live Photo MOV pairs and `.xmp`/`.aae` sidecars stay one row, that RAW
  and missing-codec files show their state, that OneDrive online-only files are
  listed as such and not downloaded, and that two photos from the same second
  get the sub-second or `_1` suffix. Rename, then Undo from History; kill
  PowerKit during a large rename and confirm the next launch shows the batch as
  interrupted with Undo for what was renamed. From Explorer, select many files
  and a folder in both menus with unapplied rows open: one confirmation, one
  combined preview.

### PowerStore

- Open PowerStore: the last saved list appears at once and the managers
  refresh in the background; a manager that is not installed is not shown.
  Filter by text, source, status and scope, and sort by name, id, install
  date and last update; updates stay pinned above installed apps. Open
  Discover, search, and confirm results use the same table.
- Install, update and uninstall one small package per available manager
  (WinGet through the helper program, and Scoop, Chocolatey, npm, pip, Cargo,
  .NET tool, PowerShell or vcpkg where installed). Queue several operations:
  they run one at a time, and cancelling one leaves the rest queued. Record
  which managers were exercised.
- The Package Info, History and Settings drawers open from the trailing edge
  and close on a click outside. Switch a manager off in Settings: its
  packages leave the list. Export the list and import it again.
- Tray submenu: the status row and count, "Update all" (enabled only with
  updates), "Check for updates" and "Open PowerStore". After a periodic check
  finds new updates, the notification opens PowerStore when clicked.
- Still open (see [PowerStore design](power-store.md#open-questions)): an
  `msstore` install, installers that raise UAC, cancel after an external
  installer window has appeared, and WinGet activation inside an installed
  MSIX.

### PowerMouse

- PowerMouse, generic layer (any mouse; record the model and connection):
  after signing in, saved bindings work without opening PowerKit. Middle,
  Back and Forward each as single action, hold down (a held shortcut such as
  push-to-talk stays down until release), long press (short and long) and
  hold-and-swipe (the cursor stays still; a slow 50 px swipe fires once).
  Horizontal scroll fires once per uninterrupted scroll. Typed text with
  Turkish characters, a workflow with a pause, a recorded shortcut. The
  vertical wheel is never changed by PowerMouse; the Scrolling and pointer
  tab's link opens Windows mouse settings. F13–F24 from a device that sends them.
  The tray item switches PowerMouse off and on. With an elevated window in front, buttons keep their
  own function; back in a normal window, remapping returns. Lock and unlock,
  sleep and resume, and switch to another user and back: remapping stops
  while away and returns. Exit while a mirrored button is held: nothing stays
  pressed. Precision touchpad: note whether horizontal scrolling and
  three-finger middle click are remapped.
- PowerMouse Actions Ring: open it from a bound button; hover highlights a
  slot; a click runs the slot's action in the application underneath, which
  keeps focus; the centre, outside, Esc, a right click and 10 s close it;
  opening it again while open closes it; at a screen edge it stays inside the
  work area; on a 150 % monitor labels stay legible.
- PowerMouse with MX Master 4 (Bluetooth; Logi Options+, OpenLogi and MX4Lite
  not running): the page shows the device, connection and battery. Gesture
  button, mode shift button and haptic panel each with a single action and
  with hold-and-swipe (device movement, no cursor travel); an unbound button
  keeps its own function. DPI presets and "keep" change and stop changing the
  pointer speed; wheel mode ratchet, free spin and SmartShift; high-resolution
  scrolling. Thumb wheel bound in one direction stops side scrolling and
  fires once per turn; check left and right are not swapped. Haptic tap, Next
  DPI, and Easy-Switch to a second paired computer and back. Mouse asleep,
  then woken: bindings and settings return. With MX4Lite or Options+ started,
  the page names it and the buttons return to the mouse. A tray notice at
  15 % battery.

### Recovery, upgrade and uninstall

- PowerKit recovery: with the packaged build running for over a minute, end
  `powerkit.exe` abnormally (Task Manager's "End task" is a normal end and
  does not count; a crash does) and confirm PowerKit returns within about
  ten seconds with PowerMouse working.
- Upgrade between two installed versions; preserve display, sound, hotkey,
  frame preferences, ScreenRuler settings and Image Resizer preferences, and start AlwaysOn, AlwaysTop and ScreenRuler off.
- Upgrade the installed MSIX with a newer test package while AlwaysOn runs and
  verify the session is not force-restarted.
- Install and uninstall a local self-signed test MSIX before Store submission;
  confirm the Windows App Runtime dependency is declared rather than
  redistributed and that PowerStore's WinGet worker still connects.
- After Store publication, verify Store install/update and winget against the Store
  package. Do not treat GitHub Releases as a product install path.
- Uninstall and verify app process, shortcuts and registration are removed.
  User preferences intentionally remain outside the installation root.

Keep other sleep blockers off when measuring sleep behavior. Record native versus
emulated architecture, Windows build, source revision and observed outcome.
A result applies to the revision it was observed on; do not present it as
evidence for a later build. The design documents list what has been verified
for [PowerMouse](power-mouse.md#verification-status) and
[PowerStore](power-store.md#winget-com-integration).

## Common issues

- Raw development executable: Store/winget updates apply only to a Store-installed
  package. Local unpackaged builds do not receive product updates.
- Local MSIX: sign with the self-signed test certificate and trust it in
  LocalMachine\TrustedPeople. Store packages are re-signed by the Store and will
  not share identity with that test install.
- Missing Windows App Runtime 2.4+ on unpackaged developer builds: PowerKit
  reports the requirement and exits instead of staying tray-only; an Explorer
  Image Resizer launch shows the same report and resizes nothing. Install the
  runtime or use a packaged Store/test MSIX that declares the dependency.
- Power request testing: timers, Windows sleep settings and other apps affect observations.
