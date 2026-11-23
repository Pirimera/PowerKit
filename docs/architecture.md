# Architecture

PowerKit is a layered Cargo workspace. Windows 11 on ARM64 and x64 with the
WinUI 3 shell is the only supported product; the layers exist so that a second
UI kit or another operating system can be added by implementing a contract
rather than by rewriting the application. No macOS, Linux or second UI
implementation exists.

## Layers

~~~text
crates/
  core/               powerkit-core: domain logic; no OS, UI or network dependency
  app/                powerkit-app: UI-neutral application layer
    ports             traits for operating system services
    command, intent   typed requests from the UI, tray, shortcuts and command line
    event             what the platform reports
    controller/       per-tool state machines and workflows, and the application controller
    view/             the snapshot, pages and tray menu a UI renders
    ui                the contract a UI kit implements
    testkit           fake ports and a settable clock, behind the `testkit` feature
  platform/windows/   powerkit-platform-windows: the ports in Win32 and WinRT
  ui/reactor/         powerkit-ui-reactor: the WinUI 3 settings UI
apps/
  powerkit/           the Windows application and composition root; its version is the product version
  winget-worker/      PowerStore's WinGet COM child process, without any UI dependency
  shell-ext/          the Explorer command DLL
~~~

The root `Cargo.toml` only declares the workspace. The scripts crate stays
outside it with its own lockfile. There is no plugin loader, background
service or web runtime. A crate for another platform or UI kit
(`platform/macos`, `platform/linux`, `ui/gpui`) is created only when its first
implemented responsibility exists.

Rules:

- Dependencies point one way: `core`, then `app`, then the platform and UI
  crates, which do not see each other, then the binary, the only place that
  names both. `powerkit-core` declares no dependency and does no file access;
  `powerkit-app` depends only on `powerkit-core`. The repository check rejects
  any other dependency or a build script in either, and limits the workspace
  crates the UI crate, the platform crate, the Explorer DLL and the WinGet
  worker may name.
- `core` and `app` forbid `unsafe`. FFI lives in the platform crate, the
  Win32 hosting of the UI crate and the Explorer DLL, and each `unsafe` block
  explains ownership, pointer validity and thread affinity.
- The UI, the tray, shortcuts and the command line produce a typed command or
  intent that carries its own value. Platform code and workers produce events.
  `powerkit_app::controller::application::App` on the main thread handles
  both: it owns the preferences, decides what every request means and returns
  the step only the shell can carry out. It never calls the platform itself.
- A port describes a capability (keep the system awake, pin a window), not a
  Win32 call. A platform reports the tools it cannot offer
  (`Snapshot::unavailable`) and the UI leaves them out.
- Data that crosses a layer is neutral: no virtual-key codes, `FILETIME`,
  `COLORREF` or window handles above the platform crate.
  `powerkit_core::primitives` holds the neutral forms. `KeyCode` uses the
  Windows virtual-key numbering as PowerKit's own table (as Chromium's
  `KeyboardCode` does), and `UtcTicks` counts 100 ns since 1601 UTC; each
  platform crate translates them to its native form.
- Stored files stay readable: the settings file keeps `version=1`, and a
  format change adds keys and never repurposes one.
- Where a type can enforce an invariant (a power request that cannot change
  thread, a controller with no path that persists a session), prefer the type
  to a comment.
- The fakes live in `powerkit_app::testkit` behind a feature, not in a crate
  of their own: the controllers' unit tests need them, and a separate crate
  would depend back on `powerkit-app`. The `fixtures` feature of
  `powerkit-core` exposes synthetic test data to other crates' tests. Neither
  is enabled in a product build.

What a second platform or UI kit meets first:

- The composition root keeps the order of startup and the order of two effects
  that must fail together (the display request is made before the preference
  is stored and undone when the store fails). Both call
  platform tools directly and decide nothing another platform could reuse.
- Timers (`WM_TIMER`), the ruler's display-change message and the WinUI host's
  own messages are window messages; `Event` covers the background work and
  the operating system notifications.
- A workspace's view model (PowerStore, MediaRenamer) sits beside its session
  under `controller/`, because it is built from state only the session sees.
