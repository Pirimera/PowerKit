//! Raise the stable PowerKit version for a release.
//!
//! `bump-version <major|minor|patch>` rewrites the three files that carry the
//! product version and prints the new one. It is run by hand when a release
//! is prepared; nothing runs it on a commit. The changelog is written by
//! hand as well.
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

/// The application package: its version is the product version.
const CARGO_TOML: &str = "apps/powerkit/Cargo.toml";
const APP_MANIFEST: &str = "apps/powerkit/app.manifest";

static PACKAGE_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^version = "(\d+\.\d+\.\d+)""#).unwrap());
static MANIFEST_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"version="\d+\.\d+\.\d+\.\d+""#).unwrap());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bump {
    Major,
    Minor,
    Patch,
}

impl Bump {
    fn parse(level: &str) -> Option<Self> {
        match level {
            "major" => Some(Self::Major),
            "minor" => Some(Self::Minor),
            "patch" => Some(Self::Patch),
            _ => None,
        }
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("Could not read {}: {error}", path.display()))
}

fn next_version(current: &str, bump: Bump) -> Result<String, String> {
    let mut parts = current.split('.');
    let parse = |part: Option<&str>, name: &str| {
        part.ok_or_else(|| format!("Version {current} is missing {name}."))?
            .parse::<u64>()
            .map_err(|_| format!("Version {current} is not numeric."))
    };
    let major = parse(parts.next(), "major")?;
    let minor = parse(parts.next(), "minor")?;
    let patch = parse(parts.next(), "patch")?;
    if parts.next().is_some() {
        return Err(format!("Version {current} is not major.minor.patch."));
    }
    let bumped = match bump {
        Bump::Major => (major.checked_add(1), Some(0), Some(0)),
        Bump::Minor => (Some(major), minor.checked_add(1), Some(0)),
        Bump::Patch => (Some(major), Some(minor), patch.checked_add(1)),
    };
    match bumped {
        (Some(major), Some(minor), Some(patch)) => Ok(format!("{major}.{minor}.{patch}")),
        _ => Err(format!("Version {current} cannot be bumped.")),
    }
}

fn current_version(cargo_toml: &str) -> Result<String, String> {
    PACKAGE_VERSION
        .captures(cargo_toml)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| "apps/powerkit/Cargo.toml has no stable package version.".into())
}

