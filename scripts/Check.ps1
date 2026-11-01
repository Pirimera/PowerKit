param(
    [ValidateSet('arm64','x64')][string]$Architecture = 'arm64',
    [string]$ToolchainRoot = $env:POWERKIT_TOOLCHAIN_ROOT
)
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    & python scripts/check_repo.py
    if ($LASTEXITCODE) { throw 'Repository checks failed' }
    & python -m unittest discover -s scripts/tests
    if ($LASTEXITCODE) { throw 'Repository checker tests failed' }
    & ./scripts/cargo.ps1 -Target $Architecture -ToolchainRoot $ToolchainRoot -CargoArgs @('test','--locked')
    if ($LASTEXITCODE) { throw 'Rust tests failed' }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE) { throw 'Formatting failed' }
    & ./scripts/cargo.ps1 -Target $Architecture -ToolchainRoot $ToolchainRoot -CargoArgs @('clippy','--locked','--all-targets','--','-D','warnings')
    if ($LASTEXITCODE) { throw 'Clippy failed' }
} finally { Pop-Location }
exit 0
