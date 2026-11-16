# PowerStore design

Status: phase-3 feature set in tree (WinGet + Scoop + Chocolatey + developer
CLIs: npm, pip, Cargo, .NET Tool, Windows PowerShell, PowerShell 7, vcpkg). The
settings-shell Power Store page (unified installed/updates table, Discover
dialog, History / Manage / Info drawers) loads package lists on a worker thread
and runs a serial install / upgrade / uninstall queue. WinGet still uses the
`--power-store-worker` Packaged COM child; other managers run as CLI children
off the UI thread (cancel kills the process). Opening Power Store (or returning
to it) keeps the last Installed list on screen and refreshes managers in the
background; only a cold first load blanks the table. A failed silent refresh
leaves the previous rows. Missing managers are hidden from Discover / filters
and listed as "not installed" in Manage. Partial list success is allowed. Developer CLIs cover **global/user tools only** (no project
`node_modules` / venv / crate path trees). ARP-only installed rows stay visible
with source `local`. Native ARM64 WinGet worker smoke is recorded below;
emulated x64 list/search smoke is in
[validation/power-store-emulated-x64-smoke.md](validation/power-store-emulated-x64-smoke.md).
Not yet verified inside an MSIX install, nor with interactive `msstore` / UAC
installers.

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
portable mode, extensions (agent skills, git repositories) and a mandatory
third-party icon CDN (an optional PowerKit-owned remote map exists instead).

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
WinGet as unavailable the same way (no crash, no silent CLI fallback). Chocolatey
below 2.0 is treated as missing (the `list`/`outdated` shapes used here require
2.x).

## Scoop and Chocolatey CLI (phase 2)

