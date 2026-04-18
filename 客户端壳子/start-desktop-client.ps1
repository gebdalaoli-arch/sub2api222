$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$manifestPath = Join-Path $repoRoot 'desktop-client\Cargo.toml'
$logDir = Join-Path $repoRoot 'desktop-client\target\run-logs'
$cargoConfigDir = Join-Path $repoRoot '.cargo'
$cargoConfigPath = Join-Path $cargoConfigDir 'config.toml'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
New-Item -ItemType Directory -Force -Path $cargoConfigDir | Out-Null

$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$stdoutLog = Join-Path $logDir "desktop-client-$timestamp.out.log"
$stderrLog = Join-Path $logDir "desktop-client-$timestamp.err.log"
$pidFile = Join-Path $logDir 'desktop-client.pid'

@'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
'@ | Set-Content -Path $cargoConfigPath -Encoding UTF8

$cargoArgs = @('run', '--manifest-path', $manifestPath)

$process = Start-Process -FilePath 'cargo' `
    -ArgumentList $cargoArgs `
    -WorkingDirectory $repoRoot `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdoutLog `
    -RedirectStandardError $stderrLog `
    -PassThru

Set-Content -Path $pidFile -Value $process.Id -Encoding ascii
Write-Host "Sub2API desktop client starting. PID: $($process.Id)"
Write-Host "Logs: $logDir"
