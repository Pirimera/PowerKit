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
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
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

/// The user guide: an introduction and one page per tool, in English under
/// `docs/user` and with the same file names in Turkish under `docs/user/tr`.
const USER_GUIDE: &str = "docs/user";
const USER_GUIDE_TRANSLATIONS: &[&str] = &["tr"];
const USER_GUIDE_PAGES: &[&str] = &[
    "README.md",
    "always-on.md",
    "always-top.md",
    "screen-ruler.md",
    "image-resizer.md",
    "media-renamer.md",
    "power-store.md",
    "power-mouse.md",
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

/// The application package: its version is the product version.
const APP_CARGO_TOML: &str = "apps/powerkit/Cargo.toml";
const APP_MANIFEST: &str = "apps/powerkit/app.manifest";
/// Runs on pull requests and pushes to main: checks only.
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
/// Builds the packages; started by hand.
const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";

fn metadata_errors(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let cargo_text = fs::read_to_string(root.join(APP_CARGO_TOML)).unwrap_or_default();
    let cargo: toml::Value = match toml::from_str(&cargo_text) {
        Ok(value) => value,
        Err(_) => return vec![format!("{APP_CARGO_TOML} is not valid TOML")],
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
    let manifest = fs::read_to_string(root.join(APP_MANIFEST)).unwrap_or_default();
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
    let ci = fs::read_to_string(root.join(CI_WORKFLOW)).unwrap_or_default();
    let release = fs::read_to_string(root.join(RELEASE_WORKFLOW)).unwrap_or_default();
    for (name, workflow) in [(CI_WORKFLOW, &ci), (RELEASE_WORKFLOW, &release)] {
        if !workflow.contains(&format!("rustup toolchain install {channel} ")) {
            errors.push(format!("{name}: Rust version differs"));
        }
        for action in ACTION.captures_iter(workflow) {
            let action = &action[1];
            if !action.starts_with("./") && !PINNED.is_match(action) {
                errors.push(format!(
                    "{name}: action must be pinned to a commit SHA: {action}"
                ));
            }
        }
        // Neither workflow publishes; the Store submission is not automated.
        if workflow.contains("gh release") || !workflow.contains("permissions:\n  contents: read\n")
        {
            errors.push(format!(
                "{name}: workflow must be read-only and must not publish GitHub Releases"
            ));
        }
    }
    let appx = fs::read_to_string(root.join("packaging/msix/AppxManifest.xml")).unwrap_or_default();
    for required in [
        r#"<Identity Name="PiriMedya.PowerKit""#,
        r#"Version="{{VERSION}}""#,
        r#"<PackageDependency Name="Microsoft.WindowsAppRuntime.2" MinVersion="2.4.0.0""#,
        r#"TaskId="PowerKitStartup""#,
    ] {
        if !appx.contains(required) {
            errors.push(format!("MSIX manifest must contain {required}"));
        }
    }
    errors.extend(layering_errors(root));
    // Pull requests run code from forks, so the workflow they start gets no
    // secret, no write access and no packaging step.
    if !ci.contains("\n  pull_request:\n")
        || ci.contains("pull_request_target")
        || ci.contains("secrets.")
        || ci.contains("Package.ps1")
    {
        errors.push(format!(
            "{CI_WORKFLOW}: must run on pull requests with no secret and no packaging"
        ));
    }
    if !release.contains("workflow_dispatch:")
        || release.contains("\n  push:")
        || release.contains("\n  pull_request")
        || release.contains("github.event_name == 'push'")
    {
        errors.push(format!(
            "{RELEASE_WORKFLOW}: packaging must be started manually"
        ));
    }
    errors
}

/// The layers below the platform must stay buildable on every platform.
/// `powerkit-core` may not depend on anything and `powerkit-app` only on
/// `powerkit-core`: an operating system or UI crate would break that, and a
/// build script could reach for one.
fn layering_errors(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    for (directory, name, allowed) in [
        ("crates/core", "powerkit-core", &[][..]),
        ("crates/app", "powerkit-app", &["powerkit-core"][..]),
    ] {
        let path = format!("{directory}/Cargo.toml");
        let text = fs::read_to_string(root.join(&path)).unwrap_or_default();
        let manifest: toml::Value = match toml::from_str(&text) {
            Ok(value) => value,
            Err(_) => {
                errors.push(format!("{path} is not valid TOML"));
                continue;
            }
        };
        let package = manifest.get("package");
        if package
            .and_then(|package| package.get("name"))
            .and_then(|name| name.as_str())
            != Some(name)
        {
            errors.push(format!("{directory} must be the {name} package"));
        }
        for table in ["build-dependencies", "target"] {
            if manifest.get(table).is_some() {
                errors.push(format!(
                    "{name} must not declare [{table}]: it has no OS or UI dependency"
                ));
            }
        }
        let dependencies = manifest
            .get("dependencies")
            .and_then(|table| table.as_table());
        for dependency in dependencies.into_iter().flat_map(|table| table.keys()) {
            if !allowed.contains(&dependency.as_str()) {
                errors.push(format!(
                    "{name} must not depend on {dependency}: it has no OS or UI dependency"
                ));
            }
        }
        if package.and_then(|package| package.get("build")).is_some()
            || root.join(directory).join("build.rs").exists()
        {
            errors.push(format!("{name} must not have a build script"));
        }
    }
    errors.extend(ui_layering_errors(root));
    errors
}

/// Crates beside the application may name only the workspace crates listed
/// for them. A UI crate and a platform crate see the application through
/// `powerkit-app` and never each other, so another platform can host the same
/// UI and another UI can be hosted by the same platform; only the application
/// names both. The WinGet worker must stay free of the UI crate. The Explorer DLL is loaded
/// into other processes and needs the domain constants only. Both may use
/// operating system and UI kit crates; the workspace crates are the check.
fn ui_layering_errors(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    for (directory, name, allowed) in [
        (
            "crates/ui/reactor",
            "powerkit-ui-reactor",
            &["powerkit-core", "powerkit-app"][..],
        ),
        (
            "apps/shell-ext",
            "powerkit-shell-ext",
            &["powerkit-core"][..],
        ),
        (
            "crates/platform/windows",
            "powerkit-platform-windows",
            &["powerkit-core", "powerkit-app"][..],
        ),
        (
            "apps/winget-worker",
            "powerkit-winget-worker",
            &["powerkit-platform-windows"][..],
        ),
    ] {
        let path = format!("{directory}/Cargo.toml");
        let text = fs::read_to_string(root.join(&path)).unwrap_or_default();
        let manifest: toml::Value = match toml::from_str(&text) {
            Ok(value) => value,
            Err(_) => {
                errors.push(format!("{path} is not valid TOML"));
                continue;
            }
        };
        if manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(|name| name.as_str())
            != Some(name)
        {
            errors.push(format!("{directory} must be the {name} package"));
        }
        let mut tables: Vec<&toml::Value> = manifest.get("dependencies").into_iter().collect();
        let targets = manifest.get("target").and_then(|target| target.as_table());
        for target in targets.into_iter().flat_map(|table| table.values()) {
            tables.extend(target.get("dependencies"));
        }
        for table in tables.into_iter().filter_map(|table| table.as_table()) {
            for (dependency, spec) in table {
                if spec.get("path").is_some() && !allowed.contains(&dependency.as_str()) {
                    errors.push(format!(
                        "{name} must not depend on {dependency}: it may name only {}",
                        allowed.join(" and ")
                    ));
                }
            }
        }
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

/// Every user guide page exists in every language, and no language has a page
/// the others lack, so a translation cannot fall behind unnoticed.
fn user_guide_errors(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let languages = std::iter::once(String::from(USER_GUIDE)).chain(
        USER_GUIDE_TRANSLATIONS
            .iter()
            .map(|language| format!("{USER_GUIDE}/{language}")),
    );
    for folder in languages {
        for page in USER_GUIDE_PAGES {
            if !root.join(&folder).join(page).is_file() {
                errors.push(format!("Missing user guide page {folder}/{page}"));
            }
        }
        let Ok(entries) = fs::read_dir(root.join(&folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".md") && !USER_GUIDE_PAGES.contains(&name.as_str()) {
                errors.push(format!(
                    "User guide page {folder}/{name} is not in USER_GUIDE_PAGES"
                ));
            }
        }
    }
    errors
}

fn repository_errors(root: &Path) -> Vec<String> {
    let mut errors = REQUIRED
        .iter()
        .filter(|name| !root.join(name).is_file())
        .map(|name| format!("Missing {name}"))
        .collect::<Vec<_>>();
    errors.extend(metadata_errors(root));
    errors.extend(skill_errors(root));
    errors.extend(user_guide_errors(root));
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
        // Tests run in parallel and the clock is too coarse to tell two of
        // them apart, so each folder takes the next number of this process.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "powerkit-repo-check-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
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
fn a_user_guide_page_missing_in_one_language_is_reported() {
    let scratch = Scratch::new();
    for folder in ["docs/user", "docs/user/tr"] {
        fs::create_dir_all(scratch.path.join(folder)).unwrap();
        for page in USER_GUIDE_PAGES {
            fs::write(scratch.path.join(folder).join(page), "# Page").unwrap();
        }
    }
    assert_eq!(user_guide_errors(&scratch.path), [] as [String; 0]);
    fs::remove_file(scratch.path.join("docs/user/tr/power-mouse.md")).unwrap();
    fs::write(scratch.path.join("docs/user/new-tool.md"), "# Page").unwrap();
    assert_eq!(
        user_guide_errors(&scratch.path),
        [
            "User guide page docs/user/new-tool.md is not in USER_GUIDE_PAGES",
            "Missing user guide page docs/user/tr/power-mouse.md",
        ]
    );
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
        toml::from_str(&fs::read_to_string(source.join(APP_CARGO_TOML)).unwrap()).unwrap();
    let version = cargo["package"]["version"].as_str().unwrap().to_string();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        APP_CARGO_TOML,
        "crates/core/Cargo.toml",
        "crates/app/Cargo.toml",
        "crates/ui/reactor/Cargo.toml",
        "apps/shell-ext/Cargo.toml",
        "apps/winget-worker/Cargo.toml",
        "crates/platform/windows/Cargo.toml",
        APP_MANIFEST,
        "rust-toolchain.toml",
        CI_WORKFLOW,
        RELEASE_WORKFLOW,
        "packaging/msix/AppxManifest.xml",
    ] {
        let target = scratch.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(name), target).unwrap();
    }
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
    let path = scratch.path.join(APP_MANIFEST);
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
fn an_os_dependency_below_the_platform_layer_is_rejected() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    let core = scratch.path.join("crates/core/Cargo.toml");
    let mut text = fs::read_to_string(&core).unwrap();
    text.push_str("\n[target.'cfg(windows)'.dependencies]\nwindows-sys = \"0.61\"\n");
    fs::write(&core, text).unwrap();
    let app = scratch.path.join("crates/app/Cargo.toml");
    let text = fs::read_to_string(&app).unwrap().replace(
        "[dependencies]\n",
        "[dependencies]\nwindows-sys = \"0.61\"\n",
    );
    fs::write(&app, text).unwrap();
    let errors = metadata_errors(&scratch.path);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("powerkit-core must not declare [target]"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("powerkit-app must not depend on windows-sys"))
    );
    // Its one allowed dependency is not reported.
    assert!(
        !errors
            .iter()
            .any(|error| error.contains("on powerkit-core"))
    );
}

#[test]
fn a_ui_crate_that_names_the_platform_crate_is_rejected() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    assert!(
        !metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("powerkit-ui-reactor"))
    );
    let ui = scratch.path.join("crates/ui/reactor/Cargo.toml");
    let mut text = fs::read_to_string(&ui).unwrap();
    text.push_str("powerkit = { path = \"../../..\" }\n");
    fs::write(&ui, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("powerkit-ui-reactor must not depend on powerkit:"))
    );
    // The platform crate and the UI crate never see each other.
    let platform = scratch.path.join("crates/platform/windows/Cargo.toml");
    let text = fs::read_to_string(&platform).unwrap().replace(
        "[dependencies]\n",
        "[dependencies]\npowerkit-ui-reactor = { path = \"../../ui/reactor\" }\n",
    );
    fs::write(&platform, text).unwrap();
    assert!(metadata_errors(&scratch.path).iter().any(|error| {
        error.contains("powerkit-platform-windows must not depend on powerkit-ui-reactor")
    }));
}

