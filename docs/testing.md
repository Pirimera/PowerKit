# Testing and troubleshooting

## Automated checks

~~~powershell
./scripts/Check.ps1 -Architecture arm64
cargo build --locked --release --target aarch64-pc-windows-msvc
./scripts/Test-AlwaysOn.ps1
~~~

Check.ps1 checks repository metadata/links, rustfmt, unit tests and Clippy.
The real-package test is intentionally ignored by ordinary cargo test because it
needs vpk output; the release workflow runs it explicitly after packaging.
See [releasing](releasing.md) for that command.

Smoke checks use a separate mutex/window class, skip user preferences and update
networking, and release their power request on their own thread. They may briefly
show a test window. They are not a substitute for real sleep or installer testing.

## Physical acceptance

- Install ARM64 Setup.exe; verify Start menu and Installed apps entries.
- Exercise indefinite/timed AlwaysOn with display option on and off.
- Verify stop/exit restores idle sleep; test battery and AC independently.
- Test manual sleep/resume, tray hide/show, Explorer restart and different monitor DPI.
- Upgrade between two installed versions; preserve display preference and start off.
- When an update is ready, verify an active session is not interrupted.
- Test offline/private-repo behavior and interrupted downloads.
- Uninstall and verify app process, shortcuts and registration are removed.
  User preferences intentionally remain outside the installation root.

Keep other sleep blockers off when measuring sleep behavior. Record native versus
emulated architecture, Windows build, source revision and observed outcome.
Historical evidence lives in [validation](validation/README.md); never overwrite
earlier results to imply they apply to a new build.

## Common issues

- Raw development executable: automatic updates require a Velopack installation.
- Private repository: anonymous update checks cannot access releases; install manually.
- Unsigned installer: publisher identity is not verified. Signing is a release task;
  do not recommend disabling Windows protection.
- Missing clang: check LLVM bin is on PATH (or POWERKIT_LLVM_ROOT for the portable helper).
- Power request testing: timers, Windows sleep settings and other apps affect observations.
