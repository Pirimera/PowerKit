use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=brand/windows/PowerKit.ico");
    println!("cargo:rerun-if-changed=third_party/winget/Microsoft.Management.Deployment.winmd");
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("windows") {
        return;
    }
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    link_manifest(&root.join("app.manifest"));
    // Explorer reads icon index 0 from the executable for the Start menu and taskbar.
    // The window icon is applied at runtime and does not fill that resource.
    link_icon(&root.join("brand/windows/PowerKit.ico"));
    generate_winget_bindings(&root);
    copy_winget_winmd(&root);
}

fn generate_winget_bindings(root: &Path) {
    if std::env::var("CARGO_CFG_TARGET_ENV").ok().as_deref() != Some("msvc") {
        return;
    }
    println!("cargo:rustc-link-lib=onecoreuap");
    let winmd = root.join("third_party/winget/Microsoft.Management.Deployment.winmd");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let bindings = out.join("winget_bindings.rs");
    let _warnings = windows_bindgen::bindgen([
        "--in",
        "default",
        winmd.to_str().expect("winmd path is UTF-8"),
        "--out",
        bindings.to_str().expect("bindings path is UTF-8"),
        "--filter",
        "Microsoft.Management.Deployment",
        // InstallOptions.AllowedArchitectures is a vector of this enum.
        "Windows.System.ProcessorArchitecture",
        "--no-comment",
        "--no-allow",
    ]);
}

fn copy_winget_winmd(root: &Path) {
    let winmd = root.join("third_party/winget/Microsoft.Management.Deployment.winmd");
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let binary_directory = out_dir
        .ancestors()
        .nth(3)
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| out_dir.clone());
    std::fs::copy(
        &winmd,
        binary_directory.join("Microsoft.Management.Deployment.winmd"),
    )
    .unwrap_or_else(|error| panic!("copy WinGet winmd beside binary: {error}"));
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
    if let Some(root) = std::env::var_os("POWERKIT_TOOLCHAIN_ROOT") {
        roots.push(
            PathBuf::from(root)
                .join("msvc")
                .join("Windows Kits")
                .join("10")
                .join("bin"),
        );
    }
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
