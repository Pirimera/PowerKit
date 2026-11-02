//! Bump the stable PowerKit version from a commit message.
use regex::Regex;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

const VERSION_PATHS: &[&str] = &["Cargo.toml", "Cargo.lock", "app.manifest", "CHANGELOG.md"];

static PACKAGE_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^version = "(\d+\.\d+\.\d+)""#).unwrap());
static MANIFEST_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"version="\d+\.\d+\.\d+\.\d+""#).unwrap());
static CONVENTIONAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?P<type>[A-Za-z][A-Za-z0-9]*)(?:\([^)\r\n]*\))?(?P<bang>!)?:[ \t]*(?P<summary>.*)$",
    )
    .unwrap()
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bump {
    Major,
    Minor,
    Patch,
}

struct ParsedCommit {
    bump: Bump,
    summary: String,
}

fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next();
        }
    }
    None
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("Could not read {}: {error}", path.display()))
}

fn meaningful_lines(message: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for line in message.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("# ------------------------ >8") {
            break;
        }
        if line.starts_with('#') {
            continue;
        }
        lines.push(line.to_string());
    }
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

fn parse_commit(message: &str) -> Result<ParsedCommit, String> {
    let lines = meaningful_lines(message);
    let subject = lines
        .first()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .ok_or("Commit message is empty.")?;
    let breaking = lines.iter().any(|line| {
        let line = line.trim();
        line.starts_with("BREAKING CHANGE:") || line.starts_with("BREAKING-CHANGE:")
    });
    if let Some(capture) = CONVENTIONAL.captures(subject) {
        let summary = capture
            .name("summary")
            .map(|value| value.as_str().trim())
            .unwrap_or("");
        if summary.is_empty() {
            return Err("Commit summary is empty.".into());
        }
        let feat = capture
            .name("type")
            .is_some_and(|value| value.as_str().eq_ignore_ascii_case("feat"));
        let bump = if capture.name("bang").is_some() || breaking {
            Bump::Major
        } else if feat {
            Bump::Minor
        } else {
            Bump::Patch
        };
        return Ok(ParsedCommit {
            bump,
            summary: summary.to_string(),
        });
    }
    Ok(ParsedCommit {
        bump: if breaking { Bump::Major } else { Bump::Patch },
        summary: subject.to_string(),
    })
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
        .ok_or_else(|| "Cargo.toml has no stable package version.".into())
}

fn update_cargo_toml(text: &str, version: &str) -> Result<String, String> {
    let mut seen = false;
    let updated = PACKAGE_VERSION.replacen(text, 1, |_: &regex::Captures| {
        seen = true;
        format!(r#"version = "{version}""#)
    });
    if !seen {
        return Err("Cargo.toml has no stable package version.".into());
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

fn insert_changelog(text: &str, version: &str, summary: &str) -> String {
    let block = format!("## {version}\n\n- {summary}\n");
    let Some(unreleased) = text.find("## Unreleased") else {
        return append_block(text, &block);
    };
    let search_from = unreleased + "## Unreleased".len();
    let Some(relative) = text[search_from..].find("\n## ") else {
        return append_block(text, &block);
    };
    let at = search_from + relative;
    let mut out = String::with_capacity(text.len() + block.len() + 2);
    out.push_str(&text[..at]);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&block);
    out.push('\n');
    out.push_str(&text[at + 1..]);
    out
}

fn append_block(text: &str, block: &str) -> String {
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(block);
    out
}

fn release_notes(version: &str, summary: &str) -> String {
    format!("# PowerKit {version}\n\n- {summary}\n")
}

fn apply_bump(root: &Path, message: &str) -> Result<String, String> {
    let parsed = parse_commit(message)?;
    let cargo_path = root.join("Cargo.toml");
    let lock_path = root.join("Cargo.lock");
    let manifest_path = root.join("app.manifest");
    let changelog_path = root.join("CHANGELOG.md");
    let cargo = read_text(&cargo_path)?;
    let version = next_version(&current_version(&cargo)?, parsed.bump)?;
    let notes_path = root.join("docs/releases").join(format!("{version}.md"));
    if notes_path.exists() {
        return Err(format!("Release notes already exist for {version}."));
    }
    let cargo = update_cargo_toml(&cargo, &version)?;
    let lock = update_lock(&read_text(&lock_path)?, &version)?;
    let manifest = update_manifest(&read_text(&manifest_path)?, &version)?;
    let changelog = insert_changelog(&read_text(&changelog_path)?, &version, &parsed.summary);
    let notes = release_notes(&version, &parsed.summary);
    if let Some(parent) = notes_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    fs::write(&cargo_path, cargo)
        .map_err(|error| format!("Could not write {}: {error}", cargo_path.display()))?;
    fs::write(&lock_path, lock)
        .map_err(|error| format!("Could not write {}: {error}", lock_path.display()))?;
    fs::write(&manifest_path, manifest)
        .map_err(|error| format!("Could not write {}: {error}", manifest_path.display()))?;
    fs::write(&changelog_path, changelog)
        .map_err(|error| format!("Could not write {}: {error}", changelog_path.display()))?;
    let mut notes_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&notes_path)
        .map_err(|error| format!("Could not create {}: {error}", notes_path.display()))?;
    notes_file
        .write_all(notes.as_bytes())
        .map_err(|error| format!("Could not write {}: {error}", notes_path.display()))?;
    Ok(version)
}

fn git(repo: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("Could not run git: {error}"))
}

