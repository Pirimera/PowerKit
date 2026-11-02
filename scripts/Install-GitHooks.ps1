$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$hooks = & git -C $repo rev-parse --git-path hooks
if ($LASTEXITCODE) { throw 'Could not locate the git hooks directory.' }
if (-not [IO.Path]::IsPathRooted($hooks)) {
    $hooks = Join-Path $repo $hooks
}
New-Item -ItemType Directory -Path $hooks -Force | Out-Null
$utf8 = New-Object System.Text.UTF8Encoding $false
foreach ($name in @('prepare-commit-msg', 'commit-msg')) {
    $text = [IO.File]::ReadAllText((Join-Path $PSScriptRoot "git-hooks/$name"))
    $text = $text -replace "`r`n", "`n" -replace "`r", "`n"
    if (-not $text.EndsWith("`n")) { $text += "`n" }
    [IO.File]::WriteAllText((Join-Path $hooks $name), $text, $utf8)
}
Write-Host "Installed PowerKit version hooks in $hooks"
