# Security policy

## Supported versions

During the 0.x phase, fixes target the latest stable release and main.
Older releases do not have a separate security maintenance branch.

## Reporting

If enabled, use [GitHub private vulnerability reporting](https://github.com/Pirimera/PowerKit/security/advisories/new).
This requires repository configuration and is not guaranteed to be available.
If the form is unavailable, open an issue asking for a private contact method
**without vulnerability details, exploit code or sensitive data**.

Include affected versions, architecture, impact and a minimal reproduction privately.
There is no guaranteed response time or bug-bounty program. Coordinate disclosure
with the maintainer rather than publishing an unpatched exploit in an issue.

## Relevant boundaries

PowerKit has no in-app updater: the Microsoft Store delivers and signs product
packages. A locally built MSIX is unsigned or signed with a self-signed test
certificate, which does not authenticate a publisher. Tokens, private signing
keys and user metadata must never be embedded in the app or committed.

PowerStore runs other package managers with the user's own rights and never
elevates itself; each installer raises its own prompt. PowerMouse installs
low-level input hooks in the user session, never a driver or a service.
AlwaysOn requests and AlwaysTop z-order restores belong to the UI thread and
must be released there. A tool that edits the user's files must preview
changes, detect collisions and support recovery.
See [architecture](docs/architecture.md) and [releasing](docs/releasing.md).