#[test]
fn mutable_action_reference_is_rejected() {
    let checkout = Regex::new(r"actions/checkout@[0-9a-f]{40}").unwrap();
    for name in [CI_WORKFLOW, RELEASE_WORKFLOW] {
        let scratch = Scratch::new();
        copy_metadata_fixture(&scratch.path);
        let path = scratch.path.join(name);
        let workflow = fs::read_to_string(&path).unwrap();
        let text = checkout.replace(&workflow, "actions/checkout@v6");
        fs::write(&path, text.as_ref()).unwrap();
        assert!(
            metadata_errors(&scratch.path)
                .iter()
                .any(|error| error.starts_with(name) && error.contains("commit SHA")),
            "{name}"
        );
    }
}

#[test]
fn automatic_packaging_trigger_is_rejected() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    let path = scratch.path.join(RELEASE_WORKFLOW);
    let workflow = fs::read_to_string(&path).unwrap();
    let text = workflow.replacen("on:\n", "on:\n  push:\n    branches: [main]\n", 1);
    fs::write(&path, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("manually"))
    );
}

#[test]
fn a_pull_request_workflow_with_secrets_or_packaging_is_rejected() {
    for unsafe_line in [
        "      - run: ./scripts/Package.ps1 -Architecture arm64\n",
        "      - run: echo ${{ secrets.STORE_TOKEN }}\n",
    ] {
        let scratch = Scratch::new();
        copy_metadata_fixture(&scratch.path);
        let path = scratch.path.join(CI_WORKFLOW);
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str(unsafe_line);
        fs::write(&path, text).unwrap();
        assert!(
            metadata_errors(&scratch.path)
                .iter()
                .any(|error| error.contains("no secret and no packaging")),
            "{unsafe_line}"
        );
    }
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    let path = scratch.path.join(CI_WORKFLOW);
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("\n  pull_request:\n", "\n  pull_request_target:\n");
    fs::write(&path, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("no secret and no packaging"))
    );
}

#[test]
fn write_access_and_github_release_publication_are_rejected() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    let path = scratch.path.join(RELEASE_WORKFLOW);
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("      - run: gh release upload v1.0.0 dist/*\n");
    fs::write(&path, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.starts_with(RELEASE_WORKFLOW) && error.contains("GitHub Releases"))
    );
    let path = scratch.path.join(CI_WORKFLOW);
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("contents: read", "contents: write");
    fs::write(&path, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.starts_with(CI_WORKFLOW) && error.contains("read-only"))
    );
}

#[test]
fn msix_identity_drift_is_rejected() {
    let scratch = Scratch::new();
    copy_metadata_fixture(&scratch.path);
    let path = scratch.path.join("packaging/msix/AppxManifest.xml");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("PiriMedya.PowerKit", "PiriMedya.Other");
    fs::write(&path, text).unwrap();
    assert!(
        metadata_errors(&scratch.path)
            .iter()
            .any(|error| error.contains("MSIX manifest"))
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
