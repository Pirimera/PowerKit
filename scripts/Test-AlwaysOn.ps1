param(
    [string]$Exe,
    [switch]$MeasureIdle
)
$ErrorActionPreference = 'Stop'
if (!$Exe) {
    # The release build for this machine's own architecture.
    $triple = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    $Exe = Join-Path $PSScriptRoot "../target/$triple/release/powerkit.exe"
    if (!(Test-Path -LiteralPath $Exe)) { throw "Release build not found (run cargo build --release first): $Exe" }
}
$arguments = @('--smoke-test')
if ($MeasureIdle) { $arguments += '--idle' }
$timer = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath (Resolve-Path -LiteralPath $Exe).Path -ArgumentList $arguments -WindowStyle Hidden -PassThru
try {
    if ($MeasureIdle) {
        Start-Sleep -Milliseconds 1000
        $p.Refresh()
        $cpuStart = $p.TotalProcessorTime.TotalMilliseconds
        Start-Sleep -Seconds 10
        $p.Refresh()
        [pscustomobject]@{
            SampleSeconds = 10
            CpuMilliseconds = [Math]::Round($p.TotalProcessorTime.TotalMilliseconds - $cpuStart, 2)
            WorkingSetMiB = [Math]::Round($p.WorkingSet64 / 1MB, 2)
            PrivateBytesMiB = [Math]::Round($p.PrivateMemorySize64 / 1MB, 2)
            Context = 'Isolated idle self-test; no Explorer icon; not a full desktop benchmark'
        } | Format-List
    }
    if (!$p.WaitForExit(15000)) { throw 'AlwaysOn self-test zaman aşımı' }
    if ($p.ExitCode -ne 0) { throw "AlwaysOn self-test başarısız: $($p.ExitCode)" }
    if ($timer.Elapsed.TotalSeconds -lt 2) { throw 'Test senaryoları çalışmadan erken çıkış' }
    Write-Output 'PASS: bağımsız test oturumu tamamlandı. Fiziksel uyku testi ayrıca yapılmalıdır.'
} finally {
    $p.Refresh()
    if (!$p.HasExited) { Stop-Process -Id $p.Id }
    $p.Dispose()
}
exit 0