- ScreenRuler's overlays, AlwaysTop's border and window tracking, the mouse
  hook and input injection, HID discovery, WIC and the Property System, the
  WinGet worker and the Explorer integration are Windows code that another
  platform has to write for itself.
- Shortcut labels name the meta key through the platform
  (`Snapshot::meta_key`).
- Not verified: `powerkit-core` and `powerkit-app` type-check and pass Clippy
  for `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin`, but their tests
  have never run off Windows.

## Boundaries

| Location | Responsibility |
| --- | --- |
| apps/powerkit/src/main.rs | Process entry point: runs the shell and reports a startup failure |
| crates/core/src/always_on.rs | Pure session, interval and expiry deadline transitions |
| crates/core/src/always_top.rs | Pure pin session (none, one window or a count) and opacity/frame bounds |
| crates/core/src/primitives/ | Neutral value types that cross layers: `KeyCode` (logical key numbering), `UtcTicks` (UTC instant) and `Rgb` |
| crates/core/src/hotkey.rs | Shared keyboard chords over `KeyCode`: storage form, labels, defaults and the Win+Alt fallback |
| crates/platform/windows/src/hotkeys.rs | `RegisterHotKey` port for the shortcut registry, kept on the app window's thread; maps `WM_HOTKEY` ids to slots |
| crates/platform/windows/src/keys.rs | `KeyCode` and chords to virtual-key codes and `RegisterHotKey` modifiers |
| crates/platform/windows/src/color.rs | `Rgb` to and from `COLORREF` at the drawing edge |
| apps/powerkit/src/shell/mod.rs | The composition root: UI thread, hidden app window, tray icon, intent and message dispatch, lifecycle; holds the `App` and carries out its steps (store preferences, dialogs, clipboard). The only place that names both the platform crate and the UI crate |
| crates/platform/windows/src/lib.rs | Platform crate root: the `Shell` callbacks the ports use to reach the application shell (view refresh, tray notification, tool glyph), the UI language, error dialogs |
| apps/winget-worker/ | `powerkit-winget-worker.exe`: runs one WinGet COM request and prints the answer; links no UI crate |
| crates/platform/windows/src/command.rs | `WM_COMMAND` ids for `powerkit_app::command::Command` (named `AppCommand` on Windows) and the `MenuId` adapter used by the tray menu and worker threads |
| crates/platform/windows/src/events.rs | Delivers `Event` values from worker threads to the app window through one posted message |
| crates/platform/windows/src/instance.rs | Hands a launch's `Request` to the running instance as one tagged `WM_COPYDATA`; reads the shell DLL's UTF-16 path lists |
| crates/platform/windows/src/intents.rs | Queue that carries `Intent` values from the settings shell to the app window on the UI thread |
| crates/ui/reactor/src/window/ | WinUI 3 settings shell (windows-reactor): renders `powerkit_app::view::Snapshot` and sends intents through `powerkit_app::ui::Host` |
| crates/ui/reactor/src/window/pages.rs | Per-tool settings pages composed from shared surfaces |
| crates/ui/reactor/src/window/store_page.rs | Power Store workspace: sticky toolbar, unified package table and Discover dialog |
| crates/ui/reactor/src/window/view_helpers.rs | Fluent helpers shared by the Power Store and MediaRenamer pages: icon labels, button rows, sized icons and the virtualized item width filler |
| crates/ui/reactor/src/window/store_drawer.rs | Power Store right-side overlay drawers (package inspector, history, Settings) |
| crates/ui/reactor/src/settings_ui.rs | Shared Fluent card/stack/stepper/hotkey/color surfaces for shell and dialogs |
| crates/ui/reactor/src/winui_host.rs | Claims WinUI HWNDs by explicit `WindowRole`; hide-on-close, start hidden, exit and key capture |
| crates/platform/windows/src/icons.rs | Branded window and notify-area icons from brand/windows ICOs |
| crates/platform/windows/src/theme.rs | Taskbar popup theme plus app light/dark colors for Win32 overlays |
| crates/platform/windows/src/startup.rs | Start with Windows: HKCU Run key (`--background`) unpackaged, manifest `StartupTask` on the UI thread when packaged; both keep the shell in the tray |
| crates/platform/windows/src/recovery.rs | Restart after a crash or hang (`RegisterApplicationRestart`) and the `--restarted` wait for the crashed instance |
| crates/platform/windows/src/package.rs | Detects MSIX package identity; packaged builds skip runtime shell/startup registration |
| crates/core/src/locale.rs | Turkish/English/Arabic string catalogs; the platform passes in the system language |
| crates/platform/windows/src/language.rs | Windows UI language mapped to a shipped catalog |
| crates/platform/windows/src/menu.rs | Native tray popup built from the `powerkit_app::view::tray` entries; maps commands to `WM_COMMAND` ids and icons to Segoe Fluent glyphs |
| crates/platform/windows/src/menu_icon.rs | Segoe Fluent glyphs rendered to 32-bit bitmaps for tray menu items |
| crates/core/src/tray_layout.rs | Pure tray menu layout: tool order, per-tool visibility and the `tray_tools` settings line |
| crates/platform/windows/src/power.rs | Execution-state flags, named power request, battery status and RAII cleanup |
| crates/app/src/ports.rs | Port traits a platform implements: clocks, keep-awake request, power status, watched processes, wake-up timer, settings store, start with the system, shortcut registration, pinnable windows, image codec, media files and their metadata reader, package catalog, HID++ channel |
| crates/app/src/controller/always_on.rs | AlwaysOn controller: session, deadline, end notice, battery policy and bound process over those ports |
| crates/app/src/controller/settings.rs | Typed preference edits (`Edit`), the Home tool switches (`Tool`) and `commit`: startup registration, then the settings write, undone together on failure |
| crates/app/src/controller/always_top.rs | AlwaysTop controller: the remembered target, pin or unpin, opacity steps, cues, over a window port with opaque window ids |
| crates/app/src/event.rs | `Event`: what the platform reports, from its background work (device list, rename progress, resize progress) and from the operating system (sleep and resume, power source, clock, session lock, hardware, theme) |
| crates/app/src/fault.rs | Typed user-facing failures carried in `io::Error` and worded in the UI language only when shown |
| crates/app/src/controller/image_resizer.rs | Image Resizer preview and write jobs over an image codec port: plan lines, progress, cancel, the rule that nothing is written without a complete preview |
| crates/app/src/controller/media_renamer/ | MediaRenamer workspace: listed files, preview rows, filters, details drawer, history and the worker coordination, over the media file ports; `commit.rs` holds the order of an apply and an undo (revalidate, journal first, group rollback); `view.rs` holds the render-ready views |
| crates/app/src/controller/power_mouse/ | The Logitech HID++ conversation above the transport: identity, battery and control queries (`logitech.rs`), device settings and actions (`device_settings.rs`) and the connected device's diversions, events and status (`link.rs`), over the HID++ channel port |
| crates/app/src/controller/power_store/ | PowerStore session: Installed and Discover lists, serial operation queue, update checks, drawers, history, export/import and the logo loader, over the package catalog port |
| crates/app/src/controller/hotkeys.rs | Global shortcut slots and their registry: registration order, Win+Alt fallback, recording conflicts and the notes shown under shortcut cards |
| crates/app/src/view/tray.rs | Tray menu as data: entries, order, labels, marks and typed commands for every tool, the shortcuts submenu, Open and Exit |
| crates/app/src/controller/application.rs | `App`: owns the preferences, the PowerStore and MediaRenamer sessions and the key-recording state; decides what every intent, page action, recorded key or incoming selection means and returns the `Step` only the shell can carry out (store, ask, have a tool act); builds the snapshot from the platform's `Readings` |
| crates/app/src/command.rs | Command vocabulary shared by the settings UI, tray and shortcuts: each command's tool, the preference edit or AlwaysOn session it stands for, and the exclusive groups |
| crates/app/src/instance.rs | `Request`: what a launch asks for (open, AlwaysOn, resize files, rename paths) and the one command-line parser |
| crates/app/src/intent.rs | `Intent`: a UI request with its value (colors, interval, expiry, filename, page actions, tray edits) or a plain command, and the tool that gates it |
| crates/app/src/view/mod.rs | What a UI renders: `Snapshot` (everything a settings UI shows), `Page` and `CaptureTarget` |
| crates/app/src/ui.rs | The UI contract: `Host`, the UI's only way to the platform, and its handle |
| crates/ui/reactor/ | `powerkit-ui-reactor`: the WinUI 3 settings UI. Sees the application only through `powerkit_app::view`, `ui`, `intent` and `command`; the repository check rejects a dependency on another workspace crate |
| crates/app/src/testkit.rs | Fake ports for controller tests (`testkit` feature) |
| crates/platform/windows/src/awake.rs | AlwaysOn Windows ports (power request, window timer, process handle, clocks); carries out the controller's refresh and notices |
| crates/platform/windows/src/clock.rs | System and file times as `UtcTicks`; local date/time input for AlwaysOn expiry, end-time labels |
| crates/platform/windows/src/top/ | PowerToys-aligned pin, border z-order and last eligible window (module map in always-top.md) |
| crates/platform/windows/src/frame.rs | PowerToys FrameDrawer D2D ring for the AlwaysTop border |
| crates/core/src/screen_ruler.rs | Pure Screen Ruler modes, edge detection, measurement text and per-monitor `Marks` (press, drag, release, right click, copied spans); the overlay only carries out their decisions |
| crates/platform/windows/src/clipboard.rs | Plain-text clipboard writes (ScreenRuler measurements, PowerStore ids) with a short retry while another app holds the clipboard |
| crates/platform/windows/src/ruler/ | ScreenRuler on the UI thread: `mod.rs` session lifecycle (a session keeps the current `Config` plus its own tool and tolerance), hotkey, tool choice and copied text; `capture.rs` GDI surfaces and screen capture; `paint.rs` layered overlays; `input.rs` overlay mouse and keys; `toolbar.rs` toolbar window, placement and window procedure; `bar.rs` toolbar drawing, hit test and tooltips |
| crates/core/src/image_resizer.rs | Pure Image Resizer sizes, names and batch plan |
| crates/core/src/power_store.rs | Pure PowerStore package model, update filters, queue and export extras |
| crates/core/src/json.rs | Bounded-depth JSON reader and string encoder shared by PowerStore parsers, export/import and the icon database |
| crates/platform/windows/src/power_store/ | The package catalog port on Windows (`port.rs`): the WinGet worker child and Packaged COM adapter as the native package service (`catalog.rs`), the process runner behind the `Programs` port (`cli_process.rs`: PATH lookup, Job Object, bounded waits), logo cache and export/import dialogs |
| crates/app/src/controller/power_store/managers/ | The command-line package managers (Scoop, Chocolatey, npm, pip, Cargo, .NET Tool, PowerShell, vcpkg) over the `Programs` port, program lookup from the manager settings, version probing, and the fan-out that asks every enabled manager at once and merges the answers with the platform's native package service |
| crates/core/src/media_renamer.rs, crates/core/src/media_renamer/ | Pure MediaRenamer formats, capture times, templates, grouping, collision planning, BMFF reader and rename history |
| crates/core/src/power_mouse.rs | Pure PowerMouse triggers, actions, bindings and press state machine |
| crates/core/src/power_mouse/device.rs | Pure model of devices a PowerMouse vendor adapter can drive: HID collections, grouping, HID++ classification and verified models |
| crates/core/src/power_mouse/diagram.rs | Pure interactive mouse diagram geometry: OpenLogi device artwork metadata, PNG IHDR dimensions, physical button anchor projection and 45° leader-line callout layout |
| crates/platform/windows/src/power_mouse/ | WH_MOUSE_LL hook thread, SendInput injector, elevation pass-through, HID discovery, the HID++ channel port (`hidpp_channel.rs`), the per-device adapter thread, off-UI OpenLogi device render cache (`assets.rs`), Win32 Actions Ring overlay, UI-thread Session and hand-run hardware probes (`probes.rs`) |
| crates/core/src/locale/power_mouse.rs | PowerMouse page and action picker strings (TR/EN/AR) |
| crates/core/src/media_extensions.rs | MediaRenamer extension lists shared by the app and the shell DLL |
| crates/platform/windows/src/media_renamer/ | The media file ports on Windows: folder scan, WIC/Property System probe, the rename and history-file primitives (`commit.rs`); plus the pickers and the Explorer handoff |
| third_party/winget/ | Vendored `Microsoft.Management.Deployment.winmd` for WinGet marshaling |
| crates/platform/windows/src/resize.rs | Image Resizer Explorer verb, second-instance handoff, file picker and the worker thread that runs the `powerkit_app` job (the dialog is WinUI only) |
| crates/ui/reactor/src/resize_dialog.rs | WinUI 3 Image Resizer dialog (windows-reactor), opened beside the settings shell |
| crates/platform/windows/src/shell_package.rs | Per-user sparse package for the Windows 11 menu (unpackaged builds only) |
| apps/shell-ext/ | `powerkit-shell-ext`: the Explorer command DLL (`powerkit_shell.dll`) for the compact menu and classic multi-select. Depends on `powerkit-core` only; its class ids are the text files in `packaging/msix` |
| crates/platform/windows/src/resize_wic.rs | WIC decode, scale, crop and encode on a worker thread; implements the image codec port |
| crates/platform/windows/src/resize_io.rs | Temp-file commit, replace backup and recycle |
| apps/powerkit/src/shell/smoke.rs | Isolated native message-loop checks |
| crates/core/src/settings.rs | Preferences and the bounded settings text codec (`decode`/`encode`); no file access |
| crates/platform/windows/src/settings_file.rs | Size-bounded settings read and atomic replacement (`MoveFileExW`) |
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
- FFI belongs in the Windows adapter (`crates/platform/windows/src`, including its atomic settings writer) and the Explorer DLL; `powerkit-core` has none.
  Explain pointer validity, ownership and thread affinity around unsafe operations.
