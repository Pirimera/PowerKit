param([string]$Exe = (Join-Path $PSScriptRoot 'powerkit.exe'))
$ErrorActionPreference = 'Stop'
$p = Start-Process -FilePath (Resolve-Path -LiteralPath $Exe).Path -ArgumentList '--smoke-test' -WindowStyle Hidden -PassThru
if (!$p.WaitForExit(15000)) {
    Stop-Process -Id $p.Id
    throw 'AlwaysOn smoke test zaman aşımı'
}
if ($p.ExitCode -ne 0) { throw "AlwaysOn smoke test başarısız: $($p.ExitCode)" }
Write-Output 'PASS: AlwaysOn güç modu ve zaman aşımı mesaj döngüsü. Menü görsel testi ayrıca yapılmalıdır.'
