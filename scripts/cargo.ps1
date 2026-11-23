param(
    [ValidateSet('arm64','x64')][string]$Target = 'arm64',
    [Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArgs = @('build','--release')
)
$ErrorActionPreference = 'Stop'
if ($env:POWERKIT_LLVM_ROOT) { $env:PATH = "$env:POWERKIT_LLVM_ROOT/bin;" + $env:PATH }
$triple = if ($Target -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    # Put target options before the test/clippy argument separator.
    $separator = [Array]::IndexOf($CargoArgs, '--')
    if ($separator -ge 0) {
        $before = if ($separator -gt 0) { $CargoArgs[0..($separator - 1)] } else { @() }
        $after = if ($separator + 1 -lt $CargoArgs.Count) { $CargoArgs[($separator + 1)..($CargoArgs.Count - 1)] } else { @() }
        $invokeArgs = @($before) + @('--target', $triple, '--') + @($after)
        & cargo @invokeArgs
    } else { & cargo @CargoArgs --target $triple }
    $result = $LASTEXITCODE
} finally { Pop-Location }
exit $result
