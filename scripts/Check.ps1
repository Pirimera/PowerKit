param(
    [ValidateSet('arm64','x64')][string]$Architecture = 'arm64'
)
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    & cargo test --manifest-path scripts/Cargo.toml --locked
    if ($LASTEXITCODE) { throw 'Repository checks failed' }
    & cargo fmt --all --manifest-path scripts/Cargo.toml -- --check
    if ($LASTEXITCODE) { throw 'Repository checker formatting failed' }
    & cargo clippy --manifest-path scripts/Cargo.toml --locked --all-targets -- -D warnings
    if ($LASTEXITCODE) { throw 'Repository checker Clippy failed' }
    & ./scripts/cargo.ps1 -Target $Architecture -CargoArgs @('test','--locked')
    if ($LASTEXITCODE) { throw 'Rust tests failed' }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE) { throw 'Formatting failed' }
    & ./scripts/cargo.ps1 -Target $Architecture -CargoArgs @('clippy','--locked','--all-targets','--','-D','warnings')
    if ($LASTEXITCODE) { throw 'Clippy failed' }
} finally { Pop-Location }
exit 0
