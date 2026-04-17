$ErrorActionPreference = 'SilentlyContinue'

$repoRoot = (Split-Path -Parent $MyInvocation.MyCommand.Path)
$logDir = Join-Path $repoRoot 'desktop-client\target\run-logs'
$pidFile = Join-Path $logDir 'desktop-client.pid'

function Test-OwnedDesktopClientProcess {
    param([Parameter(Mandatory = $true)] $ProcessInfo)

    $exe = [string]$ProcessInfo.ExecutablePath
    $cmd = [string]$ProcessInfo.CommandLine
    $name = [string]$ProcessInfo.Name

    $underRepo = $false
    if ($exe) {
        $underRepo = $exe.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)
    }
    if (-not $underRepo -and $cmd) {
        $underRepo = $cmd.IndexOf($repoRoot, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
    }

    if (-not $underRepo) {
        return $false
    }

    if ($name -ieq 'sub2api-desktop.exe') {
        return $true
    }

    return $name -ieq 'cargo.exe' -and $cmd -like '*desktop-client*Cargo.toml*'
}

function Stop-OwnedDesktopClientProcess {
    param([Parameter(Mandatory = $true)] $ProcessInfo)

    if (Test-OwnedDesktopClientProcess -ProcessInfo $ProcessInfo) {
        Stop-Process -Id $ProcessInfo.ProcessId -Force
    }
}

if (Test-Path $pidFile) {
    $recordedPid = Get-Content $pidFile | Select-Object -First 1
    if ($recordedPid -match '^\d+$') {
        $recorded = Get-CimInstance Win32_Process -Filter "ProcessId = $recordedPid"
        if ($recorded) {
            Stop-OwnedDesktopClientProcess -ProcessInfo $recorded
        }
    }
    Remove-Item -LiteralPath $pidFile -Force
}

Get-CimInstance Win32_Process |
    Where-Object { Test-OwnedDesktopClientProcess -ProcessInfo $_ } |
    ForEach-Object { Stop-OwnedDesktopClientProcess -ProcessInfo $_ }

Write-Host 'Sub2API desktop client stopped if it was running.'
