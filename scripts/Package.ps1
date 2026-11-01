param(
    [ValidateSet('arm64','x64')][string]$Architecture = 'arm64',
    [string]$VpkPath = 'vpk',
    [string]$OutputRoot = 'dist'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
    $manifest = Get-Content Cargo.toml -Raw
    $version = [regex]::Match($manifest, '(?m)^version = "(\d+\.\d+\.\d+)"').Groups[1].Value
    if (!$version) { throw 'Cargo.toml must contain a stable numeric version.' }
    if ($env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -cne "v$version") {
        throw "Tag must match Cargo.toml: v$version"
    }
    $triple = if ($Architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    $exe = Join-Path $repo "target/$triple/release/powerkit.exe"
    if (!(Test-Path $exe)) { throw "Missing release executable: $exe" }
    $bytes = [IO.File]::ReadAllBytes($exe)
    $pe = [BitConverter]::ToInt32($bytes, 0x3c)
    $machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
    $expected = if ($Architecture -eq 'arm64') { 0xaa64 } else { 0x8664 }
    if ($machine -ne $expected) { throw 'Executable architecture does not match package channel.' }
    $outputBase = if ([IO.Path]::IsPathRooted($OutputRoot)) { $OutputRoot } else { Join-Path $repo $OutputRoot }
    $root = [IO.Path]::GetFullPath((Join-Path $outputBase "$version/$Architecture"))
    if (Test-Path $root) { throw "Output already exists. Choose a fresh OutputRoot: $root" }
    $stage = New-Item -ItemType Directory -Path "$root/stage" -Force
    $releases = New-Item -ItemType Directory -Path "$root/releases" -Force
    Copy-Item $exe "$stage/powerkit.exe"
    Copy-Item README.md "$stage/README.md"
    Copy-Item docs "$stage/docs" -Recurse
    Copy-Item scripts/Test-AlwaysOn.ps1 "$stage/Test-AlwaysOn.ps1"
    & $VpkPath pack --yes --skip-updates --packId Pirimera.PowerKit --packTitle PowerKit --packAuthors Pirimera --packVersion $version --packDir $stage.FullName --mainExe powerkit.exe --runtime "win10.0.22000-$Architecture" --channel "win-$Architecture" --outputDir $releases.FullName --delta None --noPortable --instLocation PerUser --shortcuts StartMenuRoot --releaseNotes docs/RELEASE-NOTES.md
    if ($LASTEXITCODE -ne 0) { throw "Velopack pack failed: $LASTEXITCODE" }
    $feedPath = Join-Path $releases "releases.win-$Architecture.json"
    $feed = Get-Content $feedPath -Raw | ConvertFrom-Json
    if (!$feed.Assets -or !(Get-ChildItem $releases -Filter '*Setup.exe')) { throw 'Installer/feed missing.' }
    foreach ($asset in $feed.Assets) {
        $package = Join-Path $releases $asset.FileName
        if (!(Test-Path $package)) { throw "Feed refers to missing package: $package" }
        if ($asset.SHA256 -and (Get-FileHash $package -Algorithm SHA256).Hash -ne $asset.SHA256) { throw 'Feed SHA256 mismatch.' }
    }
    Get-ChildItem $releases -File | Sort-Object Name | ForEach-Object {
        '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
    } | Set-Content (Join-Path $releases "SHA256SUMS-$Architecture.txt") -Encoding utf8NoBOM
    Write-Host "Release assets: $releases"
} finally { Pop-Location }
