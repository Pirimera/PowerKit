---
name: powerkit-release
description: Prepare or review PowerKit Velopack installers, architecture-specific updates and GitHub release automation, including private-to-public repository transitions.
---

# PowerKit release engineering

Read [AGENTS.md](../../../AGENTS.md) and [releasing](../../../docs/releasing.md).
Preparation does not authorize tag pushes, publication, installation or repo visibility changes.

Check Cargo.toml, Cargo.lock, app.manifest, rust-toolchain.toml and the pinned
Velopack CLI/SDK. Run the repo checker for drift. Do not overwrite published versions.
A normal commit bumps the stable version and writes its release notes.
feat raises minor, a breaking change raises major, and every other message
raises patch. Do not hand-bump those files. A published release still needs a
matching vX.Y.Z tag. Pushing that tag does not build or publish. Publication
is a manual workflow dispatch with `publish` enabled and the same tag.

Preserve Pirimera.PowerKit, win-arm64 and win-x64. Package the matching PE architecture
in a fresh directory; verify hashes and run the ignored real-feed test against it.
Both architectures must pass before draft-first publication. Keep minimal job permissions.

Never embed GitHub tokens, disable integrity/TLS checks or bypass OS protections.
Pending updates wait for UI-thread power cleanup and graceful exit.
Inspect distribution licenses/notices. SHA256 does not authenticate a publisher.

Separate package checks from installed upgrade/uninstall and hosted Actions evidence.
If signing credentials or external access are missing, complete authorized local work
and report the exact remaining task without claiming completion.
