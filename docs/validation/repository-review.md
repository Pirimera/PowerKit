# Repository standards review

Source line: 0.2.0, repository organization follow-up (2026-11-01).
This record covers local checks, not a hosted Actions run or a public launch.

## Changes

- Adopt MIT with maintainer approval; include exact dependency license notices in packages.
- English contributor docs and Turkish introduction; versioned historical validation.
- CONTRIBUTING, SECURITY, SUPPORT, code of conduct, changelog, issue/PR templates and CODEOWNERS.
- Canonical AGENTS.md plus development and release skills under .agents/skills.
- Separate Windows UI, power ownership and smoke checks. PowerGuard is not Send/Sync.
- Pin the Rust toolchain; add repository metadata/link/skill checks, regression tests,
  SHA-pinned workflow linting and Dependabot update proposals.
- Keep installer user documentation separate from contributor documentation.

## Local evidence

- ARM64: 12 ordinary Rust tests, formatting, Clippy with warnings denied, release build,
  native smoke and real-package integration test passed.
- x64: the same checks passed on ARM64 Windows under emulation, not on native x64 hardware.
- Nine repository-check regression tests passed, including rejected version drift,
  mutable Action references, broken/escaping links and missing skill descriptions.
- Both skills passed the skill-creator validator.
- GitHub workflow passed actionlint locally.
- Both installer architectures were generated with MIT/dependency notices;
  package feed hashes and corrupted-package rejection passed.

## Still external or physical

Hosted Actions has not been run for this change. Branch/tag protection, private
vulnerability reporting, security scanning and signing were not enabled here.
Follow the current [release checklist](../releasing.md) before public launch.
Installed upgrade/uninstall and real sleep testing remain separate acceptance tasks.
