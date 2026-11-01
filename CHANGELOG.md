# Changelog

Notable user-visible changes are recorded here. Release dates are added only after
publication; a source version is not proof of a published release.

## Unreleased

- Group the tray menu by tool: PowerKit actions at the root, AlwaysOn in a status-bearing submenu with a mode radio group and display check.
- Align the tray popup menu with the Windows taskbar light/dark theme.
- Tighten the Win32 AlwaysOn window module: named controls, shared labels, testable command mapping and tray icon reuse on tooltip ticks.
- Replace Python repository and license-notice helpers with a small Rust scripts crate.
- Add the PowerKit brand kit and use its Windows icons for the window, tray and installer.
- Standardize contributor documentation, MIT licensing and repository checks.
- Add discoverable development/release skills and maintainable agent instructions.
- Include generated third-party license notices in installer payloads.
- Enforce thread-local power guard ownership in the type system.
- Separate native power ownership and smoke checks from the Windows UI module.

## 0.2.0 — publication not verified

- Add ARM64/x64 Velopack installers and architecture-specific update channels.
- Download updates in the background and apply after normal exit.
- Add GitHub Actions packaging, package integrity tests and tag-triggered releases.

## 0.1.1

- Harden AlwaysOn cleanup, settings persistence, DPI and resume behavior.
- Add isolated native smoke checks.

## 0.1.0

- Initial native AlwaysOn prototype.
