//! Fast, offline repository contracts.
use regex::Regex;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

const REQUIRED: &[&str] = &[
    "LICENSE",
    "README.md",
    "CONTRIBUTING.md",
    "CODE_OF_CONDUCT.md",
    "SECURITY.md",
    "SUPPORT.md",
    "CHANGELOG.md",
    "AGENTS.md",
    "rust-toolchain.toml",
    ".editorconfig",
    ".github/CODEOWNERS",
    ".github/dependabot.yml",
    ".github/pull_request_template.md",
    "docs/architecture.md",
    "docs/development.md",
    "docs/testing.md",
    "docs/releasing.md",
    ".agents/skills/powerkit-development/SKILL.md",
    ".agents/skills/powerkit-release/SKILL.md",
    "brand/README.md",
    "brand/tokens/colors.json",
    "brand/windows/PowerKit.ico",
    "brand/windows/tray-light.ico",
    "brand/windows/tray-dark.ico",
    "brand/windows/tray-light-paused.ico",
    "brand/windows/tray-dark-paused.ico",
];

static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new("`[^`\n]*`").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]\n]*\]\(([^)\n]+)\)").unwrap());
static ACTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"uses:\s+([^\s#]+)").unwrap());
static PINNED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[^@]+@[0-9a-f]{40}$").unwrap());
static STABLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d+\.\d+$").unwrap());
static SKILL_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap());
static IDENTITY_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<assemblyIdentity\b[^>]*\bversion="([^"]+)""#).unwrap());

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scripts crate lives in the repository")
        .to_path_buf()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(value) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(value);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn normalize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                out.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
        }
    }
    out
}