fn update_cargo_toml(text: &str, version: &str) -> Result<String, String> {
    let mut seen = false;
    let updated = PACKAGE_VERSION.replacen(text, 1, |_: &regex::Captures| {
        seen = true;
        format!(r#"version = "{version}""#)
    });
    if !seen {
        return Err("apps/powerkit/Cargo.toml has no stable package version.".into());
    }
    Ok(updated.into_owned())
}

fn split_lines(text: &str) -> (Vec<&str>, &str, bool) {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let trailing = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split(newline).collect();
    if trailing && lines.last() == Some(&"") {
        lines.pop();
    }
    (lines, newline, trailing)
}

fn join_lines(lines: &[String], newline: &str, trailing: bool) -> String {
    let mut text = lines.join(newline);
    if trailing {
        text.push_str(newline);
    }
    text
}

fn update_lock(text: &str, version: &str) -> Result<String, String> {
    let (lines, newline, trailing) = split_lines(text);
    let mut in_powerkit = false;
    let mut updated = 0;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if line == "[[package]]" {
            in_powerkit = false;
            out.push(line.to_string());
        } else if line == "name = \"powerkit\"" {
            in_powerkit = true;
            out.push(line.to_string());
        } else if in_powerkit && line.starts_with("version = \"") && line.ends_with('"') {
            out.push(format!(r#"version = "{version}""#));
            updated += 1;
            in_powerkit = false;
        } else {
            out.push(line.to_string());
        }
    }
    if updated == 0 {
        return Err("Cargo.lock has no powerkit package version.".into());
    }
    Ok(join_lines(&out, newline, trailing))
}

fn update_manifest(text: &str, version: &str) -> Result<String, String> {
    let (lines, newline, trailing) = split_lines(text);
    let mut found = false;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if line.contains("name=\"PowerKit\"") {
            if !MANIFEST_VERSION.is_match(line) {
                return Err("PowerKit assembly identity has no four-part version.".into());
            }
            found = true;
            out.push(
                MANIFEST_VERSION
                    .replace(line, format!(r#"version="{version}.0""#))
                    .into_owned(),
            );
        } else {
            out.push(line.to_string());
        }
    }
    if !found {
        return Err("app.manifest has no PowerKit assembly identity.".into());
    }
    Ok(join_lines(&out, newline, trailing))
}

fn apply_bump(root: &Path, bump: Bump) -> Result<String, String> {
    let cargo_path = root.join(CARGO_TOML);
    let lock_path = root.join("Cargo.lock");
    let manifest_path = root.join(APP_MANIFEST);
    let cargo = read_text(&cargo_path)?;
    let version = next_version(&current_version(&cargo)?, bump)?;
    // Every file is read and rewritten in memory first, so a file that
    // cannot be updated stops the bump before anything is written.
    let cargo = update_cargo_toml(&cargo, &version)?;
    let lock = update_lock(&read_text(&lock_path)?, &version)?;
    let manifest = update_manifest(&read_text(&manifest_path)?, &version)?;
    fs::write(&cargo_path, cargo)
        .map_err(|error| format!("Could not write {}: {error}", cargo_path.display()))?;
    fs::write(&lock_path, lock)
        .map_err(|error| format!("Could not write {}: {error}", lock_path.display()))?;
    fs::write(&manifest_path, manifest)
        .map_err(|error| format!("Could not write {}: {error}", manifest_path.display()))?;
    Ok(version)
}

fn repo_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| format!("Could not run git: {error}"))?;
    if !output.status.success() {
        return Err("Could not find the repository root.".into());
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn run() -> Result<String, String> {
    let mut args = std::env::args().skip(1);
    let bump = match (args.next(), args.next()) {
        (Some(level), None) => Bump::parse(&level),
        _ => None,
    }
    .ok_or("Usage: bump-version <major|minor|patch>")?;
    apply_bump(&repo_root()?, bump)
}

fn main() {
    match run() {
        Ok(version) => println!("PowerKit version is now {version}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
fn fixture(version: &str) -> (tempfile_dir::Dir, PathBuf) {
    let dir = tempfile_dir::Dir::new();
    let root = dir.path.clone();
    fs::create_dir_all(root.join("apps/powerkit")).unwrap();
    fs::write(
        root.join(CARGO_TOML),
        format!("[package]\nname = \"powerkit\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
    fs::write(
        root.join("Cargo.lock"),
        format!(
            "[[package]]\nname = \"other\"\nversion = \"1.0.0\"\n\n[[package]]\nname = \"powerkit\"\nversion = \"{version}\"\n"
        ),
    )
    .unwrap();
    fs::write(
        root.join(APP_MANIFEST),
        format!(
            "<assemblyIdentity version=\"{version}.0\" name=\"PowerKit\" type=\"win32\"/>\n<assemblyIdentity version=\"6.0.0.0\" name=\"Microsoft.Windows.Common-Controls\"/>\n"
        ),
    )
    .unwrap();
    (dir, root)
}

#[cfg(test)]
mod tempfile_dir {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub struct Dir {
        pub path: PathBuf,
    }

    impl Dir {
        pub fn new() -> Self {
            // Tests run in parallel and the clock is too coarse to tell two
            // of them apart, so each folder takes the next number.
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "powerkit-bump-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_levels_are_accepted() {
        assert_eq!(Bump::parse("major"), Some(Bump::Major));
        assert_eq!(Bump::parse("minor"), Some(Bump::Minor));
        assert_eq!(Bump::parse("patch"), Some(Bump::Patch));
        assert_eq!(Bump::parse("Minor"), None);
        assert_eq!(Bump::parse("1.2.3"), None);
    }

    #[test]
    fn version_components_roll_over() {
        assert_eq!(next_version("0.2.0", Bump::Patch).unwrap(), "0.2.1");
        assert_eq!(next_version("0.2.9", Bump::Patch).unwrap(), "0.2.10");
        assert_eq!(next_version("0.2.0", Bump::Minor).unwrap(), "0.3.0");
        assert_eq!(next_version("0.2.0", Bump::Major).unwrap(), "1.0.0");
        assert_eq!(next_version("1.4.5", Bump::Major).unwrap(), "2.0.0");
        assert!(next_version("1.4", Bump::Patch).is_err());
        assert!(next_version("1.4.5-rc1", Bump::Patch).is_err());
    }

    #[test]
    fn bump_updates_version_files_and_leaves_unrelated_versions() {
        let (_dir, root) = fixture("0.2.0");
        let version = apply_bump(&root, Bump::Minor).unwrap();
        assert_eq!(version, "0.3.0");
        let cargo = fs::read_to_string(root.join(CARGO_TOML)).unwrap();
        assert!(cargo.contains("version = \"0.3.0\""));
        let lock = fs::read_to_string(root.join("Cargo.lock")).unwrap();
        assert!(lock.contains("name = \"other\"\nversion = \"1.0.0\""));
        assert!(lock.contains("name = \"powerkit\"\nversion = \"0.3.0\""));
        let manifest = fs::read_to_string(root.join(APP_MANIFEST)).unwrap();
        assert!(manifest.contains("version=\"0.3.0.0\" name=\"PowerKit\""));
        assert!(
            manifest.contains("version=\"6.0.0.0\" name=\"Microsoft.Windows.Common-Controls\"")
        );
    }

    #[test]
    fn a_file_that_cannot_be_updated_stops_the_bump_before_any_write() {
        let (_dir, root) = fixture("0.2.0");
        fs::write(root.join(APP_MANIFEST), "<assembly/>\n").unwrap();
        assert!(apply_bump(&root, Bump::Patch).is_err());
        let cargo = fs::read_to_string(root.join(CARGO_TOML)).unwrap();
        assert!(cargo.contains("version = \"0.2.0\""));
    }
}
