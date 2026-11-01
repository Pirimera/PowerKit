# Security policy

## Supported versions

During the 0.x phase, fixes target the latest stable release and main.
Older releases do not have a separate security maintenance branch.

## Reporting

If enabled, use [GitHub private vulnerability reporting](https://github.com/Pirimera/PowerKit/security/advisories/new).
This requires repository configuration and is not guaranteed to be available while
the repository is private. If the form is unavailable, contact the maintainer through
an existing private channel. If none exists, open an issue asking for a private contact
method **without vulnerability details, exploit code or sensitive data**.

Include affected versions, architecture, impact and a minimal reproduction privately.
There is no guaranteed response time or bug-bounty program. Coordinate disclosure
with the maintainer rather than publishing an unpatched exploit in an issue.

## Relevant boundaries

Update feeds use HTTPS and package integrity checks. Initial builds are unsigned:
checksums do not authenticate a publisher. Tokens, private signing keys and user
metadata must never be embedded in the app or committed.

AlwaysOn requests belong to the UI thread and must be released there.
Future media operations must preview changes, detect collisions and support recovery.
See [architecture](docs/architecture.md) and [release readiness](docs/releasing.md).
