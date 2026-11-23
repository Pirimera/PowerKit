# PowerKit agent instructions

## Start with the task

Read [architecture](docs/architecture.md) and [development](docs/development.md)
when changing code. Inspect git status and preserve unrelated edits. Keep the task
bounded; ordinary fixes do not justify a new crate, framework or service.
The code is a layered workspace so a second UI kit and more platforms can be
added later; [docs/architecture.md](docs/architecture.md#layers) holds the
layout and its rules. New code goes where that layout puts it: domain logic in `crates/core` (no OS, UI or file access),
controllers, port traits and the UI contract in `crates/app`, the WinUI 3
settings UI in `crates/ui/reactor` (it sees only `crates/app` and
`crates/core`), Windows ports in `crates/platform/windows` (it does not
see the UI crate), and the composition of the two in `apps/powerkit`. macOS
and Linux are planned ([docs/roadmap.md](docs/roadmap.md)); do not claim
macOS, Linux or GPUI support, because none exists.
Windows 11 ARM64 is primary. PowerMouse has two layers, described in
[docs/power-mouse.md](docs/power-mouse.md): a generic layer that remaps standard
Windows mouse input for every mouse, with an Actions Ring (implemented and
accepted by hand) and device adapters that add vendor features for selected
mice (Logitech HID++ first: device buttons, battery, DPI, wheel settings,
thumb wheel, Easy-Switch and haptic tap are implemented for MX Master 4 over
Bluetooth; receivers are planned).
Adapter features stay off for models that were not physically verified. The generic layer
must keep working with no adapter present. Mice only: keyboards, lights and
webcams are out of scope, and so are per-application profiles.

## Select relevant repo skills

- [.agents/skills/powerkit-development/SKILL.md](.agents/skills/powerkit-development/SKILL.md):
  native features, fixes and refactors.
- [.agents/skills/powerkit-release/SKILL.md](.agents/skills/powerkit-release/SKILL.md):
  packaging, Store/winget publication, versioning or release-workflow changes.

These skills are repo-local and need not be installed globally. If the agent runtime
does not discover them automatically, read the linked file. Do not load unrelated
skills, spawn agents or add agent tooling merely because it is available.

## Preserve product invariants

Keep Windows FFI in crates/platform/windows (which owns the atomic settings writer),
the application shell in apps/powerkit, the Win32 hosting in crates/ui/reactor
and the Explorer DLL; `crates/core` forbids `unsafe` and takes no dependency.
Explain ownership, valid pointers and thread affinity when touching unsafe code.
Power requests must be released on their owning UI thread on every exit path.
Never resume an old active session from preferences.

Keep network operations off the UI thread. Do not force-restart an AlwaysOn session.
Preserve package identity, architecture coverage and settings location. Never embed
a Store, winget, signing or GitHub secret in source or shipped binaries.

Tools that edit the user's files require preview, collision handling and recovery. Use synthetic
fixtures, not the user's original media. Packaging, system registration, Store
submission and winget publication require authorization from the task; do not
infer them from this file.

## Implementation and completion

Use narrow modules, meaningful names and the existing Rust/Win32 approach. Add a
dependency only for a concrete benefit; verify ARM64 support and licensing.
Keep toolchain/version data consistent and Cargo.lock tracked.
Do not hand-edit generated lockfiles or check in target/dist/local tools.

Run checks appropriate to the change; use scripts/Check.ps1 as the standard Windows
entry point. Packaging or Store-submission changes also need a real local MSIX
install/uninstall check (self-signed is enough on the developer machine).
Documentation-only changes require
`cargo test --manifest-path scripts/Cargo.toml --locked`. Update relevant guides
and the Unreleased changelog for observable changes. A change users can see
also updates the user guide in `docs/user`, in English and in Turkish
(`docs/user/tr`); it describes only behavior that exists and says what was
not tried by hand.

Report what changed, checks actually run and remaining limitations. Separate native,
emulated, simulated and physical evidence. Do not claim CI, signing, Store
publication, winget listing, security settings or branch protection were enabled
without verifying them.
Do not fabricate contact addresses, contributors, roadmap dates or benchmark results.

Use English for canonical contributor docs and code identifiers; keep Turkish user
text intact unless changing localization deliberately. Keep rules and skills concise;
put durable architecture details in docs rather than duplicating them everywhere.

## Learned User Preferences

- Reply to the user in Turkish; keep English for code identifiers and contributor docs.
- When the user says to run the app ("uygulamayı çalıştır" and similar), start the
  unpackaged build from `target/` (for example
  `cargo run --target aarch64-pc-windows-msvc`); do not package or install MSIX
  unless they ask for packaging or a Store/sideload trial.
- Ship the product UI in Turkish, English and Arabic.
- Distribution is Microsoft Store as the primary channel, with winget pointing at
  the Store package. Do not ship GitHub Releases, Setup.exe or self-update feeds as
  a product download path. Source may remain on GitHub.
- Ship as MSIX. Declare Windows App Runtime 2.4+ as a Store package dependency;
  do not redistribute `WindowsAppRuntimeInstall.exe` on the product channel and
  do not keep a tray-only WinUI fallback once packaged Store builds are the
  supported install. Day-to-day unpackaged builds still need an installed 2.4+.
- Prefer WinUI 3 (windows-reactor) for settings, dialogs and on-screen toolbars.
  Keep the tray menu, hotkeys, ScreenRuler measurement overlays and AlwaysTop
  frame as Win32/Direct2D. Add no third UI stack.
- In settings UI, put control labels to the left of toggles and similar switches
  (not to the right); keep that pattern consistent across the whole app.
- Prefer Rust over Python for repository checks and packaging helpers.
- The version changes only when a release is prepared, with the `bump-version`
  tool in the scripts crate (see `docs/development.md`); an ordinary commit or
  pull request never changes it. There are no git hooks.
- "Create a release to test" means a local test build: commit the current work
  (the version stays as it is unless asked), rebuild `--release` for ARM64 and package with
  `scripts/Package.ps1`, making sure the latest changes are in the build; do
  not push, tag or publish unless asked.
- CI (`.github/workflows/ci.yml`) checks formatting, lints and tests on pull
  requests and main pushes, with no secret and no packaging. Keep the
  packaging workflow (`release.yml`) manual; do not build packages on a push.
- When launched at Windows startup/logon (or with `--background`), stay in the
  tray and do not open the main window in the foreground.
- Settings shell chrome should match Microsoft Store pixel-for-pixel where
  practical: fixed icon-over-label navigation rail (not expand/collapse) with
  near-square item targets, smaller icons/labels, centered wrapping labels and
  spaced tool display names (e.g. Always Top); Home first (app
  info, tool cards and changelog); Settings via a bottom rail button; acrylic title bar
  with left-aligned PowerKit logo and name; shell and main content both stay
  acrylic, with the main area about 50% less transparent and a Store-like
  rounded top-left content separation; window transparency only adjusts
  acrylic opacity (never fully clear glass); prefer Store-like animated nav
  selection transitions.

## Learned Workspace Facts

- Product artwork lives in `brand/`; embed `brand/windows` ICOs for the window and
  tray, and do not restyle native Win32 chrome with brand colors or Inter Tight.
- CRT is statically linked; no Visual C++ Redistributable or .NET is required.
  Store-packaged builds rely on the Store to satisfy the Windows App Runtime
  dependency. Local MSIX trials use a self-signed test certificate and Developer
  Mode; paid Authenticode/Trusted Signing is not required for that path, and
  Store publication re-signs the submitted package.
- Keep the notify-area menu as native Win32: follow the Windows taskbar theme, put only PowerKit-wide items plus status indicators at the root, nest each tool in its own submenu, and use checkmarks plus a radio group for AlwaysOn. AlwaysTop uses a check to pin the last eligible window and a check for pin/opacity sound. ScreenRuler uses a check to measure. ImageResizer and PowerStore (update count, status, Update all, Check for updates, Open PowerStore) each have their own submenu.
- The main window is a WinUI 3 settings shell with a Store-like navigation rail
  and layered acrylic chrome/content that follows the current Windows theme
  rather than brand chrome; pages cover Home, PowerStore,
  MediaRenamer, AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer, PowerMouse and
  Settings.
- Tool pages are utilities (settings page; work from tray, hotkey or a task
 window) or workspaces (work on the page; gear Settings drawer rightmost); never
 label settings "Manage". See `docs/ui-structure.md`. MediaRenamer is a
 workspace (rename in place, preview-first, undoable History); its product
 decisions and status live in `docs/media-renamer.md`.
- AlwaysOn should match PowerToys Awake for keep-awake modes and related
  behavior; selected mode must stay visually obvious in the settings page.
- AlwaysTop should match PowerToys Always On Top: multiple simultaneous pins, activation hotkey, pinned-window border, sound and transparency; do not persist pins or opacity.
- ScreenRuler and ImageResizer should match PowerToys counterparts; ImageResizer
  needs Explorer shell context-menu integration for selected images.
- Packaging and Store/winget publication start only from an authorized manual
  workflow. A matching `vX.Y.Z` tag remains the release identity. There is no
  in-app updater; development runs unpackaged.
  `scripts/Package.ps1` builds an MSIX from `packaging/msix/AppxManifest.xml`
  and stages existing release binaries without compiling, so rebuild
  `--release` first. Packaged builds rely on the manifest for the startup task
  and Explorer commands instead of runtime registry/sparse-package registration.
- PowerStore is UniGetUI-inspired (not a port; serial off-UI
  install/update/uninstall queue; WinGet via COM, plus Scoop, Chocolatey 2.0+,
  npm, pip, Cargo, .NET tool, PowerShell and vcpkg via off-UI CLI adapters;
  hide missing managers; no auto-updates or cloud/Gist sync in v1; design in
  `docs/power-store.md`): use one unified package table with a
  sticky filter/action bar and select-all header (actionable updates pinned
  above installed apps, instant local filter, source/status/scope filters,
  name/ID/install-date/last-updated sort), show the last cached list immediately
  then refresh managers in the background (optimistic update), a modal Discover
  dialog for uninstalled packages rendering in the same table, and right-side
  overlay drawers for Package Info/Options, History and Settings instead of
  separate subpages.
- Never call WinGet COM in-process from the WinUI host: Windows App Runtime's
  dynamic package dependency (`AddPackageDependency`) breaks in-process
  `Microsoft.Management.Deployment` calls (`Package Microsoft.WindowsAppRuntime…
  not found`); run WinGet COM in the WinUI-free child worker
  (`powerkit-winget-worker.exe`, built from `apps/winget-worker` and
  installed beside `powerkit.exe`), ship
  `Microsoft.Management.Deployment.winmd` beside the running process image
  (the platform crate's build.rs and the MSIX stage it), and gate updates on WinGet's
  `IsUpdateAvailable` rather than version string inequality.
- In windows-reactor WinUI 3 layouts, a stretched element with `MaxWidth`
  shifts right and clips past the window edge once its parent exceeds
  `MaxWidth`; do not put `MaxWidth` on stretched page content, and keep
  horizontal scrolling disabled on page `ScrollViewer`s.
