# Architecture

PowerKit is one Rust binary crate. Keep it this way until independently reusable
components or independently deployable programs justify a workspace. There is no
plugin loader, background service or web runtime.

## Boundaries

| Location | Responsibility |
| --- | --- |
| src/main.rs | Velopack startup hook, then platform entry point |
| src/always_on.rs | Pure mode/deadline transitions |
| src/platform/windows/mod.rs | UI thread, message loop, tray, controls, lifecycle |
| src/platform/windows/power.rs | Execution-state flags and RAII cleanup |
| src/platform/windows/smoke.rs | Isolated native message-loop checks |
| src/settings.rs | Bounded settings parsing and atomic Windows replacement |
| src/updates.rs | Worker, update state, architecture channel and pending package |
| packaging/windows/ | End-user files included in the installer |
| scripts/ | Build, checks and packaging; no product behavior |
| .github/ | Automation and contributor entry points |
| .agents/skills/ | Task-specific guidance for automated contributors |

New tools should first get a small domain module and explicit platform adapter.
Keep domain logic independently testable; avoid duplicating a shell or creating
empty modules for planned tools. Prefer narrow visibility over shared mutable state.

## Non-negotiable behavior

- Power requests must be removed on their owning UI thread, including error paths.
- Do not persist an active AlwaysOn session. Resume reconciles deadlines without extension.
- FFI belongs in the Windows adapter or the existing atomic settings writer.
  Explain pointer validity, ownership and thread affinity around unsafe operations.
- Network/download work stays off the UI thread. A pending update must not interrupt
  an active session. Start the update helper only after power cleanup.
- Stable package identity is Pirimera.PowerKit; channels are win-arm64 and win-x64.
  No GitHub credentials ship in the app. Do not weaken integrity checks.
- Settings are outside the installation directory. Preserve malformed settings until
  an explicit user change replaces them.
- Future batch media operations require preview, deterministic collision handling
  and recoverable changes before touching original files.

## Design records

Document meaningful decisions here or in a focused new design document when needed.
Avoid creating an ADR for routine refactoring. Keep behavior and limitations explicit;
do not infer support for unimplemented tools.