fn ensure_clean(repo: &Path) -> Result<(), String> {
    let mut args = vec!["diff", "--quiet", "--"];
    args.extend(VERSION_PATHS);
    let output = git(repo, &args)?;
    match output.status.code() {
        Some(0) => Ok(()),
        Some(1) => {
            Err("Version files have unstaged edits. Stage or stash them before committing.".into())
        }
        _ => Err(format!(
            "Could not inspect version files: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

fn stage(repo: &Path, version: &str) -> Result<(), String> {
    let notes = format!("docs/releases/{version}.md");
    let mut args = vec!["add", "--"];
    args.extend(VERSION_PATHS);
    args.push(&notes);
    let output = git(repo, &args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not stage version files: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn repo_root() -> Result<PathBuf, String> {
    if let Some(path) = arg_value("--repo") {
        return Ok(PathBuf::from(path));
    }
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

fn main() {
    let message_file = arg_value("--message-file").unwrap_or_else(|| {
        eprintln!("--message-file is required.");
        std::process::exit(1);
    });
    let message = match fs::read_to_string(&message_file) {
        Ok(message) => message,
        Err(error) => {
            eprintln!("Could not read {message_file}: {error}");
            std::process::exit(1);
        }
    };
    let root = match repo_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = ensure_clean(&root) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    let version = match apply_bump(&root, &message) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = stage(&root, &version) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    eprintln!("PowerKit version is now {version}");
}

#[cfg(test)]
fn fixture(version: &str) -> (tempfile_dir::Dir, PathBuf) {
    let dir = tempfile_dir::Dir::new();
    let root = dir.path.clone();
    fs::create_dir_all(root.join("docs/releases")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
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
        root.join("app.manifest"),
        format!(
            "<assemblyIdentity version=\"{version}.0\" name=\"PowerKit\" type=\"win32\"/>\n<assemblyIdentity version=\"6.0.0.0\" name=\"Microsoft.Windows.Common-Controls\"/>\n"
        ),
    )
    .unwrap();
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nIntro.\n\n## Unreleased\n\n- pending\n\n## 0.1.0\n\n- old\n",
    )
    .unwrap();
    (dir, root)
}

#[cfg(test)]
mod tempfile_dir {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub struct Dir {
        pub path: PathBuf,
    }

    impl Dir {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "powerkit-bump-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
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
    fn conventional_messages_select_the_semver_component() {
        let parsed = parse_commit("feat(tray): pin the window\n\nBody.\n").unwrap();
        assert_eq!(parsed.bump, Bump::Minor);
        assert_eq!(parsed.summary, "pin the window");
        assert_eq!(
            parse_commit("fix: keep the session alive\n").unwrap().bump,
            Bump::Patch
        );
        assert_eq!(
            parse_commit("chore: refresh the notes\n").unwrap().bump,
            Bump::Patch
        );
        assert_eq!(
            parse_commit("Document the hook.\n").unwrap().bump,
            Bump::Patch
        );
        let breaking = parse_commit("feat!: change the tray contract\n").unwrap();
        assert_eq!(breaking.bump, Bump::Major);
        assert_eq!(breaking.summary, "change the tray contract");
        let footer =
            parse_commit("fix: remove the flag\n\nBREAKING CHANGE: old flag is gone\n").unwrap();
        assert_eq!(footer.bump, Bump::Major);
        assert_eq!(footer.summary, "remove the flag");
    }

    #[test]
    fn comments_and_the_scissors_section_are_ignored() {
        let message = "# Please enter the commit message\n\nfeat: add a ruler\n\n# ------------------------ >8 ------------------------\nBREAKING CHANGE: not part of the message\n";
        let parsed = parse_commit(message).unwrap();
        assert_eq!(parsed.bump, Bump::Minor);
        assert_eq!(parsed.summary, "add a ruler");
    }

    #[test]
    fn empty_summary_is_rejected() {
        assert!(parse_commit("feat:\n").is_err());
        assert!(parse_commit("# only a comment\n").is_err());
    }

    #[test]
    fn version_components_roll_over() {
        assert_eq!(next_version("0.2.0", Bump::Patch).unwrap(), "0.2.1");
        assert_eq!(next_version("0.2.9", Bump::Patch).unwrap(), "0.2.10");
        assert_eq!(next_version("0.2.0", Bump::Minor).unwrap(), "0.3.0");
        assert_eq!(next_version("0.2.0", Bump::Major).unwrap(), "1.0.0");
        assert_eq!(next_version("1.4.5", Bump::Major).unwrap(), "2.0.0");
    }

    #[test]
    fn bump_updates_version_files_and_leaves_unrelated_versions() {
        let (_dir, root) = fixture("0.2.0");
        let version = apply_bump(&root, "feat: pin the last window\n").unwrap();
        assert_eq!(version, "0.3.0");
        let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("version = \"0.3.0\""));
        let lock = fs::read_to_string(root.join("Cargo.lock")).unwrap();
        assert!(lock.contains("name = \"other\"\nversion = \"1.0.0\""));
        assert!(lock.contains("name = \"powerkit\"\nversion = \"0.3.0\""));
        let manifest = fs::read_to_string(root.join("app.manifest")).unwrap();
        assert!(manifest.contains("version=\"0.3.0.0\" name=\"PowerKit\""));
        assert!(
            manifest.contains("version=\"6.0.0.0\" name=\"Microsoft.Windows.Common-Controls\"")
        );
        let changelog = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
        assert!(changelog.contains(
            "## Unreleased\n\n- pending\n\n## 0.3.0\n\n- pin the last window\n\n## 0.1.0\n"
        ));
        let notes = fs::read_to_string(root.join("docs/releases/0.3.0.md")).unwrap();
        assert_eq!(notes, "# PowerKit 0.3.0\n\n- pin the last window\n");
    }

    #[test]
    fn existing_release_notes_are_not_replaced() {
        let (_dir, root) = fixture("0.2.0");
        fs::write(root.join("docs/releases/0.2.1.md"), "keep\n").unwrap();
        let error = apply_bump(&root, "fix: a small correction\n").unwrap_err();
        assert!(error.contains("0.2.1"));
        let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("version = \"0.2.0\""));
    }

    #[test]
    fn unstaged_version_edits_block_the_bump() {
        let (_dir, root) = fixture("0.2.0");
        assert!(git(&root, &["init"]).unwrap().status.success());
        let add = git(
            &root,
            &[
                "add",
                "--",
                "Cargo.toml",
                "Cargo.lock",
                "app.manifest",
                "CHANGELOG.md",
            ],
        );
        assert!(add.unwrap().status.success());
        assert!(ensure_clean(&root).is_ok());
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"powerkit\"\nversion = \"9.9.9\"\n",
        )
        .unwrap();
        let error = ensure_clean(&root).unwrap_err();
        assert!(error.contains("unstaged"));
    }
}