- **Scoop:** installed apps from `scoop export` JSON; updates by comparing each
  app's bucket manifest `version` to the installed version (no fragile `scoop
  status` table parse); Discover walks local `buckets/*/…json` for name /
  description matches; details read the bucket manifest; operations use
  `scoop install|update|uninstall` with `CREATE_NO_WINDOW`. Source is the bucket
  name (`main`, `extras`, …).
- **Chocolatey:** `choco list|outdated|search --limit-output --no-progress
  --no-color` pipe-delimited rows; operations use
  `choco install|upgrade|uninstall -y --no-progress`; source is the fixed label
  `chocolatey`. Details are best-effort from an exact search row.
- Shared spawn/cancel lives in `cli_process.rs`. Catalog fan-out is
  `catalog.rs`: merge WinGet broker results with Scoop/Chocolatey when present.
- Unit tests cover Scoop export JSON and Chocolatey `-r` row parsers without a
  live Scoop/Chocolatey install. Physical on this ARM64 machine (2026-11-16):
  Scoop is present (`scoop export` returned an `apps` array); Chocolatey is not
  on PATH (expected Manage “not installed” / hidden Discover source).

## Developer CLI managers (phase 3)

Global/user-scope only. Shared spawn/cancel remains `cli_process.rs`;
`catalog.rs` fans out beside Scoop/Chocolatey.

| Manager | List | Updates | Discover | Ops |
| --- | --- | --- | --- | --- |
| npm | `npm list -g --depth=0 --json` | `npm outdated -g --json` | `npm search --json` | `npm i\|update\|un -g` |
| pip | `python\|py -m pip list --format=json` | `list --outdated --format=json` | exact `pip index versions <id>` | `install` / `install -U` / `uninstall -y` |
| Cargo | `cargo install --list` | no auto-outdated; Update = `--force` | `cargo search` | `install` / `install --force` / `uninstall` |
| .NET Tool | `dotnet tool list -g` | Update = `dotnet tool update -g` | `dotnet tool search` | `tool install\|update\|uninstall -g` |
| Windows PS / PS 7 | `Get-InstalledModule \| ConvertTo-Json` | no gallery poll on list | `Find-Module` JSON | `Install\|Update\|Uninstall-Module` |
| vcpkg | `vcpkg list` | Update = reinstall | `vcpkg search` | `install` / `remove` |

Parser fixtures for these shapes live beside the Scoop/Chocolatey unit tests in
`src/power_store.rs`. Physical on this ARM64 machine (2026-11-16): npm, pip,
python/py, cargo, dotnet, powershell and pwsh are on PATH; vcpkg and Chocolatey
are not (expected Manage “not installed”).

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
- Ship `Microsoft.Management.Deployment.winmd` next to the running process
  image (and in the MSIX package root beside the executable). Without it,
  marshaling fails with `0x80073D54` (proxy registration not found). Under
  Velopack the binary lives in `current\`; if the process image is the parent
  stub or `Update.exe`, PowerStore copies the winmd beside that image before
  `CoCreateInstance`. The NuGet package marks this as out-of-proc /
  `FactoryLinkage=none`; PowerKit does not need the optional in-proc
  `Microsoft.Management.Deployment.dll` for the agreed model.
- The NuGet package is MIT (`license` expression MIT; upstream
  [winget-cli](https://github.com/microsoft/winget-cli) is MIT). Keep the winmd
  as a vendored build input with attribution; do not embed Store or winget
  secrets.
- Never call WinGet COM from the UI process. windows-reactor adds the Windows
  App Runtime framework to the process package graph (`AddPackageDependency`);
  with that graph present every WinGet call fails with
  `Package Microsoft.WindowsAppRuntime.2_<version>_<arch>__8wekyb3d8bbwe not found`
  (reproduced on native ARM64 with App Installer 1.30.300). PowerStore spawns
  `powerkit.exe --power-store-worker <verb> [value]` with `CREATE_NO_WINDOW`
  from a worker thread; the child never bootstraps WinUI, does not inherit the
  graph, and returns tab-separated `P` package lines followed by `OK` or
  `ERR\t<unavailable|operation|cancelled>\t<message>` on stdout (`broker.rs`).
  `details <id> <source>` returns one `D` line with ten manifest fields
  (remote catalog first, then the installed one).
  Operations take `<verb> <id> <source> <scope> <architecture> <version>
  <extra-args>` (empty arguments mean "unset"; install and upgrade apply them
  through `InstallOptions`, uninstall applies only the scope), stream
  `G\t<stage>\t<percent>` lines
  (percent empty when WinGet reports none) and keep stdin open: any stdin line,
  or the pipe closing because PowerKit exited, makes the child call `Cancel()`
  on the WinGet async operation. The child polls the operation status on its
  own thread because `IAsyncOperationWithProgress` is not `Send`.
- Upgrade resolves the package through the installed composite catalog (it
  needs the installed version); install connects to the row's source catalog
  (`winget` or `msstore`); uninstall uses the installed catalog.
- Pin the ComInterop / winmd version deliberately and re-verify after WinGet
  upgrades. Prefer Size/GetAt over `for`/`IntoIterator` on projected WinRT
  vectors; the probe's iterator path hit `E_NOINTERFACE` while indexed access
  succeeded.
- Verified operations on this machine: connect to the `winget` catalog, search
  by package id (`Microsoft.WindowsTerminal`), and list installed packages
  through a composite local catalog (`winget` + `msstore`, 154 matches).
  Physical worker-child smoke on native ARM64 with the portable `jqlang.jq`:
  install with download/install progress, cancel during download (status
  `cancelled`, nothing installed), upgrade 1.7.1 → 1.8.2 and uninstall, each
  confirmed with `winget list`. Not exercised: installers that raise UAC,
  `msstore` installs, cancel during an installer run, and elevated
  `WinGetServerManualActivation_CreateInstance`.
- On `CLASS_NOT_REGISTERED`, the worker checks for the
  `Microsoft.DesktopAppInstaller_8wekyb3d8bbwe` package family via
  `Windows.Management.Deployment.PackageManager` and returns sentinel
  unavailable codes (`app-installer-missing` / `app-installer-too-old`) that
  the settings status card maps to localized copy. A numeric App Installer
  floor is intentionally not pinned.
- Emulated x64 (`x86_64-pc-windows-msvc` on ARM64 Windows) worker list/search
  smoke (2026-11-16): `installed` returned 155 lines + `OK`;
  `search-id Microsoft.WindowsTerminal` returned one `winget` row + `OK`.
  That is emulation coverage, not native x64. See
  [validation/power-store-emulated-x64-smoke.md](validation/power-store-emulated-x64-smoke.md).
  MSIX-packaged activation remains open until MSIX packaging exists in-tree.

## User interface

PowerStore is a full-pane workspace in the settings shell navigation rail,
displayed as "Power Store". Its header, unified filter/action toolbar and
five-column table header (`CheckBox`, `Application`, `Version`, `Source`,
`Actions`) stay pinned at the top while only the package rows scroll:

- **Unified list:** Installed packages and updates share one table. When the
  status filter is "All statuses", actionable updates are always pinned in a top
  group (`Updates available`), followed by the remaining installed packages
  (`Installed applications`).
- **Filters and sorting:** A client-side filter box narrows the visible rows by
  name, package ID or source without confusing local filtering with remote
  catalog search. A source button opens a flyout with one checkbox per
  available source (several may be ticked; none means all sources). ComboBoxes
  filter by status (all, updates, up to date, ignored/skipped) and scope
  (`user`, `machine`). One sort ComboBox lists name, package ID, install date
  and last update date each with ↑ (ascending) and ↓ (descending); rows without
  a known date always sort last. Clicking the Application header still flips
  the name sort.
  Install and update dates combine Windows Uninstall registry metadata
  (`InstallDate` and key `LastWriteTime`), install folder timestamps and
  PowerStore's own operation history (`arp.rs`).
- **Logos (layered):** (1) Uninstall registry `DisplayIcon` when present;
  (2) otherwise the first HTTP(S) URL from WinGet `CatalogPackageMetadata.Icons`
  for WinGet rows; (3) optional remote PowerKit icon map
  (`data/package-icons.json`, Manage toggle, **off by default**) fills remaining
  gaps. The catalog worker downloads/encodes PNGs (`icons.rs`); the UI thread
  only reads the in-memory cache. Rows still without an icon show the first
  letter of the name. Enabling remote logos uses the network; the default map in
  this repository starts empty so entries are added deliberately.
- **Actions and selection:** Header `CheckBox` selects or clears all visible
  rows. Top actions provide smart `Update all` / `Update selected` / `Install
  selected`, bulk `Uninstall`, bulk `Ignore updates` and `Refresh`. Page
  `Update all` counts and queues only packages that match the current list
  filters; the tray `Update all` still covers every actionable update. Clicking
  an application name opens its Package Info drawer.
  Each row has one primary button (`Update`, `Install`, `Uninstall` or `Info`)
  plus a `⋯` `MenuFlyout` for details/options, skipping the offered version,
  ignoring/unignoring updates, uninstalling an updatable package, and copying
  the package ID.
- **Discover search dialog:** Clicking "Find Packages" opens a `ContentDialog`
  to search remote catalogs (`winget` / `msstore`) by name, ID and moniker for
  packages that are not yet installed. Results render in the same unified table
  with a mode banner (`Discover results: "…" · Back to installed`).
- **Right-side overlay drawers:** Package Info (manifest details + per-package
  install options), History and Manage open as right-docked overlay drawers with
  light-dismiss backdrop rather than separate pages. Strings live in
  `src/locale.rs` in Turkish, English and Arabic.

The notify-area menu has a PowerStore submenu after the mini-app submenus: a
disabled status row (update count, "up to date", "not checked yet" or
"checking"), "Update all" (enabled only with updates), "Check for updates" and
"Open PowerStore". The root label shows the count, for example
"Power Store (3)".

The first periodic check runs 60 s after start, then every
`power_store_check_minutes` (15 minutes to 7 days; the page steps through 15,
30 min, 1, 2, 3, 6, 12 h, 1, 2, 3 and 7 days). A check lists installed packages
in the worker child; the count applies ignore and skip rules. When
`power_store_notify` is on and the count grows past the last notified value, a
balloon reports it; clicking it opens the Updates section. Nothing is installed
automatically.

## Operations

- One serial queue owns all install, update and uninstall operations. Waiting
  items can be removed one-by-one from the live strip or cleared in bulk; the
  running item can be cancelled when the manager supports it (WinGet typically
  stops between stages once an external installer has started; exit cancels and
  waits at most two seconds, then detaches). Search and update checks may run
  alongside the queue.
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

Persisted through the existing settings writer: `power_store_check_minutes`,
`power_store_notify`, `power_store_ignore`, `power_store_skip` and
`power_store_option=manager|source|id|scope|architecture|version|extra-args`
(at most 512 option entries; the scope must be `user` or `machine`, the
architecture `x64`, `arm64`, `x86` or `arm`, and every value a single line).
Options are reused for later updates of the same package. Operation history is
`power-store-history.txt` beside `settings.txt` (header `powerkit-history=1`,
one tab-escaped line per operation), written through a temporary file and a
rename after each finished operation. It keeps the 200 most recent operations
and at most 4 KiB of log text per operation; the smoke test never loads or
writes it. The queue, search results and update lists are not persisted; a
restart starts with an empty queue.

## Export format

Exports use the WinGet `winget export` JSON shape so `winget import` can read
the WinGet part directly. Packages from other managers and per-package options
are stored under a top-level `PowerKit` object in the same file. The product
exports installed packages that WinGet correlates with a catalog (ARP-only rows
cannot be reinstalled) and writes the file through a temporary file and a
rename. Import parses both parts, shows every phase-1 package with a checkbox,
stores imported options and queues installs only for the rows left selected.

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
- `src/platform/windows/power_store/`: WinGet COM adapter (`winget.rs`, used
  only inside the worker child), the child-process `broker.rs`, Scoop /
  Chocolatey / npm / pip / Cargo / .NET Tool / PowerShell / vcpkg CLI adapters,
  `catalog.rs` fan-out, and a `session` that feeds the settings page and tray
  (lists, selection, periodic checks, details, history, export/import) and runs
  a serial install/update/uninstall queue off the UI thread with progress and
  cancel. `file_dialog.rs` holds the modal JSON open/save dialogs.
- `src/platform/windows/window/store_page.rs`: the Power Store page composed
  from `settings_ui.rs` surfaces.
- `third_party/winget/Microsoft.Management.Deployment.winmd`: vendored build
  input (ComInterop 1.29.380). `build.rs` generates bindings into `OUT_DIR` and
  copies the winmd next to `powerkit.exe`; packaging copies it into the stage
  directory.
## Resolved product decisions

- ARP-only installed rows are shown with source `local` (not hidden by default)
  and appear in the source filter after `winget` / `msstore`.
- App Installer / WinGet unavailable status distinguishes **missing** (no
  `Microsoft.DesktopAppInstaller` package family) from **too old / incomplete**
  (package present but Packaged COM returns `CLASS_NOT_REGISTERED`). No numeric
  minimum App Installer version is pinned to the vendored winmd; activation
  success remains the gate, with those two status-card strings for the common
  failure modes. Other unavailable reasons (missing winmd beside the exe, and
  so on) keep their technical detail text.

## Open questions

- Microsoft Store policy review for an app that installs third-party software.
  UniGetUI is listed in the Store, but PowerKit's submission still has to pass
  its own review.
- Confirm the same COM activation and winmd-beside-exe requirement inside a
  self-signed MSIX install (MSIX packaging is not in-tree yet; only Velopack
  staging exists).
- Interactive physical coverage still open: `msstore` install, UAC-raising
  installers, and cancel after an external installer UI has appeared.

## Next implementation slice

1. After MSIX packaging lands: install/uninstall smoke of PowerStore WinGet
   activation inside a self-signed package; add interactive `msstore` / UAC
   installer checks when an operator can approve prompts.
2. Optional polish: gallery outdated polling for PowerShell modules, Cargo
   crates.io version compare, and pip broad search if a stable CLI appears.
