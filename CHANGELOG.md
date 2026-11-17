# Changelog

Notable user-visible changes are recorded here. Release dates are added only after
publication; a source version is not proof of a published release.

## Unreleased

- Localize the Image Resizer WinUI/Win32 dialog, preview/write status lines, file picker, Explorer verb and shell-menu title in Turkish, English and Arabic (including size preset names used in previews and output file names), and AlwaysTop pin/unpin user errors.
- ImageResizer stays a utility: the page leads with File Explorer instructions and demotes “Select images here…” to a Subtle fallback; General no longer offers a resize quick action.
- Apply the utility/workspace page pattern across the shell: AlwaysOn, AlwaysTop and ScreenRuler pages group settings under section headings with a launch/status card first; General lists every tool with status and Open (plus a quick action where one exists); the navigation rail puts PowerStore after General and before the utilities; the Settings page title reads "PowerKit settings".
- Standardize tool pages as utilities (settings page, work done from tray, hotkey or a task window) or workspaces (work on the page, settings in a drawer); see `docs/ui-structure.md`. The PowerStore `Manage` button and drawer are now `Settings` / "PowerStore settings", grouped into package managers, update checks, appearance, backup, ignored and skipped sections. The ImageResizer page starts with a launch card that explains the Explorer command and a `Select images…` button, then groups defaults into resize options (noting that the resize window saves its changes here), file and encoding sections.
- PowerStore page `Update all` follows the current list filters for both the count badge and the queued packages; the tray action still updates everything.
- PowerStore filter controls keep fixed widths (and the source button always reads “Sources (n)”) so picking filters no longer shifts the toolbar.
- PowerStore logos use a layered source: local Uninstall `DisplayIcon`, then WinGet manifest icon URLs, then an optional remote PowerKit icon map (Manage toggle, off by default; repository map starts empty).
- PowerStore keeps the last Installed package list on screen when the page opens again and refreshes managers silently in the background; only the first cold load shows the empty loading state, and a failed silent refresh does not wipe the previous rows.
- PowerStore: the source filter is now a multi-select flyout with one checkbox per source; the sort list offers every key in both directions (↑ / ↓) and the separate direction button is gone; the toolbar `Info` button is removed — click an application name to open its details; installed rows show the app logo from its local Windows icon (first-letter placeholder when none is found).
- Open the settings window centred in the work area of its monitor on first show, shrinking it to fit when the default 980×720 DIP size would run under the taskbar.
- Fix settings shell pages shifting right and clipping past the window edge on wide windows: remove `MaxWidth` from the stretched page `StackPanel` (which triggers WinUI 3's stretched `MaxWidth` arrange offset bug once the pane exceeds 1000 DIP) and keep horizontal scrolling disabled on page/drawer `ScrollViewer`s.
- Add PowerStore phase 3: npm, pip, Cargo, .NET Tool, Windows PowerShell, PowerShell 7 and vcpkg join the unified package table for global/user tools (project-local trees stay out of scope). Missing managers stay hidden; Manage lists every phase-3 manager’s availability.
- Add PowerStore phase 2: Scoop and Chocolatey join the unified package table. Missing managers stay hidden from Discover/filters and show as “not installed” in Manage; Chocolatey below 2.0 is treated as missing. Lists merge across managers (WinGet COM child plus CLI), and install/update/uninstall route by package manager with process-kill cancel for CLI ops.
- Close the last PowerStore phase-1 product gaps: waiting queue items can be removed one-by-one from the live strip (bulk “cancel pending” remains), and a missing App Installer package is distinguished from an installed-but-too-old/incomplete WinGet COM registration on the status card (TR/EN/AR). ARP-only rows stay listed as source `local`.
- Redesign the Power Store page around a unified package table, sticky filter/action toolbar, Discover search dialog and right-side overlay drawers. Installed packages and available updates now share one list with actionable updates pinned at the top, a header select-all checkbox, live text filtering, source / status / scope filters, sorting by name, package ID, install date or last update date (combining Windows Uninstall registry timestamps, install folder metadata and PowerStore history), smart bulk actions (`Update all` / `Update selected` / `Install selected`, `Uninstall`, `Ignore`, `Info`, `Refresh`), and a compact per-row primary action plus `⋯` menu. Searching uninstalled catalog packages opens via a dedicated `ContentDialog` and renders results in the same table; Package Info + Install Options, History and Manage open as right-side overlay drawers instead of separate pages.
- Complete PowerStore phase 1. A periodic update check (first after one minute, then every 6 hours by default; adjustable from 15 minutes to 7 days) feeds a new PowerStore tray submenu with the update count, "Update all", "Check for updates" and "Open PowerStore", and shows a notify-area balloon when more updates appear (can be turned off). Updates are never installed automatically. The Power Store page adds multi-select with bulk install / update / uninstall, manifest details (publisher, author, license, homepage, description), per-package install options (scope, architecture, version, extra installer arguments) reused for later updates, "Skip this version" and "Ignore updates", a History section (200 most recent operations, 4 KiB log each, kept in `power-store-history.txt` beside the settings) and a Manage section with the check interval, notifications, ignored / skipped lists and `winget export`-compatible export and import (import lists packages with checkboxes before queuing installs).
- Show PowerStore install / update / uninstall progress (stage and percentage when WinGet reports one) with a progress bar, and add "Cancel operation" for the running item. Exit cancels a running operation and waits at most two seconds. Updates now resolve the package through the installed catalog, and installs use the row's own source (`winget` or `msstore`), so upgrades and Store packages no longer fail with "package not found".
- Fix PowerStore WinGet COM activation under Velopack by ensuring `Microsoft.Management.Deployment.winmd` sits beside the running executable (copying from `current\` when needed), and surface the real COM error on the status card instead of only the generic unavailable label.
- Keep PowerStore package lists usable when a single installed catalog row fails to hydrate; skip that row instead of failing the whole Installed / Updates refresh.
- PowerStore Updates now trust WinGet's own version comparison (`IsUpdateAvailable`) instead of "installed version differs from catalog version", so self-updating apps that run ahead of the catalog (for example Edge and OneDrive) are no longer offered an older version. Installed rows show no "new" version in that case.
- Run PowerStore WinGet requests in a hidden `powerkit.exe --power-store-worker` child process. The settings shell adds the Windows App Runtime framework to its own package graph, and with that graph present every WinGet COM call failed with "Package Microsoft.WindowsAppRuntime.2_… not found".
- Start PowerStore phase 1: domain model and settings keys for update checks / ignore / skip, vendored WinGet COM metadata, and a Windows Packaged COM adapter that can search and list installed packages. The settings shell Power Store page (Discover / Installed / Updates) loads packages off the UI thread and can enqueue serial install / upgrade / uninstall through WinGet; exit waits for or drains the queue before power cleanup.
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

## 0.15.0

- localize Image Resizer and complete utility page layout

## 0.14.1

- standardize utility and workspace tool pages

## 0.14.0

- polish PowerStore logos, silent refresh, and filter-stable Update all

## 0.13.3

- centre the settings window in the monitor work area on first show

## 0.13.2

- remove MaxWidth on stretched settings pane to stop right-side clipping

## 0.13.1

- constrain settings pane content to the available column width

## 0.13.0

- complete PowerStore phases 1-3 with Scoop, Chocolatey, and developer CLIs

## 0.12.0

- add PowerStore WinGet page and serial install queue

## 0.11.2

- record AlwaysTop review and refresh agent memory

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
