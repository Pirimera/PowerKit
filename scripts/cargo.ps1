param(
    [ValidateSet('arm64','x64')][string]$Target = 'arm64',
    [string]$ToolchainRoot = $env:POWERKIT_TOOLCHAIN_ROOT,
    [Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArgs = @('build','--release')
)
$ErrorActionPreference = 'Stop'
$triple = if ($Target -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
if ($ToolchainRoot) {
    $env:RUSTUP_HOME = Join-Path $ToolchainRoot 'toolchain/rustup'
    $env:CARGO_HOME = Join-Path $ToolchainRoot 'toolchain/cargo'
    $rustBin = Join-Path $env:RUSTUP_HOME 'toolchains/stable-aarch64-pc-windows-msvc/bin'
    $vc = (Get-ChildItem "$ToolchainRoot/msvc/VC/Tools/MSVC" -Directory | Sort-Object Name -Descending | Select-Object -First 1).FullName
    $sdk = Join-Path $ToolchainRoot 'msvc/Windows Kits/10'
    $ver = (Get-ChildItem "$sdk/Lib" -Directory | Sort-Object Name -Descending | Select-Object -First 1).Name
    $env:PATH = "$rustBin;$vc/bin/Hostarm64/arm64;$sdk/bin/$ver/arm64;" + $env:PATH
    $env:LIB = "$vc/lib/arm64;$sdk/Lib/$ver/ucrt/arm64;$sdk/Lib/$ver/um/arm64"
    $env:INCLUDE = "$vc/include;$sdk/Include/$ver/ucrt;$sdk/Include/$ver/shared;$sdk/Include/$ver/um"
    $env:CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER = "$vc/bin/Hostarm64/arm64/link.exe"
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = "$vc/bin/Hostarm64/x64/link.exe"
    if ($Target -eq 'x64') {
        $env:CARGO_ENCODED_RUSTFLAGS = @("-Lnative=$vc/lib/x64", "-Lnative=$sdk/Lib/$ver/ucrt/x64", "-Lnative=$sdk/Lib/$ver/um/x64") -join [char]31
    } else { Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue }
}
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    # Put target options before the test/clippy argument separator.
    $separator = [Array]::IndexOf($CargoArgs, '--')
    if ($separator -ge 0) {
        $before = if ($separator -gt 0) { $CargoArgs[0..($separator - 1)] } else { @() }
        $after = if ($separator + 1 -lt $CargoArgs.Count) { $CargoArgs[($separator + 1)..($CargoArgs.Count - 1)] } else { @() }
        & cargo @before --target $triple -- @after
    } else { & cargo @CargoArgs --target $triple }
    $result = $LASTEXITCODE
} finally { Pop-Location }
exit $result
