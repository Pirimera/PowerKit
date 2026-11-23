//! What a manager's detail drawer shows about the program behind it: where it was found
//! and, for a manager that is switched on, which version it reports.
//!
//! [`probe`] may start the program, so it runs on a worker thread.
use super::{Cx, cargo_mgr, chocolatey, dotnet_tool, npm, pip, powershell_mgr, scoop, vcpkg};
use crate::controller::power_store::ManagerProbe;
use powerkit_core::power_store::ManagerKind;
use std::path::{Path, PathBuf};

/// Longest token accepted as a version; longer tokens are commit hashes or noise.
const MAX_VERSION_CHARS: usize = 48;

/// A switched-off manager is located but never started, like everywhere else in PowerStore.
pub fn probe(cx: &Cx, kind: ManagerKind) -> ManagerProbe {
    let Some((program, prefix)) = locate(cx, kind) else {
        return ManagerProbe::default();
    };
    let version = cx
        .is_enabled(kind)
        .then(|| version_of(cx, kind, &program, &prefix))
        .flatten();
    ManagerProbe {
        path: Some(program.display().to_string()),
        version,
    }
}

/// The program plus the arguments that must precede the version request (pip may run
/// as `python -m pip`).
fn locate(cx: &Cx, kind: ManagerKind) -> Option<(PathBuf, Vec<String>)> {
    let program = match kind {
        ManagerKind::WinGet => cx.locate_program(kind, || cx.find("winget")),
        ManagerKind::Scoop => scoop::locate(cx),
        ManagerKind::Chocolatey => chocolatey::locate(cx),
        ManagerKind::Npm => npm::locate(cx),
        ManagerKind::Pip => return pip::locate_pip(cx),
        ManagerKind::Cargo => cargo_mgr::locate(cx),
        ManagerKind::DotnetTool => dotnet_tool::locate(cx),
        ManagerKind::PowerShell | ManagerKind::PowerShell7 => powershell_mgr::locate(cx, kind),
        ManagerKind::Vcpkg => vcpkg::locate(cx),
    }?;
    Some((program, Vec::new()))
}

fn version_of(cx: &Cx, kind: ManagerKind, program: &Path, prefix: &[String]) -> Option<String> {
    let mut args: Vec<&str> = prefix.iter().map(String::as_str).collect();
    args.extend(version_args(kind));
    let text = cx.run_output(program, &args).ok()?;
    extract_version(&text)
}

fn version_args(kind: ManagerKind) -> &'static [&'static str] {
    match kind {
        ManagerKind::Vcpkg => &["version"],
        // Windows PowerShell 5.1 has no `--version`; both shells answer this.
        ManagerKind::PowerShell | ManagerKind::PowerShell7 => &[
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
        _ => &["--version"],
    }
}

/// First word of `text` that looks like a version: starts with a digit (after an optional
/// `v`) and holds a `.` or `-`, as in `v1.9.25`, `24.0` or `2025-01-29-abc`.
fn extract_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|word| {
        let word = word.trim_matches([',', ';', '(', ')', '[', ']']);
        let word = word.strip_prefix(['v', 'V']).unwrap_or(word);
        let starts_with_digit = word.chars().next().is_some_and(|c| c.is_ascii_digit());
        (starts_with_digit
            && word.contains(['.', '-'])
            && word.chars().count() <= MAX_VERSION_CHARS)
            .then(|| word.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_versions_from_typical_version_output() {
        for (text, expected) in [
            ("v1.9.25200\n", "1.9.25200"),
            ("10.0.404\r\n", "10.0.404"),
            ("cargo 1.80.0 (376290515 2024-07-16)\n", "1.80.0"),
            ("pip 24.0 from C:\\Python\\Lib (python 3.12)\n", "24.0"),
            ("PowerShell 7.4.1\n", "7.4.1"),
            (
                "Current Scoop version:\nv0.5.2 - Released at 2024-01-01\n",
                "0.5.2",
            ),
            (
                "vcpkg package management program version 2025-01-29-0123abc\n",
                "2025-01-29-0123abc",
            ),
            ("5.1.26100.1\n", "5.1.26100.1"),
        ] {
            assert_eq!(extract_version(text).as_deref(), Some(expected), "{text}");
        }
    }

    #[test]
    fn ignores_text_without_a_version() {
        assert_eq!(extract_version(""), None);
        assert_eq!(extract_version("usage: tool [options]\n"), None);
        assert_eq!(extract_version("deadbeefcafe 3\n"), None);
    }
}
