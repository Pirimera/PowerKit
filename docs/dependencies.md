# Dependency licensing

PowerKit itself uses MIT. Distribution notices are generated from locked dependency
license files by `cargo run --manifest-path scripts/Cargo.toml --locked --bin dependency-notices`;
build dependencies are included too.
Generated notices are build outputs, not hand-maintained source.

When a crate omits its license text, add a reviewed copy under `docs/licenses/`
named `<crate>-<version>.txt`. The generator fails on missing license texts instead
of silently omitting them.

This is an attribution inventory, not a security audit or a claim that every possible
distribution condition has been mechanically verified. Dependency changes still
need license and platform review.

PowerStore also vendors
[`Microsoft.Management.Deployment.winmd`](../third_party/winget/README.md) from
Microsoft.WindowsPackageManager.ComInterop 1.29.380 (MIT). That file is not a
Cargo package; keep the third_party README current when refreshing the pin.
