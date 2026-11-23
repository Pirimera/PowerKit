use std::path::{Path, PathBuf};

fn main() {
    // The WinGet metadata is shared with packaging and lives at the
    // repository root.
    let package =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = package.join("../../..");
    println!(
        "cargo:rerun-if-changed={}",
        root.join("third_party/winget/Microsoft.Management.Deployment.winmd")
            .display()
    );
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("windows") {
        return;
    }
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
