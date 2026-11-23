# Contributing to PowerKit

PowerKit is maintained by [@Pirimera](https://github.com/Pirimera). English and Turkish
contributions are welcome. Read the [code of conduct](CODE_OF_CONDUCT.md).

## Before changing code

For a bug, include the app version, Windows build, CPU architecture, whether PowerKit
runs from an installed package or as a development build, reproduction steps and expected/actual behavior. Remove personal paths and metadata.
For a substantial feature or dependency, open an issue to discuss scope first.
Small corrections can go directly to a pull request.

Do not post security exploit details in issues; follow [SECURITY.md](SECURITY.md).

## Development

Follow [development.md](docs/development.md) and [architecture.md](docs/architecture.md).
Do not change the version in a pull request; it is raised when a release is prepared.
Keep changes focused and preserve the native ARM64 path. Do not add a framework,
crate split or service without a concrete requirement. Include regression tests for
behavior changes; documentation-only changes need repository checks, not unrelated UI tests.

Run ./scripts/Check.ps1 for the affected Windows architecture. Packaging changes
also need the local MSIX install and uninstall check described in
[releasing.md](docs/releasing.md).
Report separately what was compiled, automated, tested under emulation, and physically
tested. Never present skipped hardware testing as a pass.

## Pull requests

Explain the problem, resulting behavior and relevant validation. Use a descriptive
imperative title; a conventional-commit prefix is welcome but not mandatory.
Link related issues. Include before/after images only for meaningful UI changes.
Document observable behavior changes and update [CHANGELOG.md](CHANGELOG.md).

The maintainer reviews and merges changes. CI checks formatting, lints and tests
on every pull request; its success is necessary but does not replace review. Small projects may use one maintainer review. A documentation
correction needs the repository checks only, not a packaged install.

## Licensing and generated contributions

Contributions are provided under the project's [MIT license](LICENSE).
Only submit work you have the right to contribute. Preserve attribution and
dependency notices; do not paste code from PowerToys or other apps without
checking its license.

AI-assisted changes have the same review, testing and licensing requirements as
human changes. The contributor remains responsible for correctness. Agents follow
[AGENTS.md](AGENTS.md); no blanket permission to publish or alter user files is implied.
No CLA or mandatory sign-off is currently required.
