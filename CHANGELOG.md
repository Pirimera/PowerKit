# Changelog

Notable user-visible changes are recorded here. Release dates are added only after
publication; a source version is not proof of a published release.

## Unreleased

- Settings toggles keep the setting title on the left and no longer show On/Off text beside the switch. Choice groups (AlwaysOn modes, theme, language, ruler, Image Resizer sizes) use full-width Accent/Subtle option rows so the selected item is obvious. AlwaysOn page layout is reworked around a status card and clearer sections.
- Align AlwaysOn with PowerToys Awake: add a custom interval (hours and minutes, remembered between launches) to the tray and settings, and a "keep awake until" date/time card. Expiry end times follow system clock and time zone changes; intervals still never extend across sleep. Remaining time now reads in hours and minutes, and an expiry shows its end time.
- Add AlwaysOn extras: keep awake while an app runs (binds to the last active window's app), a toggle shortcut (Win+Ctrl+Shift+A), a notice 5 minutes before a timed session ends with a click-to-extend and an "Extend by 30 minutes" tray item, power conditions (only on AC, stop below a battery level), a named power request visible in `powercfg /requests`, and `--always-on` / `--always-on-pid` command-line control.
- Move AlwaysOn session, timer and hotkey handling into its own adapter; replace the 1 Hz timer with one that wakes on minute changes and deadlines, localize the remaining Turkish-only AlwaysOn and settings errors, and make preference changes during the smoke test skip the settings file.
- Rebuild the settings shell chrome: acrylic extends through the title bar (with the PowerKit logo and name left-aligned on WinUI TitleBar metrics), a fixed Microsoft Store-style navigation rail replaces the collapsible pane (near-square icon-over-label items with optional two-line labels; the selected item hides its label, shows a filled icon, and fades in a chip with a 3×16 Accent indicator), the main content keeps acrylic with an extra 50% tint (less transparent than the rail/chrome) and a large top-left radius, Settings moves to a footer page (language, theme, tray, startup, update prefs, exit), General stays the home/about surface, and `shell_opacity` tints the acrylic backdrop instead of making the whole window layered-transparent. Avoid Scale / negative-margin / Subtle foreground overrides on the rail — those crash WinUI (`0xc000027b`) with windows-reactor 0.100.
- Stop treating a missing Windows App Runtime / WinUI failure as a tray-only product mode: report the requirement, optionally finish a pending `--resize` with the Win32 dialog, then exit. The smoke test still uses its plain Win32 loop.
- Claim WinUI settings and dialog windows by an explicit `WindowRole` announced before create, instead of assuming the first top-level HWND is settings and the second is the Image Resizer dialog.
- Share a HotkeyField control across AlwaysTop and ScreenRuler settings; capture sessions use typed `AppCommand` values, and ScreenRuler gets its own measurement-shortcut label.
- Replace the Win32 `ChooseColorW` dialog with a WinUI 3 `ColorPicker` flyout on AlwaysTop frame and ScreenRuler line color swatches; chosen colors still apply through the shared `AppCommand` path.
- Route tray and settings actions through a typed `AppCommand` adapter while keeping the existing `WM_COMMAND` wire format.
- Align the General settings page with the shared card row API: ToggleSwitch rows replace update checkboxes and icon-only cards; the version/update expander stays.
- Split the WinUI settings shell into shared Fluent surfaces (`settings_ui`), per-tool pages and a thinner shell bridge so the Image Resizer dialog reuses the same card chrome.
- Choose Microsoft Store as the primary distribution channel with winget pointing at the Store package; ship MSIX, declare Windows App Runtime 2.4+ as a Store dependency, and stop treating GitHub Releases / Velopack as a product install path.
- Match the ScreenRuler toolbar to PowerToys: a compact icon bar on the primary work area, with hover, tooltips and a second click that clears the active tool.
- Put General first in settings navigation and open it by default; stack the update action under the version block so release notes wrap on narrow panes.
- Add Turkish, English and Arabic UI catalogs (plus Windows-default language); settings shell, tray menu and update status follow the selection.
- Use Desktop Acrylic on the settings window and Image Resizer dialog for Terminal-style translucent blur.
- Align the General settings page with PowerToys: version/update expander, optional Run-at-startup, theme and language cards, tray icon toggle, and update preference checkboxes.
- Rebuild the settings window with WinUI 3 (navigation pane, Mica, toggle switches and option groups). It requires the Windows App Runtime 2.4 or later. Image Resizer filename edits now apply with an explicit button.
- Open the Image Resizer dialog as a WinUI 3 window when the settings shell is available; keep the Win32 dialog for smoke and for a pending `--resize` when WinUI cannot start. The dialog reuses the settings icons and comes to the foreground when opened. Opening no longer clears the preview status until the user changes a setting. Layout follows the settings Fluent cards and toggle switches.
- Restyle the settings shell toward Fluent Design: soft navigation pills, Settings-card rows and segmented mode chips.
- Draw settings toggles as a smooth pill with a round thumb.
- Show the running version, architecture, update channel, package identity and settings file on the General page.
- Keep the PowerKit icon visible in the Start menu and on the taskbar after the first launch.
- Show Image Resizer in the Windows 11 Explorer menu and resize every selected image in one dialog.
- Add Image Resizer, aligned with PowerToys: the four default sizes, fit/fill/stretch, units, filename tokens, shrink-only, replace, orientation, metadata and encoder options. A preview is required before writing, and replace keeps a recoverable backup.

## 0.11.1

- clarify settings toggles and AlwaysOn mode selection

## 0.11.0

- align AlwaysOn with Awake and add process bind, hotkey, and power policy

## 0.10.0

- rebuild settings shell with Store-style chrome and acrylic tint

## 0.9.0

- match ScreenRuler toolbar to PowerToys and adopt Store distribution

## 0.8.0

- align General with PowerToys and add tr/en/ar UI

## 0.7.1

- write package checksums without utf8NoBOM

## 0.7.0

- rebuild settings and Image Resizer with WinUI 3

## 0.6.0

- restyle the settings shell toward Fluent Design

## 0.5.0

- show the running version on General and smooth settings toggles

## 0.4.1

- keep the PowerKit icon visible in the Start menu and on the taskbar

## 0.4.0

- show Image Resizer in the Windows Explorer menu

## 0.3.1

- release the WIC factory before COM shutdown

## 0.3.0

- add Image Resizer aligned with PowerToys

## 0.2.1

- Bump the stable SemVer on every normal commit: minor for feat, major for a breaking change, and patch otherwise.
- Start GitHub builds and releases only from a manual workflow dispatch. Main pushes, pull requests and tag pushes no longer package or publish the app.
- Add ScreenRuler, aligned with PowerToys Screen Ruler: Win+Ctrl+Shift+M, bounds and spacing tools, edge detection, clipboard copy, and the same measurement settings.
- Paint settings buttons with the app light/dark surfaces instead of classic white Win32 faces, and draw AlwaysOn display sleep plus AlwaysTop sound as Windows Settings-style toggles.
- Present AlwaysOn modes as a wrapping radio group (off, 30 minutes, 1 hour, 2 hours, indefinite) and add AlwaysTop hotkey plus frame color/opacity/thickness controls.
- Stop AlwaysTop from aborting when a hotkey pin paints the frame, which left the target topmost after the process died.
- Add AlwaysTop: pin the last eligible foreground window from the tray, show the title in the submenu and tip, and restore z-order on unpin, target close or exit. Pins are not saved.
- Pin the last focused window instead of the top z-order window, add Win+Ctrl+T (Win+Alt+T fallback) and draw an accent border around the pinned window.
- Align AlwaysTop pin, hotkey and frame z-order with PowerToys Always On Top so File Explorer stays with its border.
- Play PowerToys-matching pin and opacity sounds, and step pinned-window transparency with Win+Ctrl++ / Win+Ctrl+-.
- Draw the AlwaysTop pin frame with the PowerToys FrameDrawer D2D ring, accent color and rounded corners.
- Keep every pinned window on top, like PowerToys, instead of replacing the previous pin.
- Group the tray menu by tool: PowerKit actions at the root, AlwaysOn in a status-bearing submenu with a mode radio group and display check.
- Align the tray popup menu with the Windows taskbar light/dark theme.
- Tighten the Win32 AlwaysOn window module: named controls, shared labels, testable command mapping and tray icon reuse on tooltip ticks.
- Replace Python repository and license-notice helpers with a small Rust scripts crate.
- Add the PowerKit brand kit and use its Windows icons for the window, tray and installer.
- Standardize contributor documentation, MIT licensing and repository checks.
- Add discoverable development/release skills and maintainable agent instructions.
- Include generated third-party license notices in installer payloads.
- Enforce thread-local power guard ownership in the type system.
- Separate native power ownership and smoke checks from the Windows UI module.

## 0.2.0 — publication not verified

- Add ARM64/x64 Velopack installers and architecture-specific update channels.
- Download updates in the background and apply after normal exit.
- Add GitHub Actions packaging, package integrity tests and tag-triggered releases.

## 0.1.1

- Harden AlwaysOn cleanup, settings persistence, DPI and resume behavior.
- Add isolated native smoke checks.

## 0.1.0

- Initial native AlwaysOn prototype.
