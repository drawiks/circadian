param(
    [switch]$RemoveData
)

$ErrorActionPreference = "SilentlyContinue"

Get-Process circadian, circadiand, circadian-app, circadian-daemon |
    Stop-Process -Force
Start-Sleep -Milliseconds 300

$installDir = Join-Path $env:LOCALAPPDATA "Programs\Circadian"
$startMenu = [Environment]::GetFolderPath("Programs")
Remove-Item (Join-Path $startMenu "Circadian.lnk") -Force
Remove-Item $installDir -Recurse -Force
Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "Circadian"

if ($RemoveData) {
    Remove-Item (Join-Path $env:LOCALAPPDATA "Circadian") -Recurse -Force
}

Write-Host "Circadian removed"
