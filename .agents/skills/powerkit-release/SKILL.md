---
name: powerkit-release
description: Prepare or review PowerKit MSIX packaging, Microsoft Store and winget publication, versioning and release automation.
---

# PowerKit release engineering

Read [AGENTS.md](../../../AGENTS.md) and [releasing](../../../docs/releasing.md).
Preparation does not authorize tag pushes, Store submission, winget publication,
installation or repo visibility changes.

## Product channel

Microsoft Store is the primary distribution channel. winget must point at the
Store package. Do not publish GitHub Releases, Setup.exe or self-update feeds as
a product download path. Source code may remain on GitHub.

Ship MSIX for ARM64 and x64. Declare Windows App Runtime 2.4 or later as a Store
package dependency. Do not redistribute the Windows App Runtime installer on the
product channel. Store re-signs submitted packages; local trials use a
self-signed test certificate and Developer Mode.

## Versioning and publication

Check Cargo.toml, Cargo.lock, app.manifest, rust-toolchain.toml and packaging
pins. Run the repo checker for drift. Do not overwrite published versions.
A normal commit bumps the stable version and writes its release notes.
feat raises minor, a breaking change raises major, and every other message
raises patch. Do not hand-bump those files. A published release still needs a
matching vX.Y.Z tag. Tag pushes alone do not submit to the Store or winget.

Preserve Pirimera.PowerKit (or the Partner Center identity once assigned) and
build both architectures before submission. Prefer draft/staged Store rollout
before making a version mandatory.

## Safety

Never embed Store, winget, signing or GitHub secrets, disable integrity/TLS
checks or bypass OS protections. Pending updates must wait for UI-thread power
cleanup and graceful exit; do not force-restart an AlwaysOn session.
Inspect distribution licenses/notices.

Separate package checks from installed upgrade/uninstall and hosted Actions or
Partner Center evidence. If signing credentials or Partner Center access are
missing, complete authorized local work and report the exact remaining task
without claiming completion.

## Packaging

`packaging/msix/AppxManifest.xml` is the template; `scripts/Package.ps1` fills
it and packs release binaries (build `--release` first). Keep the startup
`TaskId` equal to `src/platform/windows/startup.rs` and the Explorer CLSIDs
sourced from `src/shell_clsid*.txt`. The manual workflow only builds unsigned
MSIX artifacts; Partner Center submission is not automated.
