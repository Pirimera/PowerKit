# Changelog

Notable user-visible changes are recorded here. Release dates are added only after
publication; a source version is not proof of a published release.

## Unreleased

- Put General first in settings navigation and open it by default; stack the update action under the version block so release notes wrap on narrow panes.
- Add Turkish, English and Arabic UI catalogs (plus Windows-default language); settings shell, tray menu and update status follow the selection.
- Use Desktop Acrylic on the settings window and Image Resizer dialog for Terminal-style translucent blur.
- Align the General settings page with PowerToys: version/update expander, optional Run-at-startup, theme and language cards, tray icon toggle, and update preference checkboxes.
- Rebuild the settings window with WinUI 3 (navigation pane, Mica, toggle switches and option groups). It requires the Windows App Runtime 2.4 or later; without it PowerKit runs from the notification area. Image Resizer filename edits now apply with an explicit button.
- Open the Image Resizer dialog as a WinUI 3 window when the settings shell is available; keep the Win32 dialog for tray-only mode. The dialog reuses the settings icons and comes to the foreground when opened. Opening no longer clears the preview status until the user changes a setting. Layout follows the settings Fluent cards and toggle switches.
- Restyle the settings shell toward Fluent Design: soft navigation pills, Settings-card rows and segmented mode chips.
- Draw settings toggles as a smooth pill with a round thumb.
- Show the running version, architecture, update channel, package identity and settings file on the General page.
- Keep the PowerKit icon visible in the Start menu and on the taskbar after the first launch.
- Show Image Resizer in the Windows 11 Explorer menu and resize every selected image in one dialog.
- Add Image Resizer, aligned with PowerToys: the four default sizes, fit/fill/stretch, units, filename tokens, shrink-only, replace, orientation, metadata and encoder options. A preview is required before writing, and replace keeps a recoverable backup.

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
