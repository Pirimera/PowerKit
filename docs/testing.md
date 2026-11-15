# Testing and troubleshooting

## Automated checks

~~~powershell
./scripts/Check.ps1 -Architecture arm64
cargo build --locked --release --target aarch64-pc-windows-msvc
./scripts/Test-AlwaysOn.ps1
~~~

Check.ps1 checks repository metadata/links via the scripts crate, rustfmt, unit tests and Clippy.
The real-package test is intentionally ignored by ordinary cargo test because it
needs vpk output; the release workflow runs it explicitly after packaging.
See [releasing](releasing.md) for that command.

Smoke checks use a separate mutex/window class, skip user preferences and update
networking, and release their power request on their own thread. They may briefly
show a test window. They are not a substitute for real sleep or installer testing.

## Physical acceptance

- Install ARM64 Setup.exe; verify Start menu and Installed apps entries.
- Exercise indefinite/timed AlwaysOn with display option on and off.
- Verify stop/exit restores idle sleep; test battery and AC independently.
- Test manual sleep/resume, tray hide/show, Explorer restart and different monitor DPI.
  Confirm the branded icon on the window, in the Start menu shortcut and on the
  taskbar, including after a second launch, and the light/dark/paused tray glyphs.
  Open the main window in light and dark app themes, including a live switch
  and high contrast: title-bar chrome and the settings pages should follow
  AppsUseLightTheme, with AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer and General navigation.
  The General page shows the update expander, startup toggle, language/theme cards,
  tray visibility, package facts and exit. Switching theme should restyle the
  settings window; disabling the tray icon should remove it and still allow
  reopen via a second Start menu launch.
  Navigation should use soft selected pills; toggles and preference rows should read as Settings cards.
  Open the tray menu in light and dark taskbar themes, including a live switch;
  it should follow the taskbar colors. AlwaysOn is a submenu whose root label
  shows remaining time; mode uses a radio group and display sleep uses a check.
  AlwaysTop is a submenu whose root label names the pinned window; the check
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
  ScreenRuler opens from Win+Ctrl+Shift+M or its tray command. Ctrl+1 through
  Ctrl+4 select bounds, spacing, horizontal and vertical. A primary click copies
  the measurement; Shift keeps it; Escape closes. The wheel changes tolerance by
  15 for that session. Confirm the settings page shortcut, default mode,
  continuous capture, per-channel detection, tolerance, extra units, feet and
  line color, then launch again with no measurement still open.
  Image Resizer opens from its tray command, the Windows 11 compact Explorer
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
- Upgrade between two installed versions; preserve display, sound, hotkey,
  frame preferences, ScreenRuler settings and Image Resizer preferences, and start AlwaysOn, AlwaysTop and ScreenRuler off.
- When an update is ready, verify an active session is not interrupted.
- Test offline/private-repo behavior and interrupted downloads.
- Uninstall and verify app process, shortcuts and registration are removed.
  User preferences intentionally remain outside the installation root.

Keep other sleep blockers off when measuring sleep behavior. Record native versus
emulated architecture, Windows build, source revision and observed outcome.
Historical evidence lives in [validation](validation/README.md); never overwrite
earlier results to imply they apply to a new build.

## Common issues

- Raw development executable: automatic updates require a Velopack installation.
- Private repository: anonymous update checks cannot access releases; install manually.
- Unsigned installer: publisher identity is not verified. Signing is a release task;
  do not recommend disabling Windows protection.
- Missing clang: check LLVM bin is on PATH (or POWERKIT_LLVM_ROOT for the portable helper).
- Power request testing: timers, Windows sleep settings and other apps affect observations.