fn has_scheme(target: &str) -> bool {
    let Some((scheme, rest)) = target.split_once(':') else {
        return false;
    };
    if scheme.is_empty() || rest.is_empty() {
        return false;
    }
    let mut chars = scheme.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn fence_marker(line: &str) -> Option<&str> {
    let line = line.trim_end_matches(['\r', '\n']);
    let first = line.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let count = line.chars().take_while(|c| *c == first).count();
    (count >= 3).then(|| &line[..count])
}

fn strip_fences(text: &str) -> String {
    let mut out = String::new();
    let mut fence: Option<String> = None;
    for line in text.split_inclusive('\n') {
        if let Some(open) = fence.as_deref() {
            if fence_marker(line) == Some(open)
                && line.trim_end_matches(['\r', '\n'])[open.len()..]
                    .trim()
                    .is_empty()
            {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            fence = Some(marker.to_string());
            continue;
        }
        out.push_str(line);
    }
    out
}

fn local_link_errors(root: &Path, source: &Path, text: &str) -> Vec<String> {
    let text = strip_fences(text);
    let text = INLINE_CODE.replace_all(&text, "");
    let root = normalize(root);
    let mut errors = Vec::new();
    for capture in LINK.captures_iter(&text) {
        let target = capture[1].trim().trim_matches(['<', '>']);
        if target.is_empty() || target.starts_with('#') || has_scheme(target) {
            continue;
        }
        let path = percent_decode(
            target
                .split_once(['#', '?'])
                .map_or(target, |(path, _)| path),
        );
        let resolved = normalize(&source.parent().unwrap_or(source).join(path));
        let relative = source
            .strip_prefix(&root)
            .unwrap_or(source)
            .display()
            .to_string()
            .replace('\\', "/");
        if !resolved.starts_with(&root) {
            errors.push(format!("{relative}: link escapes repository: {target}"));
        } else if !resolved.exists() {
            errors.push(format!("{relative}: broken link: {target}"));
        }
    }
    errors
}

fn skill_errors(root: &Path) -> Vec<String> {
    let skills = root.join(".agents/skills");
    let entries = match fs::read_dir(&skills) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let skill = entry.path().join("SKILL.md");
        if skill.is_file() {
            paths.push(skill);
        }
    }
    paths.sort();
    let mut errors = Vec::new();
    for skill in paths {
        let text = fs::read_to_string(&skill).unwrap_or_default();
        let mut parts = text.splitn(3, "---");
        let prefix = parts.next().unwrap_or("");
        let frontmatter = parts.next();
        if frontmatter.is_none() || !prefix.trim().is_empty() {
            errors.push(format!(
                "Missing skill frontmatter: {}",
                skill.file_name().unwrap().to_string_lossy()
            ));
            continue;
        }
        let fields: Vec<(String, String)> = frontmatter
            .unwrap()
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        let name = fields
            .iter()
            .find(|(key, _)| key == "name")
            .map(|(_, value)| value.trim())
            .unwrap_or("");
        let description = fields
            .iter()
            .find(|(key, _)| key == "description")
            .map(|(_, value)| value.trim())
            .unwrap_or("");
        let directory = skill.parent().and_then(|path| path.file_name()).unwrap();
        let directory = directory.to_string_lossy();
        if name != directory || !SKILL_NAME.is_match(name) {
            errors.push(format!("Skill name must match its directory: {directory}"));
        }
        if description.is_empty() {
            errors.push(format!("Missing skill description: {directory}"));
        }
    }
    errors
}

fn metadata_errors(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let cargo_text = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    let cargo: toml::Value = match toml::from_str(&cargo_text) {
        Ok(value) => value,
        Err(_) => return vec!["Cargo.toml is not valid TOML".into()],
    };
    let package = cargo.get("package");
    let version = package
        .and_then(|value| value.get("version"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if !STABLE.is_match(version) {
        errors.push("Cargo version must be numeric and stable".into());
    }
    let license = package
        .and_then(|value| value.get("license"))
        .and_then(|value| value.as_str());
    let publish = package
        .and_then(|value| value.get("publish"))
        .and_then(|value| value.as_bool());
    if license != Some("MIT") || publish != Some(false) {
        errors.push("Application must declare MIT and publish = false".into());
    }
    let manifest = fs::read_to_string(root.join("app.manifest")).unwrap_or_default();
    let identity = IDENTITY_VERSION
        .captures(&manifest)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str());
    if identity != Some(&format!("{version}.0")) {
        errors.push("app.manifest version must match Cargo.toml + .0".into());
    }
    let lock_text = fs::read_to_string(root.join("Cargo.lock")).unwrap_or_default();
    let lock: toml::Value = match toml::from_str(&lock_text) {
        Ok(value) => value,
        Err(_) => {
            errors.push("Cargo.lock is not valid TOML".into());
            return errors;
        }
    };
    let lock_matches = lock
        .get("package")
        .and_then(|value| value.as_array())
        .is_some_and(|packages| {
            packages.iter().any(|package| {
                package.get("name").and_then(|value| value.as_str()) == Some("powerkit")
                    && package.get("version").and_then(|value| value.as_str()) == Some(version)
            })
        });
    if !lock_matches {
        errors.push("Cargo.lock application version differs".into());
    }
    if !root
        .join("docs/releases")
        .join(format!("{version}.md"))
        .is_file()
    {
        errors.push(format!("Missing release notes for {version}"));
    }
    let toolchain_text = fs::read_to_string(root.join("rust-toolchain.toml")).unwrap_or_default();
    let toolchain: toml::Value =
        toml::from_str(&toolchain_text).unwrap_or(toml::Value::Table(toml::map::Map::new()));
    let channel = toolchain
        .get("toolchain")
        .and_then(|value| value.get("channel"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let rust_version = package
        .and_then(|value| value.get("rust-version"))
        .and_then(|value| value.as_str());
    if rust_version != Some(channel) {
        errors.push("Validated minimum Rust and pinned toolchain differ".into());
    }
    let workflow =
        fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap_or_default();
    if !workflow.contains(&format!("rustup toolchain install {channel} ")) {
        errors.push("Workflow Rust version differs".into());
    }
    for action in ACTION.captures_iter(&workflow) {
        let action = &action[1];
        if !action.starts_with("./") && !PINNED.is_match(action) {
            errors.push(format!("Action must be pinned to a commit SHA: {action}"));
        }
    }
    let sdk = cargo
        .get("target")
        .and_then(|value| value.get("cfg(windows)"))
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("velopack"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim_start_matches('=');
    if !workflow.contains(&format!("vpk --version {sdk} ")) {
        errors.push("Velopack CLI and SDK versions differ".into());
    }
    errors
}

fn main() {
    let errors = repository_errors(&root());
    for error in &errors {
        eprintln!("{error}");
    }
    if !errors.is_empty() {
        std::process::exit(1);
    }
    println!("Repository contracts and local Markdown links: PASS");
}

fn repository_errors(root: &Path) -> Vec<String> {
    let mut errors = REQUIRED
        .iter()
        .filter(|name| !root.join(name).is_file())
        .map(|name| format!("Missing {name}"))
        .collect::<Vec<_>>();
    errors.extend(metadata_errors(root));
    errors.extend(skill_errors(root));
    let listed = Command::new("git")
        .args([
            format!("--git-dir={}", root.join(".git").display()),
            format!("--work-tree={}", root.display()),
            "ls-files".into(),
            "--cached".into(),
            "--others".into(),
            "--exclude-standard".into(),
            "-z".into(),
        ])
        .current_dir(root)
        .output();
    let names = match listed {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>(),
        _ => {
            errors.push("git ls-files failed".into());
            return errors;
        }
    };
    let mut names = names;
    names.sort();
    names.dedup();
    for name in names {
        let source = root.join(&name);
        if name.ends_with(".md")
            && source.is_file()
            && let Ok(text) = fs::read_to_string(&source)
        {
            errors.extend(local_link_errors(root, &source, &text));
        }
    }
    errors
}

#[test]
fn repository_contracts_and_local_markdown_links() {
    let errors = repository_errors(&root());
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[cfg(test)]
struct Scratch {
    path: PathBuf,
}

#[cfg(test)]
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "powerkit-repo-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(path.join("docs")).unwrap();
        Self { path }
    }
}

#[cfg(test)]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn relative_anchor_and_external_links() {
    let scratch = Scratch::new();
    fs::write(scratch.path.join("README.md"), "# Example").unwrap();
    let source = scratch.path.join("docs/guide.md");
    let errors = local_link_errors(
        &scratch.path,
        &source,
        "[root](../README.md#example) [web](https://example.org/a)",
    );
    assert_eq!(errors, [] as [String; 0]);
}

#[test]
fn missing_target_and_escape_are_rejected() {
    let scratch = Scratch::new();
    fs::write(scratch.path.join("README.md"), "# Example").unwrap();
    let source = scratch.path.join("docs/guide.md");
    let errors = local_link_errors(
        &scratch.path,
        &source,
        "[bad](missing.md) [escape](../../outside.md)",
    );
    assert_eq!(errors.len(), 2);
}

#[test]
fn examples_are_not_treated_as_links() {
    let scratch = Scratch::new();
    let source = scratch.path.join("docs/guide.md");
    let errors = local_link_errors(
        &scratch.path,
        &source,
        "~~~md\n[example](missing.md)\n~~~\n",
    );
    assert_eq!(errors, [] as [String; 0]);
}

#[test]
fn percent_encoded_spaces() {
    let scratch = Scratch::new();
    fs::write(scratch.path.join("docs/my guide.md"), "text").unwrap();
    let source = scratch.path.join("docs/guide.md");
    let errors = local_link_errors(&scratch.path, &source, "[space](my%20guide.md)");
    assert_eq!(errors, [] as [String; 0]);
}

#[cfg(test)]
fn copy_metadata_fixture(scratch: &Path) -> String {
    let source = root();
    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(source.join("Cargo.toml")).unwrap()).unwrap();
    let version = cargo["package"]["version"].as_str().unwrap().to_string();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "app.manifest",
        "rust-toolchain.toml",
        ".github/workflows/release.yml",
    ] {
        let target = scratch.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(name), target).unwrap();
    }
    let notes = scratch.join("docs/releases").join(format!("{version}.md"));
    fs::create_dir_all(notes.parent().unwrap()).unwrap();
    fs::copy(
        source.join("docs/releases").join(format!("{version}.md")),
        notes,
    )
    .unwrap();
    version
}

