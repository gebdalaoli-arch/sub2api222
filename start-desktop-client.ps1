$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$manifestPath = Join-Path $repoRoot 'desktop-client\Cargo.toml'
$logDir = Join-Path $repoRoot 'desktop-client\target\run-logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$stdoutLog = Join-Path $logDir "desktop-client-$timestamp.out.log"
$stderrLog = Join-Path $logDir "desktop-client-$timestamp.err.log"
$pidFile = Join-Path $logDir 'desktop-client.pid'

$cargoArgs = @(
    '--config', 'source.crates-io.replace-with="rsproxy-sparse"',
    '--config', 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"',
    'run',
    '--manifest-path', $manifestPath
)

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
