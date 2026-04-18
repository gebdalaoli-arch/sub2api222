param(
    [string]$InstallerPath = (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) "dist\desktop-client\Sub2API-Desktop-Setup-0.1.0.exe"),
    [string]$CertThumbprint,
    [string]$PfxPath,
    [string]$PfxPassword,
    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$signtool = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe"
if (-not (Test-Path $signtool)) {
    throw "未找到 signtool：$signtool"
}
if (-not (Test-Path $InstallerPath)) {
    throw "未找到安装包：$InstallerPath"
}

$arguments = @("sign", "/fd", "SHA256", "/td", "SHA256", "/tr", $TimestampUrl)

if ($PfxPath) {
    if (-not (Test-Path $PfxPath)) {
        throw "未找到 PFX 证书：$PfxPath"
    }
    $arguments += @("/f", $PfxPath)
    if ($PfxPassword) {
        $arguments += @("/p", $PfxPassword)
    }
}
elseif ($CertThumbprint) {
    $arguments += @("/sha1", $CertThumbprint)
}
else {
    $arguments += "/a"
}

$arguments += $InstallerPath

Write-Host "==> 正在签名：$InstallerPath"
& $signtool @arguments
Write-Host "==> 签名完成"
