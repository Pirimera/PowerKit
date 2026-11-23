// Links the application manifest and icon into the executables of the
// package that includes this file (`include!` from its own build script).
use std::path::{Path, PathBuf};
use std::process::Command;

fn link_windows_resources() {
    let package = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    // Both application packages sit two levels below the repository root.
    let root = package.join("../..");
    let manifest = root.join("apps/powerkit/app.manifest");
    let icon = root.join("brand/windows/PowerKit.ico");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", icon.display());
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("windows") {
        return;
    }
    link_manifest(&manifest);
    // Explorer reads icon index 0 from the executable for the Start menu and taskbar.
    // The window icon is applied at runtime and does not fill that resource.
    link_icon(&icon);
}

fn link_manifest(manifest: &Path) {
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}

fn link_icon(icon: &Path) {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let compiled = out_dir.join("powerkit_icon.res");
    compile_icon(icon, &compiled);
    println!("cargo:rustc-link-arg-bins={}", compiled.display());
}

fn compile_icon(icon: &Path, compiled: &Path) {
    let script = compiled.with_extension("rc");
    let icon_path = icon.to_string_lossy().replace('\\', "/");
    if icon_path.contains('"') {
        panic!("icon path contains a quote: {icon_path}");
    }
    std::fs::write(&script, format!("1 ICON \"{icon_path}\"\n")).expect("write icon script");
    let compiler = resource_compiler();
    let mut command = Command::new(&compiler);
    if compiler
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("llvm-rc.exe") || name.eq_ignore_ascii_case("llvm-rc")
        })
    {
        command.arg("/no-preprocess");
    }
    let output = command
        .arg("/FO")
        .arg(compiled)
        .arg(&script)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", compiler.display()));
    if !output.status.success() {
        panic!(
            "icon resource compile failed ({}):\n{}{}",
            compiler.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn resource_compiler() -> PathBuf {
    if let Some(path) = command_on_path("rc.exe") {
        return path;
    }
    if let Some(path) = newest_windows_kit_rc() {
        return path;
    }
    if let Some(path) = command_on_path("llvm-rc.exe") {
        return path;
    }
    if let Some(root) = std::env::var_os("POWERKIT_LLVM_ROOT") {
        let path = PathBuf::from(root).join("bin").join("llvm-rc.exe");
        if path.is_file() {
            return path;
        }
    }
    panic!("Windows resource compiler not found. Install the Windows SDK or LLVM.");
}

fn command_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn newest_windows_kit_rc() -> Option<PathBuf> {
    let host = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        "x86" => "x86",
        _ => "x64",
    };
    let mut found: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    for bin in kit_bin_dirs() {
        let Ok(versions) = std::fs::read_dir(&bin) else {
            continue;
        };
        for version in versions.flatten() {
            let dir = version.path();
            if !dir.is_dir() {
                continue;
            }
            for arch in [host, "x64", "arm64", "x86"] {
                let rc = dir.join(arch).join("rc.exe");
                if rc.is_file() {
                    found.push((version_key(&dir), rc));
                    break;
                }
            }
        }
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found.pop().map(|(_, path)| path)
}

fn kit_bin_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(root) = std::env::var_os(variable) {
            roots.push(
                PathBuf::from(root)
                    .join("Windows Kits")
                    .join("10")
                    .join("bin"),
            );
        }
    }
    roots
}

fn version_key(path: &Path) -> Vec<u32> {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}
