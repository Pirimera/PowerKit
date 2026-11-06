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
  installer, updater, versioning or release-workflow changes.

These skills are repo-local and need not be installed globally. If the agent runtime
does not discover them automatically, read the linked file. Do not load unrelated
skills, spawn agents or add agent tooling merely because it is available.

## Preserve product invariants

Keep Windows FFI in src/platform/windows and the existing settings writer.
Explain ownership, valid pointers and thread affinity when touching unsafe code.
Power requests must be released on their owning UI thread on every exit path.
Never resume an old active session from preferences.

Keep network operations off the UI thread. Do not force-restart an AlwaysOn session.
Preserve package identity, architecture channels, checksum validation and settings
location. Never embed a GitHub token or signing secret in source or shipped binaries.

Future media edits require preview, collision handling and recovery. Use synthetic
fixtures, not the user's original media. Installer execution, system registration
and publishing require authorization from the task; do not infer it from this file.

## Implementation and completion

Use narrow modules, meaningful names and the existing Rust/Win32 approach. Add a
dependency only for a concrete benefit; verify ARM64 support and licensing.
Keep toolchain/version data consistent and Cargo.lock tracked.
Do not hand-edit generated lockfiles or check in target/dist/local tools.

Run checks appropriate to the change; use scripts/Check.ps1 as the standard Windows
entry point. Packaging changes also require the real-feed test. Documentation-only
changes require `cargo test --manifest-path scripts/Cargo.toml --locked`. Update relevant guides and the
Unreleased changelog for observable changes.

Report what changed, checks actually run and remaining limitations. Separate native,
emulated, simulated and physical evidence. Do not claim CI, signing, release publication,
security settings or branch protection were enabled without verifying them.
Do not fabricate contact addresses, contributors, roadmap dates or benchmark results.

Use English for canonical contributor docs and code identifiers; keep Turkish user
text intact unless changing localization deliberately. Keep rules and skills concise;
put durable architecture details in docs rather than duplicating them everywhere.

## Learned User Preferences

- Reply to the user in Turkish; keep English for code identifiers and contributor docs.
- Keep the native Rust/Win32 UI; do not add WinUI, WinAppSDK, or another UI stack.
- Prefer Rust over Python for repository checks and packaging helpers.
- Each normal commit advances the stable SemVer: `feat` raises minor, a breaking change raises major, and every other message raises patch. Merge, squash, amend, rebase, and cherry-pick leave the version unchanged.

## Learned Workspace Facts

- Product artwork lives in `brand/`; embed `brand/windows` ICOs for the window, tray and Velopack icon, and do not restyle native Win32 chrome with brand colors or Inter Tight.
- The per-user installer does not require Visual C++ Redistributable, .NET or WinUI runtime; CRT is statically linked.
- Keep the notify-area menu as native Win32: follow the Windows taskbar theme, put only PowerKit-wide items plus status indicators at the root, nest each mini-app in its own submenu, and use checkmarks plus a radio group for AlwaysOn. AlwaysTop uses a check to pin the last eligible window and a check for pin/opacity sound. ScreenRuler uses a check to measure. ImageResizer has its own submenu.
- The main window is a native settings shell (AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer and General) that follows the current Windows theme rather than brand chrome.
- AlwaysTop should match PowerToys Always On Top: multiple simultaneous pins, activation hotkey, pinned-window border, sound and transparency; do not persist pins or opacity.
- GitHub build and release start only from a manual workflow dispatch. Publication runs only when that run sets `publish` and a matching `vX.Y.Z` tag.
