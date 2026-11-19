# Creates or reuses a self-signed code-signing certificate for local MSIX trials.
# The private key stays in Cert:\CurrentUser\My; only the public .cer is exported.
param(
    # Must equal the -Publisher passed to Package.ps1.
    [string]$Publisher = 'CN=1D60B9B0-5B58-4FDF-B209-1FEC64FABD24',
    [string]$OutputRoot = 'dist'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$friendlyName = 'PowerKit MSIX test'

$certificate = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $Publisher -and $_.FriendlyName -eq $friendlyName -and $_.NotAfter -gt (Get-Date).AddDays(7) } |
    Sort-Object NotAfter -Descending |
    Select-Object -First 1
if (!$certificate) {
    $certificate = New-SelfSignedCertificate -Type Custom -Subject $Publisher -FriendlyName $friendlyName `
        -KeyUsage DigitalSignature -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(2) `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
}

$outputBase = if ([IO.Path]::IsPathRooted($OutputRoot)) { $OutputRoot } else { Join-Path $repo $OutputRoot }
New-Item -ItemType Directory -Path $outputBase -Force | Out-Null
$cer = Join-Path $outputBase 'PowerKit-test.cer'
Export-Certificate -Cert $certificate -FilePath $cer | Out-Null

# App Installer and Add-AppxPackage trust the machine-wide Trusted People store.
$trusted = Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object Thumbprint -eq $certificate.Thumbprint
if (!$trusted) {
    $elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
    if ($elevated) {
        Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
    } else {
        Write-Warning "Trust the certificate once from an elevated PowerShell:`n  Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
    }
}
Write-Host "Thumbprint: $($certificate.Thumbprint)"
Write-Host "Package:    ./scripts/Package.ps1 -Publisher '$Publisher' -CertificateThumbprint $($certificate.Thumbprint)"
