# PowerKit agent instructions

## Start with the task

Read [architecture](docs/architecture.md) and [development](docs/development.md)
when changing code. Inspect git status and preserve unrelated edits. Keep the task
bounded; ordinary fixes do not justify a workspace split, new framework or service.
Windows 11 ARM64 is primary. MouseMapper is out of scope.

## Select relevant repo skills

- [.agents/skills/powerkit-development/SKILL.md](.agents/skills/powerkit-development/SKILL.md):
  native features, fixes and refactors.
- [.agents/skills/powerkit-release/SKILL.md](.agents/skills/powerkit-release/SKILL.md):
  packaging, Store/winget publication, versioning or release-workflow changes.

These skills are repo-local and need not be installed globally. If the agent runtime
does not discover them automatically, read the linked file. Do not load unrelated
skills, spawn agents or add agent tooling merely because it is available.

## Preserve product invariants

Keep Windows FFI in src/platform/windows and the existing settings writer.
Explain ownership, valid pointers and thread affinity when touching unsafe code.
Power requests must be released on their owning UI thread on every exit path.
Never resume an old active session from preferences.

Keep network operations off the UI thread. Do not force-restart an AlwaysOn session.
Preserve package identity, architecture coverage and settings location. Never embed
a Store, winget, signing or GitHub secret in source or shipped binaries.

Future media edits require preview, collision handling and recovery. Use synthetic
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
and the Unreleased changelog for observable changes.

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
- Each normal commit advances the stable SemVer: `feat` raises minor, a breaking change raises major, and every other message raises patch. Merge, squash, amend, rebase, and cherry-pick leave the version unchanged.
- Keep CI build and release workflows manual; do not auto-build on every main push.
- "Create a release to test" means a local test build: commit the current work
  (the hook bumps the version), rebuild `--release` for ARM64 and package with
  `scripts/Package.ps1`, making sure the latest changes are in the build; do
  not push, tag or publish unless asked.
- Settings shell chrome should match Microsoft Store pixel-for-pixel where
  practical: fixed icon-over-label navigation rail (not expand/collapse) with
  near-square item targets, smaller icons/labels, centered wrapping labels and
  spaced tool display names (e.g. Always Top); General first as home (app
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
  rather than brand chrome; pages cover General (home), PowerStore,
  MediaRenamer, AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer and Settings.
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
  workflow. A matching `vX.Y.Z` tag remains the release identity. Velopack and
  the in-app updater are gone; development runs unpackaged with no updater.
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
  name/ID/install-date/last-updated sort), a modal Discover dialog for
  uninstalled packages rendering in the same table, and right-side overlay
  drawers for Package Info/Options, History and Settings instead of separate
  subpages.
- Never call WinGet COM in-process from the WinUI host: Windows App Runtime's
  dynamic package dependency (`AddPackageDependency`) breaks in-process
  `Microsoft.Management.Deployment` calls (`Package Microsoft.WindowsAppRuntime…
  not found`); run WinGet COM in the WinUI-free child worker
  (`powerkit.exe --power-store-worker`), ship
  `Microsoft.Management.Deployment.winmd` beside the running process image
  (build.rs and the MSIX stage it), and gate updates on WinGet's
  `IsUpdateAvailable` rather than version string inequality.
- In windows-reactor WinUI 3 layouts, a stretched element with `MaxWidth`
  shifts right and clips past the window edge once its parent exceeds
  `MaxWidth`; do not put `MaxWidth` on stretched page content, and keep
  horizontal scrolling disabled on page `ScrollViewer`s.
