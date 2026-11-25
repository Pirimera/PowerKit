param(
    [ValidateSet('arm64','x64')][string]$Architecture = 'arm64',
    # Must equal the signing certificate subject; Partner Center assigns the Store value.
    [string]$Publisher = 'CN=1D60B9B0-5B58-4FDF-B209-1FEC64FABD24',
    # Optional code-signing certificate in Cert:\CurrentUser\My. Leave empty for a Store upload.
    [string]$CertificateThumbprint = '',
    [string]$OutputRoot = 'dist',
    # Fourth part of the MSIX version, for local test packages only: a higher value
    # installs over the previous test package. The Store reserves this part, so it
    # stays 0 for a Store upload.
    [ValidateRange(0, 65535)][int]$Revision = 0
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent

function Find-SdkTool([string]$Name) {
    $onPath = Get-Command $Name -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $kitBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $hostArch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
    $tool = Get-ChildItem $kitBin -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^\d+(\.\d+){3}$' } |
        Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName "$hostArch\$Name" } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
    if (!$tool) { throw "$Name not found. Install the Windows 10/11 SDK." }
    return $tool
}

function Assert-Machine([string]$Path, [uint16]$Expected) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    $pe = [BitConverter]::ToInt32($bytes, 0x3c)
    if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne $Expected) {
        throw "Architecture does not match the package: $Path"
    }
}