- Network/download work stays off the UI thread. PowerKit has no in-app
  updater; Store/winget updates must not interrupt an active session.
- Stable package identity is PiriMedya.PowerKit (the Partner Center
  identity). Product distribution is Microsoft Store plus winget pointing at the
  Store package; do not ship GitHub Releases or Setup.exe feeds as an install
  channel. No Store, winget, signing or GitHub credentials ship in the app.
- Settings are outside the installation directory. Preserve malformed settings until
  an explicit user change replaces them. A file without the `version=1` header is
  rejected as a whole; inside it, a known key with an invalid or repeated value is
  skipped (the user sees a partial-read notice) and the other lines still apply,
  and a key this build does not know is kept verbatim and written back on save,
  so running one build does not erase another build's settings.
  Failures raised below the UI that the user sees travel as a typed `Fault` in
  `io::Error` (`crates/app/src/fault.rs`) and are worded in the current
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
  UI strings live in `crates/core/src/locale.rs`. Do not restyle WinUI controls with brand
  colors or Inter Tight. Do not add empty submenus for planned tools.
- The shell and tray share `AppCommand`. The shell sends typed intents, which
  carry their own value, and the tray posts `WM_COMMAND` ids; both reach one
  entry point in a hidden Win32 app window. Preference writes, power
  requests and hotkeys stay there. After the settings shell starts, the reactor
  loop replaces the Win32 message loop on the same UI thread; a tray/logon start
  may run a plain Win32 pump first until settings open.
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
- A tool that edits the user's files requires preview, deterministic collision
  handling and recoverable changes before touching original files.

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
`%LOCALAPPDATA%\PowerKit`. Packaged builds toggle that startup task on the UI
thread after the settings shell is created (`RequestEnableAsync` from an MTA
helper crashes in `combase.dll`). A logon start (StartupTask activation or the
unpackaged Run key's `--background`) and a crash restart keep the settings
shell hidden while the tray icon is shown; a Start menu or tray open still
presents it. Those tray starts also defer WinUI entirely until the first
settings or Image Resizer open (a plain Win32 pump runs meanwhile) and defer
PowerStore manager PATH probes until the store is opened, so logon only pays
for the tray, hotkeys and PowerMouse. Prefer WinUI 3 for settings, dialogs and
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

The `powerkit-app` AlwaysOn controller owns the running session; `awake.rs`
keeps it on the UI thread and supplies its Windows ports. One timer fires when the
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
domain module; a second instance forwards them, like every other launch
request, as a `powerkit_app::instance::Request` in one tagged `WM_COPYDATA`.

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
are skipped. The pin shortcut pressed over a PowerKit window is ignored without
an error; over any other window that cannot be pinned it reports why. Pins and
opacity are not persisted.
The parity matrix, open checks, the `top/` module map and the optional
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
