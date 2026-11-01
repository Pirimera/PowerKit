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
Power ownership cannot migrate to another thread. Smoke checks stay isolated from
preferences, normal singleton state and update networking. For updates, consult the
release skill. For future media tools, design preview/collision/recovery before writes.

Use synthetic regression cases that exercise failures or boundaries, not source-text
assertions. Run scripts/Check.ps1 for the affected architecture. UI/power changes
also need a release build and Test-AlwaysOn.ps1. For docs-only edits run
`cargo test --manifest-path scripts/Cargo.toml --locked`. Record physical checks separately.

Review for machine-specific paths, extra dependencies and generated files. Update
the relevant guide and changelog. Distinguish native, emulated and physical evidence.