Push-Location $repo
try {
    $manifest = Get-Content apps/powerkit/Cargo.toml -Raw
    $version = [regex]::Match($manifest, '(?m)^version = "(\d+\.\d+\.\d+)"').Groups[1].Value
    if (!$version) { throw 'apps/powerkit/Cargo.toml must contain a stable numeric version.' }
    $releaseTag = $env:POWERKIT_RELEASE_TAG
    if ($env:GITHUB_REF_TYPE -eq 'tag') {
        if ($releaseTag -and $releaseTag -cne $env:GITHUB_REF_NAME) {
            throw 'Release tag input does not match the GitHub ref.'
        }
        if (-not $releaseTag) { $releaseTag = $env:GITHUB_REF_NAME }
    }
    if ($releaseTag -and $releaseTag -cne "v$version") {
        throw "Tag must match apps/powerkit/Cargo.toml: v$version"
    }
    if ($Revision -and ($releaseTag -or !$CertificateThumbprint)) {
        throw '-Revision is for signed local test packages; a release or Store upload keeps revision 0.'
    }
    $packageVersion = "$version.$Revision"
    $triple = if ($Architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    $machine = if ($Architecture -eq 'arm64') { 0xaa64 } else { 0x8664 }
    # Honour CARGO_TARGET_DIR / cargo config rather than assuming ./target.
    $metadataJson = & cargo metadata --format-version 1 --no-deps --manifest-path (Join-Path $repo 'Cargo.toml')
    if ($LASTEXITCODE) { throw 'cargo metadata failed' }
    $targetRoot = ($metadataJson | ConvertFrom-Json).target_directory
    if (!$targetRoot) { throw 'cargo metadata did not report target_directory' }
    $exe = Join-Path $targetRoot "$triple/release/powerkit.exe"
    $dll = Join-Path $targetRoot "$triple/release/powerkit_shell.dll"
    # PowerStore's WinGet COM child; it loads the WinGet metadata from its own folder.
    $worker = Join-Path $targetRoot "$triple/release/powerkit-winget-worker.exe"
    foreach ($binary in @($exe, $dll, $worker)) {
        if (!(Test-Path $binary)) { throw "Missing release binary (run cargo build --release first): $binary" }
        Assert-Machine $binary $machine
    }

    $makeAppx = Find-SdkTool 'makeappx.exe'
    $makePri = Find-SdkTool 'makepri.exe'
    $outputBase = if ([IO.Path]::IsPathRooted($OutputRoot)) { $OutputRoot } else { Join-Path $repo $OutputRoot }
    # A test revision gets its own folder, so earlier outputs need no removal.
    $outputVersion = if ($Revision) { $packageVersion } else { $version }
    $root = [IO.Path]::GetFullPath((Join-Path $outputBase "$outputVersion/$Architecture"))
    if (Test-Path $root) { throw "Output already exists. Remove it or choose a fresh OutputRoot: $root" }
    $stage = (New-Item -ItemType Directory -Path "$root/stage" -Force).FullName

    Copy-Item $exe "$stage/powerkit.exe"
    Copy-Item $dll "$stage/powerkit_shell.dll"
    Copy-Item $worker "$stage/powerkit-winget-worker.exe"
    $winmd = Join-Path $repo 'third_party/winget/Microsoft.Management.Deployment.winmd'
    if (!(Test-Path $winmd)) { throw "Missing WinGet COM metadata: $winmd" }
    Copy-Item $winmd "$stage/Microsoft.Management.Deployment.winmd"
    Copy-Item packaging/windows/README.md "$stage/README.md"
    Copy-Item LICENSE "$stage/LICENSE"
    & cargo run --manifest-path scripts/Cargo.toml --locked --bin dependency-notices -- --output "$stage/THIRD-PARTY-NOTICES.txt" --target $triple
    if ($LASTEXITCODE) { throw 'Dependency notice generation failed' }
    New-Item -ItemType Directory -Path "$stage/Assets" | Out-Null
    Copy-Item brand/windows/msix/*.png "$stage/Assets/"

    $tokens = @{
        '{{PUBLISHER}}' = [Security.SecurityElement]::Escape($Publisher)
        '{{VERSION}}' = $packageVersion
        '{{ARCHITECTURE}}' = $Architecture
        '{{SHELL_CLSID}}' = (Get-Content packaging/msix/shell_clsid.txt -Raw).Trim()
        '{{RENAME_CLSID}}' = (Get-Content packaging/msix/shell_clsid_rename.txt -Raw).Trim()
    }
    $appx = Get-Content packaging/msix/AppxManifest.xml -Raw
    foreach ($token in $tokens.Keys) { $appx = $appx.Replace($token, $tokens[$token]) }
    if ($appx -match '\{\{[A-Z_]+\}\}') { throw "Unfilled manifest token: $($Matches[0])" }
    [IO.File]::WriteAllText("$stage/AppxManifest.xml", $appx, [Text.UTF8Encoding]::new($false))

    # resources.pri maps Assets\Name.png to the scale/targetsize variants.
    $priConfig = Join-Path $root 'priconfig.xml'
    & $makePri createconfig /cf $priConfig /dq lang-en-US /pv 10.0.0 /o | Out-Null
    if ($LASTEXITCODE) { throw 'makepri createconfig failed' }
    & $makePri new /pr $stage /cf $priConfig /mn "$stage/AppxManifest.xml" /of "$stage/resources.pri" /o | Out-Null
    if ($LASTEXITCODE) { throw 'makepri new failed' }

    $package = Join-Path $root "PowerKit_$($packageVersion)_$Architecture.msix"
    & $makeAppx pack /d $stage /p $package /o | Out-Null
    if ($LASTEXITCODE) { throw 'makeappx pack failed' }

    if ($CertificateThumbprint) {
        $signTool = Find-SdkTool 'signtool.exe'
        & $signTool sign /fd SHA256 /sha1 $CertificateThumbprint /s My $package | Out-Null
        if ($LASTEXITCODE) { throw 'signtool sign failed' }
    }
    $hash = (Get-FileHash $package -Algorithm SHA256).Hash.ToLowerInvariant()
    $sumsPath = Join-Path $root "SHA256SUMS-$Architecture.txt"
    # Windows PowerShell 5.1 has no utf8NoBOM; write UTF-8 without BOM explicitly.
    [IO.File]::WriteAllLines($sumsPath, @("$hash  $(Split-Path $package -Leaf)"), [Text.UTF8Encoding]::new($false))
    Write-Host "MSIX: $package$(if (!$CertificateThumbprint) { ' (unsigned)' })"
} finally { Pop-Location }