#[test]
fn current_metadata_matches() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    assert_eq!(metadata_errors(&scratch.path), [] as [String; 0]);
}

#[test]
fn manifest_version_drift_is_rejected() {
    let scratch = Scratch::new();
    let version = copy_metadata_fixture(&scratch.path);
    let path = scratch.path.join("app.manifest");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace(&format!("{version}.0"), "9.9.9.0");
    fs::write(&path, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("app.manifest"))
    );
}

#[test]
fn mutable_action_reference_is_rejected() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    let path = scratch.path.join(".github/workflows/release.yml");
    let workflow = fs::read_to_string(&path).unwrap();
    let text = Regex::new(r"actions/checkout@[0-9a-f]{40}")
        .unwrap()
        .replace(&workflow, "actions/checkout@v6");
    fs::write(&path, text.as_ref()).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("commit SHA"))
    );
}

#[test]
fn sdk_cli_drift_is_rejected() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    let path = scratch.path.join(".github/workflows/release.yml");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("vpk --version ", "vpk --version 9.9.9-");
    fs::write(&path, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("Velopack"))
    );
}

#[test]
fn missing_description_prevents_silent_discovery_failure() {
    let scratch = Scratch::new();
    let skill = scratch.path.join(".agents/skills/example/SKILL.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(&skill, "---\nname: example\n---\nBody").unwrap();
    assert_eq!(
        skill_errors(&scratch.path),
        ["Missing skill description: example"]
    );
}
