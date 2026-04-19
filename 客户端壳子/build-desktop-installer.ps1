param(
    [string]$ApiBaseUrl
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$cargoToml = Join-Path $repoRoot "desktop-client\Cargo.toml"
$cargoText = Get-Content $cargoToml -Raw
$versionMatch = [regex]::Match($cargoText, 'version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) {
    throw "无法从 $cargoToml 解析版本号。"
}
$appVersion = $versionMatch.Groups[1].Value

$releaseExe = Join-Path $repoRoot "desktop-client\target\release\sub2api-desktop.exe"
$outputDir = Join-Path $repoRoot "dist\desktop-client"
$issPath = Join-Path $repoRoot "desktop-client\packaging\windows\desktop-client.iss"
$iscc = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
$cargoConfigDir = Join-Path $repoRoot ".cargo"
$cargoConfigPath = Join-Path $cargoConfigDir "config.toml"
$installerPath = Join-Path $outputDir "Sub2API-Desktop-Setup-$appVersion.exe"
$licenseCandidates = @(
    (Join-Path $repoRoot "LICENSE"),
    (Join-Path (Split-Path $repoRoot -Parent) "LICENSE")
)
$licenseFile = $licenseCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

function Normalize-ApiBaseUrl([string]$InputUrl) {
    $trimmed = ""
    if ($null -ne $InputUrl) {
        $trimmed = $InputUrl.Trim()
    }
    if ([string]::IsNullOrWhiteSpace($trimmed)) {
        throw "ApiBaseUrl 不能为空。"
    }

    $trimmed = $trimmed.TrimEnd('/')
    if ($trimmed.EndsWith('/api/v1')) {
        return $trimmed
    }
    return "$trimmed/api/v1"
}

if (-not (Test-Path $iscc)) {
    throw "未找到 Inno Setup 编译器：$iscc"
}

if (-not $licenseFile) {
    throw "未找到 LICENSE 文件。已检查路径：$($licenseCandidates -join ', ')"
}

if ([string]::IsNullOrWhiteSpace($ApiBaseUrl)) {
    throw "打包安装包时必须显式传入 -ApiBaseUrl，例如：powershell -NoProfile -ExecutionPolicy Bypass -File .\\build-desktop-installer.ps1 -ApiBaseUrl `"https://your-sub2api.example.com`""
}

New-Item -ItemType Directory -Force $outputDir | Out-Null
New-Item -ItemType Directory -Force $cargoConfigDir | Out-Null
@'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
'@ | Set-Content -Path $cargoConfigPath -Encoding UTF8

Write-Host "==> 构建 release 二进制"
$normalizedApiBaseUrl = Normalize-ApiBaseUrl $ApiBaseUrl
$env:SUB2API_DESKTOP_API_BASE_URL = $normalizedApiBaseUrl
Write-Host "==> 使用 API 基地址: $normalizedApiBaseUrl"
Push-Location $repoRoot
try {
    & cargo build --release --manifest-path $cargoToml
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build 失败，退出码：$LASTEXITCODE"
    }
}
finally {
    Pop-Location
    if (Test-Path $cargoConfigPath) {
        Remove-Item $cargoConfigPath -Force
    }
}

if (-not (Test-Path $releaseExe)) {
    throw "未生成 release 二进制：$releaseExe"
}

Write-Host "==> 生成 Inno Setup 安装包"
& $iscc `
    "/DMyAppVersion=$appVersion" `
    "/DMySourceExe=$releaseExe" `
    "/DMyOutputDir=$outputDir" `
    "/DMyRepoRoot=$repoRoot" `
    "/DMyLicenseFile=$licenseFile" `
    $issPath
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup 编译失败，退出码：$LASTEXITCODE"
}

if (-not (Test-Path $installerPath)) {
    throw "未生成安装包：$installerPath"
}

$hash = (Get-FileHash $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$hashPath = "$installerPath.sha256"
Set-Content -Path $hashPath -Value "$hash  $(Split-Path $installerPath -Leaf)" -Encoding ascii

Write-Host "==> 安装包输出目录: $outputDir"
Write-Host "==> SHA256 文件: $hashPath"
