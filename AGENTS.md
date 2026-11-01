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
changes require scripts/check_repo.py and its tests. Update relevant guides and the
Unreleased changelog for observable changes.

Report what changed, checks actually run and remaining limitations. Separate native,
emulated, simulated and physical evidence. Do not claim CI, signing, release publication,
security settings or branch protection were enabled without verifying them.
Do not fabricate contact addresses, contributors, roadmap dates or benchmark results.

Use English for canonical contributor docs and code identifiers; keep Turkish user
text intact unless changing localization deliberately. Keep rules and skills concise;
put durable architecture details in docs rather than duplicating them everywhere.
