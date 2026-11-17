---
name: powerkit-development
description: Implement and review PowerKit Rust features, Windows lifecycle fixes and refactors while preserving ARM64, power ownership and file safety.
---

# PowerKit development

Read [AGENTS.md](../../../AGENTS.md), [architecture](../../../docs/architecture.md)
and [development](../../../docs/development.md). Paths are relative to this skill.

Trace behavior through pure state, the Win32 boundary and persistence. Keep domain
decisions outside message handlers where practical. Add modules only for implemented
responsibilities; no generic plugin system for planned tools.

For AlwaysOn, review normal/error/session-end cleanup and deadline reconciliation.
Power ownership cannot migrate to another thread. For AlwaysTop, restore
`HWND_NOTOPMOST` on that same thread unless the window was already topmost; do
not persist pins or window opacity. Pin the last focused window, not z-order;
keep every pin until that window is toggled, closed or PowerKit exits.
Keep the layered D2D FrameDrawer ring immediately behind the tracked window
and reapply `HWND_TOPMOST` when focus or restore drops it, matching PowerToys
Always On Top. Restore the original layered style after opacity steps. Play the Windows
Media pin and opacity cues only when sound is enabled. Destroy the
border window with the pin. Smoke checks stay isolated from
preferences and normal singleton state. Startup and Explorer registration differ
between unpackaged and MSIX builds (`package::is_packaged`); for packaging,
consult the release skill. For future media tools, design
preview/collision/recovery before writes.

Use synthetic regression cases that exercise failures or boundaries, not source-text
assertions. Run scripts/Check.ps1 for the affected architecture. UI/power changes
also need a release build and Test-AlwaysOn.ps1. For docs-only edits run
`cargo test --manifest-path scripts/Cargo.toml --locked`. Record physical checks separately.

Review for machine-specific paths, extra dependencies and generated files. Update
the relevant guide and changelog. Distinguish native, emulated and physical evidence.
