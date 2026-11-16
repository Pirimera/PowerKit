# PowerStore design

Status: phase-1 install queue in tree. Domain model, settings keys, vendored
winmd, the WinGet COM adapter and a settings-shell Power Store page (Discover /
Installed / Updates) load package lists on a worker thread and enqueue serial
install / upgrade / uninstall through Packaged COM. Tray submenu, progress
callbacks, cancel-in-flight, periodic update checks and toasts are not wired
yet. A temporary local probe confirmed the WinGet COM path on native ARM64
before the adapter landed; live install/upgrade/uninstall against App Installer
still need a physical smoke pass.

PowerStore is a PowerKit tool that discovers, installs, updates and removes
software through existing Windows package managers. It is inspired by
[UniGetUI](https://github.com/Devolutions/UniGetUI) (MIT), but it is not a port:
PowerKit stays one Rust binary with the existing Win32 app window, WinUI 3
settings shell and settings writer. No code is copied from UniGetUI.

## Scope

In the first release:

- Search packages, show manifest details and install.
- List installed packages and uninstall.
- List available updates; update one, a selection or all.
- Skip a specific version or ignore all updates for a package.
- Per-package install options (scope, architecture, version, extra installer
  arguments), reused for later updates of that package.
- Export and import a package list.
- Operation history with bounded logs.
- Periodic update check with a notify-area count and a toast. Updates are never
  installed automatically.

Not planned for the first release: automatic updates, cloud backup or Gist sync,
shortcut management, package share links, a CLI/IPC automation surface, a TUI,
portable mode, extensions (agent skills, git repositories) and third-party icon
services.

## Package managers and phases

Every manager sits behind one `PackageSource` adapter so the pages, queue and
persistence do not depend on how a manager is reached.

| Phase | Managers | Transport |
| --- | --- | --- |
| 1 | WinGet (`winget` and `msstore` sources) | WinGet COM/WinRT API |
| 2 | Scoop, Chocolatey | CLI with structured output where available |
| 3 | npm, pip, Cargo, .NET Tool, Windows PowerShell, PowerShell 7, vcpkg | CLI |

The WinGet API (`Microsoft.Management.Deployment`) avoids parsing localized
console output and reports typed progress. Other managers have no comparable
API; their adapters must pin the command shapes they rely on, prefer JSON
output, and treat unparseable output as an operation error rather than guessing.

A manager that is not installed is hidden from the pages and shown as "not
installed" in PowerStore settings. PowerStore does not install managers itself.
If App Installer / WinGet is missing or too old for the linked winmd, treat
WinGet as unavailable the same way (no crash, no silent CLI fallback in phase 1).

## WinGet COM integration (verified)

Evidence: unpackaged native `aarch64-pc-windows-msvc` probe against
`Microsoft.DesktopAppInstaller` 1.30.300.0 on Windows 11 ARM64, using
`Microsoft.WindowsPackageManager.ComInterop` 1.29.380.

- `windows::Management::Deployment` in the `windows` crate is the MSIX
  deployment API, not WinGet. WinGet requires bindings generated from
  `Microsoft.Management.Deployment.winmd`.
- Generate bindings at build time with `windows-bindgen` (`--in default` plus
  the winmd, filter `Microsoft.Management.Deployment`). Link `onecoreuap`.
  Depend on `windows-core`, `windows-collections` and `windows-future` at the
  versions that match the pinned `windows` crate.
- Activate with `CoCreateInstance(..., CLSCTX_LOCAL_SERVER)` against the
  documented CLSIDs (for example PackageManager
  `{C53A4F16-787E-42A4-B304-29EFFB4BF597}`). The server is Packaged COM hosted
  by App Installer; the same path works for unpackaged callers.
- Ship `Microsoft.Management.Deployment.winmd` next to `powerkit.exe` (and in
  the MSIX package root beside the executable). Without it, marshaling fails
  with `0x80073D54` (proxy registration not found). The NuGet package marks
  this as out-of-proc / `FactoryLinkage=none`; PowerKit does not need the
  optional in-proc `Microsoft.Management.Deployment.dll` for the agreed model.
- The NuGet package is MIT (`license` expression MIT; upstream
  [winget-cli](https://github.com/microsoft/winget-cli) is MIT). Keep the winmd
  as a vendored build input with attribution; do not embed Store or winget
  secrets.
- Pin the ComInterop / winmd version deliberately and re-verify after WinGet
  upgrades. Prefer Size/GetAt over `for`/`IntoIterator` on projected WinRT
  vectors; the probe's iterator path hit `E_NOINTERFACE` while indexed access
  succeeded.
- Verified operations on this machine: connect to the `winget` catalog, search
  by package id (`Microsoft.WindowsTerminal`), and list installed packages
  through a composite local catalog (`winget` + `msstore`, 154 matches).
  Install, upgrade, uninstall, progress callbacks and elevated
  `WinGetServerManualActivation_CreateInstance` were not exercised.
- x64 was not probed here; the NuGet ships winmd plus optional native factories
  for x64/x86/arm64, so expect the same activation model, then confirm on
  native or emulated x64 before release.

## User interface

PowerStore is a page in the settings shell navigation rail, displayed as
"Power Store", with Discover, Installed and Updates sections. Package rows show
name, identifier, version, available version and source; details come from the
manifest only (publisher, homepage, license, description, installer URL and
size when the manifest provides them). Packages without manifest artwork use a
generic glyph. Multi-select drives bulk install, update and uninstall. Settings
on the page cover the update check interval, enabled managers, ignored packages
and skipped versions. Strings live in `src/locale.rs` in Turkish, English and
Arabic.

The notify-area menu gets a PowerStore submenu only once the tool ships: a
status label with the pending update count, "Update all" and "Open PowerStore".

## Operations

- One serial queue owns all install, update and uninstall operations. Queued
  items can be removed; the running item can be cancelled when the manager
  supports it. Search and update checks may run alongside the queue.
- All manager calls, network access and process waits run on worker threads.
  Results return to the UI thread as messages; the UI thread never blocks on a
  manager. COM objects stay on the worker that created them unless explicitly
  marshaled.
- Each installer raises its own UAC prompt. PowerStore never runs elevated
  itself and does not keep an elevated helper. If a future need appears for
  admin-context activation, evaluate WinGet's manual activation helper then;
  it is out of scope for phase 1.
- PowerStore excludes PowerKit's own package from updates and uninstall; the
  Store owns PowerKit updates.
- A pending or running operation must not stop or restart an AlwaysOn session,
  and exit waits for or cancels the running operation before power cleanup.

## Persistence

Persisted through the existing settings writer: update check interval, enabled
managers, ignored packages, skipped versions and per-package install options,
keyed by manager, source and package identifier. Operation history is a
separate bounded file beside the settings (recent operations with truncated
logs), not in the installation directory. Defaults until tuned: keep the 200
most recent operations and at most 4 KiB of log text per operation. The queue,
search results and update lists are not persisted; a restart starts with an
empty queue.

## Export format

Exports use the WinGet `winget export` JSON shape so `winget import` can read
the WinGet part directly. Packages from other managers and per-package options
are stored under a top-level `PowerKit` object in the same file.

Physical check on this machine: a `winget export` file with an added `PowerKit`
section was accepted by `winget import --ignore-versions --no-upgrade
--disable-interactivity`; WinGet processed its own packages and did not reject
the unknown property. Import in the product still shows the full list and lets
the user deselect items before anything is queued. Re-check after WinGet
updates in case schema validation tightens.

## Module layout

- `src/power_store.rs`: pure package model, filters, ignore/skip rules, queue
  state transitions and export parsing/serialization. Unit-tested without
  Windows.
- `src/platform/windows/power_store/`: WinGet COM adapter plus `session` worker
  that feeds the settings page (search, installed, updates) and runs a serial
  install/update/uninstall queue off the UI thread. Later: progress/cancel,
  toast/tray glue and CLI managers.
- `third_party/winget/Microsoft.Management.Deployment.winmd`: vendored build
  input (ComInterop 1.29.380). `build.rs` generates bindings into `OUT_DIR` and
  copies the winmd next to `powerkit.exe`; packaging copies it into the stage
  directory.
- Settings persist `power_store_check_minutes`, `power_store_ignore` and
  `power_store_skip` through the existing writer. The page will compose
  `settings_ui.rs` surfaces in `window/pages.rs` when the UI lands.
- Do not add a tray submenu until the page can open and show update counts.

## Open questions

- Microsoft Store policy review for an app that installs third-party software.
  UniGetUI is listed in the Store, but PowerKit's submission still has to pass
  its own review.
- Minimum App Installer / WinGet version that matches the pinned winmd, and how
  to detect "too old" versus "missing" for the settings status string.
- Confirm the same COM activation and winmd-beside-exe requirement on native or
  emulated x64, and inside a self-signed MSIX install.
- Install / upgrade / uninstall progress and cancellation through the COM API
  (not yet probed).
- Whether PowerKit should filter ARP-only installed rows in the Installed page
  by default, or show them with a clear non-WinGet source label.

## Next implementation slice

1. Progress and cancel-in-flight for the running WinGet operation; physical
   smoke of install / upgrade / uninstall on ARM64 (and x64).
2. Periodic update checks, tray submenu and toast when the page can report counts.
3. Per-package options and export/import UI.
