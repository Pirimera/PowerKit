# Changelog

Notable user-visible changes are recorded here. Release dates are added only after
publication; a source version is not proof of a published release.

## Unreleased

- Settings gains a "Tray menu" list: switch each tool's tray entry on or off and move it up or down to reorder the menu. The layout is saved in `settings.txt` (`tray_tools`). A tool switched off on Home still never appears.
- Tray menu entries show each tool's icon, following the taskbar light/dark menu colors. ScreenRuler and ImageResizer are now single items at the top level instead of submenus with one command; the ScreenRuler item shows a check mark while measuring and no longer lists its shortcut, so the menu stays narrow. Menu rows are spaced a little further apart. "Open PowerKit window" is now "Open PowerKit".
- Polish the MediaRenamer workspace and align its History, Settings and Details drawers with PowerStore: the page header gains a live status pill and grouped command bar; progress, semantic outcome/stale/error banners and scan notes sit in distinct strips; the template card shows a live `current → new` example (or a sample before files are added) above a two-column toggle row; a two-row toolbar pairs a clearable filter box and counted state dropdown with one-click status filter pills (`All`, `Ready`, `Problems`) and `Rename` / `Clear` actions; preview rows add 32 px media-kind icon tiles, companion-count pills, parent folder captions, left selection stripes, inline probing spinners and a `Clear filter` empty state; and all three drawers now share PowerStore's 460 px dimmed overlay shell, icon-badged headers, sticky History summary + Clear bar, left-accented batch cards, dirty-aware template Save button and hairline-grouped Windows 11 settings and file rows.
- MediaRenamer accepts `.hif` files, the HEIF extension that Canon, Sony and Fujifilm cameras write, instead of listing them as unsupported. Decoding still needs the HEIF and HEVC Video Extensions from the Microsoft Store, and some 10-bit camera `.HIF` files may stay unreadable even with both installed.
- MediaRenamer's Details drawer now lists every metadata value of each file (EXIF, GPS, Interop, thumbnail, XMP and format blocks for images; Windows Property System values for videos), grouped by section with selectable text. The values are read in the background only while the drawer is open, and online-only files are not read.
- MediaRenamer's stale-preview notice gets a Refresh (Yenile) button: it checks the listed files again, drops the ones that were moved or deleted, and plans the preview again without renaming anything.
- MediaRenamer's preview no longer stops at 500 rows; every visible file is listed, and the table builds only the rows on screen.
- MediaRenamer scans large selections faster: 8,000 explicitly chosen files took about 59 s before and about 6 s now on the development machine (16,000 files about 11 s), because each batch of results replans once and folder listings are indexed.
- MediaRenamer renames files in folders whose paths are longer than 260 characters, without the application's long-path opt-in.
- MediaRenamer's History is limited to 64 MB: the oldest batches are dropped to make room, and an operation whose record alone exceeds the limit is refused before any file is renamed.
- If MediaRenamer's history file cannot be read (too large, or not a PowerKit history file), renaming, undo and clearing stay off with a notice, so the file is never overwritten.
- Polish the PowerStore workspace and right-side drawers: a responsive two-row toolbar gives the local filter generous width alongside an icon-labelled multi-select source menu, scope and sort dropdowns, one-click status filter pills (`All`, `Updates available`, `Up to date`, `Ignored / Skipped`) and a contextual selection action bar; package rows gain 32 px framed logo/monogram tiles, single-line ellipsis names with compact status badges, stacked two-line version diffs (`old` over accent `→ new`), subtle left selection stripes, inline row progress rings and progress bars, calmer subtle `Uninstall` buttons so `Update` actions stand out, and semantic error/confirmation banners; drawers (History, Settings, per-manager and Package Inspector/Options) widen to 460 px with a dimming scrim, rounded leading corners, icon-badged headers, Windows 11-style hairline-grouped settings cards with framed manager tiles, segmented pill pickers for scope and architecture, dirty-aware Save buttons, sticky History outcome summary + Clear bar, semantic left-accented history cards with right-aligned timestamps and selectable logs, and integrated per-manager detection hero cards.
- Fix PowerStore icon-only buttons clipping their glyph: the back arrow in the manager settings drawer lost its tip, and the row `⋯` menu button and other small icon buttons were cut at the edges. Their width now follows the glyph.
- PowerStore remembers the last installed list between runs: opening the page after a restart shows that list right away with a "showing the last saved list" notice and a progress bar while the package managers are asked again in the background. Rows change only when the new list differs, and a failed refresh keeps the previous rows and says so.
- PowerStore asks all package managers at the same time for the installed list, update checks and Discover search, so one slow manager no longer delays the rest one after another.
- PowerStore's package options editor shows only the options that a package's manager honors. Managers that honor none (npm, Cargo, .NET Tool, Windows PowerShell, PowerShell 7 and vcpkg) no longer show an options section whose values were silently ignored.
- PowerStore no longer checks the installed package managers on the UI thread when PowerKit starts; the check runs in the background and respects switched-off managers, so a disabled Chocolatey is not even asked for its version.
- PowerStore recognizes PowerKit's own MSIX listing (`MSIX\PiriMedya.PowerKit_…`), so it is never offered for update or uninstall from PowerStore.
- Fix PowerStore queueing the same update twice when "Update all" is used on the page and from the tray; each package and action now waits once.
- Fix PowerStore's "last checked" time changing after a failed update check; it now changes only after a check succeeds.
- PowerStore's package table builds only the rows on screen (virtualized list), so long installed lists and Discover results create far fewer controls at once (not timed yet). Row contents, group headings, the truncation notice and the 500-row cap are unchanged.
- Fix PowerStore's installed list being dropped or stuck on "Loading" when a Discover search started while it was still loading: each list (Installed and Discover) now has its own worker, so starting one never cancels the other.
- PowerStore's "no package manager", "no manager answered the search" and "background task stopped" messages are translated into Turkish, English and Arabic instead of appearing in English.
- PowerStore refuses PowerShell module names with characters other than letters, digits, `.`, `-` and `_`, so an imported or typed name can no longer change the command that PowerShell runs. Such a Discover search shows no results.
- The PowerStore icon database cache is replaced atomically, so an interrupted download cannot leave a truncated file that suppresses logos for a day.
- Fix AlwaysTop leaving its colored border on screen: after a pinned window closed (for up to a second), while it was hidden to the notification area, and when it was on another virtual desktop. Closing a tab or panel inside a pinned window no longer risks unpinning it.
- Fix PowerKit crashing on exit after AlwaysTop had drawn a border.
- When AlwaysTop cannot restore one pinned window on exit or when the tool is turned off, it still restores the others.
- Fix AlwaysTop transparency steps drifting (90%, 79%, 68% …): every step now lands on an exact 10% value between 20% and 100%.
- AlwaysTop no longer pins or remembers PowerKit's own settings window; the pin shortcut and the page's pin button act on another app's window.
- AlwaysTop uses less CPU while windows are pinned: the border is reused when a pinned window is minimized and restored, it repaints only when its size, color, opacity or thickness changes, and it reads the accent color once instead of on every repaint.
- AlwaysTop's status text follows the app language: English and Arabic show "Untitled window" or the pinned-window count instead of Turkish text, and the frame opacity reads "50%" in English and Arabic (Turkish keeps "%50").
- AlwaysTop removes its pin marker from a window that was already always-on-top before pinning, and from a window whose pin failed. A pin or transparency shortcut set to another slot's default chord no longer moves to a Win+Alt fallback.
- PowerStore's copy-id button no longer says "Copied" when the clipboard could not be written. ScreenRuler and PowerStore now retry for a moment when another app is holding the clipboard, instead of giving up at once.
- ScreenRuler no longer keeps measuring the old screen layout after a monitor is connected, disconnected or changes resolution: the ruler reopens with the same tool for the new layout.
- ScreenRuler now closes and shows an error when choosing a tool fails to create the measurement overlay, instead of silently leaving the toolbar without a working tool; a failure while opening no longer leaves a hidden toolbar behind. The tray status also updates when the ruler closes itself (close button, a measuring click or Escape).
- The open ScreenRuler toolbar follows a Windows light/dark, high contrast or accent color change instead of keeping the old colors until it is reopened.
- Fix ScreenRuler copying wrong inch, centimetre, millimetre and DIP values when Escape copies Bounds measurements: the copy assumed 96 DPI and no physical screen size. Every monitor's measurements now convert with that monitor's own scale and size, including marks kept on several monitors.
- ScreenRuler draws faster: moving the pointer repaints only the monitor it is on (and the one it just left) instead of every monitor, continuous capture recaptures only the monitor under the pointer and only for the spacing tools, and capture surfaces and the label font are reused instead of being recreated every frame.
- Fix ScreenRuler showing an old picture of the screen when a tool is chosen: the frozen image was taken when the toolbar opened, so a window that was in front then (for example PowerKit itself when the ruler was started from its settings page) came back on top of the app you had switched to. The screen is now captured when you choose the tool.
- The ScreenRuler toolbar can be moved with the dotted grip at its left edge. It stays fully on screen, keeps its size and tooltips when dragged to a monitor with another scale, and reopens where you left it until PowerKit exits. Choosing a tool after opening the ruler with no tool selected no longer hides the toolbar behind the measurement overlay.
- Fix PowerStore hanging on a package manager that writes a lot to its error output: PowerKit read the CLI's normal output to the end before the error output, so a full error pipe could stall the manager forever. Both are now read at once.
- PowerStore list, search, details and probe calls (including WinGet) now give up after 5 minutes and show an error instead of loading forever. Cancelling or timing out an operation now also ends the processes the package manager started (for example the tool behind a `.cmd` shim), not only the shim.
- One invalid line in `settings.txt` no longer resets every preference to its default: PowerKit skips that line, keeps the rest and shows a notice. Settings written by a newer PowerKit that this build does not know are kept and written back instead of being dropped.
- Error messages from Image Resizer, ScreenRuler, AlwaysTop, the tray and second-instance launches now follow the app language instead of always appearing in Turkish, and the Image Resizer TIFF compression choice shows "Default"/"None" in the app language.
- Turning on Start with Windows no longer leaves the startup entry changed when the setting cannot be saved.
- The Home page animation no longer redraws the settings window every second while it is hidden or minimized.
- AlwaysTop, ScreenRuler and AlwaysOn settings now show the shortcut that is actually registered, including the Win+Alt fallback used when a default Win+Ctrl shortcut is taken, and the tray no longer shows a shortcut that Windows refused to register.
- AlwaysTop's tray "Increase/Decrease transparency" items now change the window you were using before opening the menu when it is pinned (the same window the tray pin item acts on), instead of always the most recently pinned window.
- Remove the Win32 Image Resizer dialog. It only opened when the Windows App Runtime was missing; Image Resizer is now WinUI only, and without the runtime an Explorer resize shows the runtime requirement and resizes nothing. Store and test MSIX installs bring the runtime, so they are unaffected.
- PowerStore reads package-manager JSON with one shared parser instead of five hand-written ones. Package details now decode `\u` escapes, read only top-level fields (a nested `"version"` can no longer be mistaken for the package's), and show npm authors and licenses written as objects, Scoop licenses written as `{ "identifier": … }` and Windows PowerShell module home pages.
- PowerStore logos now follow UniGetUI's method: packages from Scoop, Chocolatey, npm, pip, Cargo and WinGet get their logo from UniGetUI's public icon database (looked up with UniGetUI's per-manager id rules), downloaded in parallel in the background, scaled to 96 px and kept in `%LOCALAPPDATA%\PowerKit\icons` so later starts show them at once. Lists no longer wait for logo downloads; logos fill in as they arrive. The "Remote package logos" setting is now on by default for new settings and uses the UniGetUI database instead of the empty PowerKit map.
- Fix PowerStore not showing Scoop updates or Discover results: Scoop keeps bucket manifests in a `bucket` subfolder (`buckets\main\bucket\*.json`), and PowerStore read them from the bucket root, so no installed package was compared with its manifest.
- PowerStore lists every Scoop package under the source `scoop` instead of its bucket name (`main`, `java`); packages from other buckets show their id as `bucket/name`. Saved ignore, skip and option entries from earlier builds still apply.
- Fix PowerStore not listing Scoop packages or updates: Scoop's extensionless `shims\scoop` script was picked before `scoop.cmd` and could not be started, so Scoop's whole list failed silently. PowerStore now looks up CLI programs only as `.exe`, `.cmd` or `.bat`.
- PowerStore's manager drawer now shows the detected program (path and the version it reports; a switched-off manager is located but not started) and, for a manager that is missing or too old, the command or link to install it with a copy button. WinGet gains a default installer architecture and vcpkg a default triplet; they apply only to packages without their own value. The WinGet architecture restricts the installer choice, so a package with no installer for it fails to install.
- PowerStore's package manager list now shows an "Installed" / "Not installed" / "Too old or incomplete" badge under each manager's name instead of the "Available"/"Off" text; the on/off state is the switch alone. A switched-off manager is still looked up so its badge stays accurate, and Discover offers only managers that are both installed and switched on.
- PowerStore shows the UniGetUI icon of every package manager (WinGet, Scoop, Chocolatey, npm, pip, Cargo, .NET Tool, PowerShell, PowerShell 7, vcpkg) in its settings drawers and in each package row's Source column, and the manager switch now sits right next to the `>` button. Most PNGs are MIT-licensed copies from UniGetUI and npm's logo is the npm organization avatar; `brand/package-managers/README.md` records the sources, commit and notices.
- PowerStore settings now let each package manager be switched on or off, with a `>` button beside each switch that opens that manager's own drawer, and expose the few settings that matter most there: CLI managers (Scoop through vcpkg) can run from a program you choose instead of the one found on PATH, and WinGet has a default install scope (automatic, user or machine) that applies when a package has no scope of its own. A manager that is off is not probed and is left out of the list, Discover, update checks and the update count; a chosen program that no longer exists is reported as not installed rather than replaced silently. Extra arguments, other scopes and per-source toggles stay per-package or out of scope on purpose.
- PowerStore install/update/uninstall progress now lives on each package's own row instead of a banner above the filters: the row button shows `Updating %N` (or the install/uninstall stage) while running and `Waiting` while queued, and its `⋯` button turns into cancel or remove-from-queue. The header summary names the running package, the action bar adds `Cancel pending (N)`, and the top banner remains only for failures and notices.
- PowerStore lists installed Microsoft Store-delivered packages (for example Dolby Access OEM, Xbox Identity Provider, Windows App Runtime and Auto Super Resolution) with the source `Microsoft Store` and an `Update in Store` button that opens Microsoft Store's updates page. WinGet's catalog does not report these updates, so they are not counted in the update badge or queued by PowerStore.
- Redesign Home (Ana Sayfa) after PowerToys Welcome: animated tool-icon hero,
  welcome copy, a two-column tool grid with enable toggles (disabled tools leave
  the rail/tray and stop system-wide via `module_*` settings; PowerMouse uses
  `mouse_enabled`), application info with an icon header, embedded latest
  changelog, and no trailing “X hides the window” caption.
- Rename the General rail/page label to Home (Ana Sayfa / الرئيسية); the page
  identity and settings keys stay `general`.
- Start PowerMouse slices 1–4: pure remapper domain (`src/power_mouse.rs`) with
  Middle/Back/Forward/tilt/F13–F24 triggers, single/long-press/gesture bindings,
  action catalog and settings grammar (`mouse_enabled`, `mouse_<trigger>`);
  Windows mouse hook + optional F13–F24 keyboard hook, SendInput injector and
  elevated-foreground pass-through; settings page with enable toggle,
  binding-type choice, per-slot action pickers (including “Record shortcut…”),
  wheel tilt, add/remove F13–F24 rows and reset-to-defaults (TR/EN/AR). Tray
  submenu and physical acceptance remain; see `docs/power-mouse.md`.
- Remove Velopack and the in-app updater: the tray "Updates" item, the General page update button and status, and the Settings update toggles are gone; Store/winget will deliver updates. Development builds run unpackaged without an updater. Existing `settings.txt` files with the retired `auto_update`, `update_notify` and `show_notes` lines still load. The General page shows whether the build runs unpackaged or from the MSIX, and its release-notes link opens the changelog.
- Add MSIX packaging infrastructure: `packaging/msix/AppxManifest.xml` (identity `PiriMedya.PowerKit`, Windows App Runtime 2.4+ dependency, `PowerKitStartup` startup task, Windows 11 Explorer commands for Image Resizer and MediaRenamer through a COM surrogate, settings kept in `%LOCALAPPDATA%\PowerKit`), `scripts/Package.ps1` (MakePri + MakeAppx, optional SignTool) and `scripts/New-TestCertificate.ps1` for local self-signed trials. Packaged builds use the startup task instead of the Run key and skip runtime Explorer registration. The manual workflow now builds unsigned ARM64/x64 MSIX artifacts and no longer publishes GitHub Releases. LLVM clang and the .NET SDK are no longer build prerequisites.
- Add MediaRenamer, a workspace that renames photos and videos in place from their capture time. It reads EXIF through WIC and MP4/MOV/M4V/3GP creation times (Apple local dates first, otherwise UTC converted to local time) for JPEG, HEIF, RAW, PNG, WebP, MP4, MOV, M4V, 3GP and AVI; files come from the Explorer verb (Windows 11 menu and classic menu, files and folders), drag and drop, or the file and folder pickers. Token templates with presets render a live preview; Live Photo pairs and `.xmp`/`.aae` sidecars rename as one group; collisions take the sub-second value and then `_1`, `_2`…; missing dates are excluded unless the modified-date toggle is on, and OneDrive online-only files are never downloaded. Renaming revalidates every file, writes a journal first, never overwrites, rolls back a group whose member fails, and can be cancelled between files. History (50 batches, kept in `media-renamer-history.txt` beside the settings) offers Undo across restarts, including batches interrupted by a crash. Default template, subfolders, modified-date fallback and lowercase extensions are saved from the Settings drawer. MediaRenamer joins the navigation rail after PowerStore, the General page and the tray (ready count, status, Open), in Turkish, English and Arabic.
- Localize the Image Resizer WinUI/Win32 dialog, preview/write status lines, file picker, Explorer verb and shell-menu title in Turkish, English and Arabic (including size preset names used in previews and output file names), and AlwaysTop pin/unpin user errors.
- ImageResizer stays a utility: the page leads with File Explorer instructions and demotes “Select images here…” to a Subtle fallback; General no longer offers a resize quick action.
- Apply the utility/workspace page pattern across the shell: AlwaysOn, AlwaysTop and ScreenRuler pages group settings under section headings with a launch/status card first; General lists every tool with status and Open (plus a quick action where one exists); the navigation rail puts PowerStore after General and before the utilities; the Settings page title reads "PowerKit settings".
- Standardize tool pages as utilities (settings page, work done from tray, hotkey or a task window) or workspaces (work on the page, settings in a drawer); see `docs/ui-structure.md`. The PowerStore `Manage` button and drawer are now `Settings` / "PowerStore settings", grouped into package managers, update checks, appearance, backup, ignored and skipped sections. The ImageResizer page starts with a launch card that explains the Explorer command and a `Select images…` button, then groups defaults into resize options (noting that the resize window saves its changes here), file and encoding sections.
- PowerStore page `Update all` follows the current list filters for both the count badge and the queued packages; the tray action still updates everything.
- PowerStore filter controls keep fixed widths (and the source button always reads “Sources (n)”) so picking filters no longer shifts the toolbar.
- PowerStore logos use a layered source: local Uninstall `DisplayIcon`, then WinGet manifest icon URLs, then the UniGetUI icon database (see the entry above).
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

## 0.23.0

- choose and reorder tray tools in Settings and show tool icons in the menu

## 0.22.0

- list all file metadata and polish the workspace and drawers

## 0.21.1

- polish drawers and size icon-only buttons to their glyphs

## 0.21.0

- show the last saved installed list at startup while managers refresh in the background

## 0.20.10

- make virtualized package rows fill the table width

## 0.20.9

- use the Partner Center identity PiriMedya.PowerKit and publisher

## 0.20.8

- rename WinGetError to ManagerError, the error every manager adapter returns

## 0.20.7

- split session list loading, logo loader and tests into modules

## 0.20.6

- split the package model into focused modules

## 0.20.5

- virtualize the package table and share its row list

## 0.20.4

- keep installed list loading when Discover starts and localize session errors

## 0.20.3

- list and search managers in parallel and show only the install options each honors

## 0.20.2

- keep PowerStore startup probes off the UI thread and guard queue and MSIX edge cases

## 0.20.1

- harden AlwaysTop pin lifecycle, border cost and localization

## 0.20.0

- movable ScreenRuler toolbar, measurement fixes and a testable ruler module

## 0.19.11

- correct PowerMouse status and the Home page test notes

## 0.19.10

- remove the Win32 Image Resizer dialog and its message hook

## 0.19.9

- point AlwaysTop tray opacity at the window the tray pin item targets

## 0.19.8

- show only hotkeys Windows actually registered, sharing one Win+Alt fallback

## 0.19.7

- split AlwaysTop top.rs into top/ modules without behavior changes

## 0.19.6

- parse PowerStore JSON with one bounded parser that reads top-level fields only

## 0.19.5

- gate every command by its tool without a wildcard and rename edit_prefs

## 0.19.4

- undo startup registration when saving fails, skip hidden Home redraws and flush PowerStore history writes

## 0.19.3

- show errors and TIFF compression labels in the app language

## 0.19.2

- keep valid settings when one line is invalid and preserve unknown keys

## 0.19.1

- bound PowerStore CLI and WinGet worker calls and end their process trees

## 0.19.0

- PowerStore manager drawers, Scoop fixes and UniGetUI logo database

## 0.18.0

- add PowerMouse device-independent remapper

## 0.17.0

- retire Velopack and add local MSIX packaging

## 0.16.0

- add MediaRenamer workspace with preview, undo and Explorer entry

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
