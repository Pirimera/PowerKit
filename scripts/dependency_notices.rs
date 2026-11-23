//! Collect exact locked Cargo dependency license texts for distribution.
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next();
        }
    }
    None
}

fn license_files(directory: &Path, license_file: Option<&str>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let upper = name.to_ascii_uppercase();
            if upper.starts_with("LICENSE")
                || upper.starts_with("LICENCE")
                || upper.starts_with("COPYING")
                || upper.starts_with("NOTICE")
            {
                files.push(path);
            }
        }
    }
    if let Some(license_file) = license_file {
        files.push(directory.join(license_file));
    }
    files.sort();
    files.dedup();
    files
}

fn main() -> ExitCode {
    let Some(output) = arg_value("--output").map(PathBuf::from) else {
        eprintln!("usage: dependency_notices --output PATH --target TRIPLE");
        return ExitCode::from(2);
    };
    let Some(target) = arg_value("--target") else {
        eprintln!("usage: dependency_notices --output PATH --target TRIPLE");
        return ExitCode::from(2);
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scripts crate lives in the repository")
        .to_path_buf();
    let metadata = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            &target,
        ])
        .current_dir(&root)
        .output();
    let metadata = match metadata {
        Ok(output) if output.status.success() => output.stdout,
        Ok(output) => {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("cargo metadata failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let metadata: Value = match serde_json::from_slice(&metadata) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("cargo metadata JSON failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let empty = Vec::new();
    let active = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect::<Vec<_>>();
    let members = metadata["workspace_members"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|id| id.as_str())
        .collect::<Vec<_>>();
    let mut packages = metadata["packages"].as_array().cloned().unwrap_or_default();
    packages.sort_by(|left, right| {
        (
            left["name"].as_str().unwrap_or(""),
            left["version"].as_str().unwrap_or(""),
        )
            .cmp(&(
                right["name"].as_str().unwrap_or(""),
                right["version"].as_str().unwrap_or(""),
            ))
    });
    let mut sections = vec![
        "PowerKit third-party notices".into(),
        format!("Cargo dependencies for {target}; includes build dependencies."),
        "Generated from Cargo.lock and upstream license files. No source paths are included."
            .into(),
    ];
    for package in packages {
        let id = package["id"].as_str().unwrap_or("");
        if !active.contains(&id) || members.contains(&id) {
            continue;
        }
        let name = package["name"].as_str().unwrap_or("");
        let version = package["version"].as_str().unwrap_or("");
        let directory = Path::new(package["manifest_path"].as_str().unwrap_or("."))
            .parent()
            .unwrap_or(Path::new("."));
        let mut licenses = license_files(directory, package["license_file"].as_str());
        if licenses.is_empty() {
            let reviewed = root
                .join("docs/licenses")
                .join(format!("{name}-{version}.txt"));
            if reviewed.is_file() {
                licenses.push(reviewed);
            }
        }
        if licenses.is_empty() {
            eprintln!("Review missing license text: {name} {version}");
            return ExitCode::FAILURE;
        }
        let spdx = package["license"].as_str().unwrap_or("see license text");
        sections.push(format!("\n=== {name} {version} ===\nSPDX: {spdx}"));
        for file in licenses {
            let text = match fs::read_to_string(&file) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("{}: {error}", file.display());
                    return ExitCode::FAILURE;
                }
            };
            let file_name = file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("license");
            sections.push(format!("--- {file_name} ---\n{text}"));
        }
    }
    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(&output, sections.join("\n\n") + "\n") {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    println!(
        "Dependency notices generated: {}",
        output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("THIRD-PARTY-NOTICES.txt")
    );
    ExitCode::SUCCESS
}
