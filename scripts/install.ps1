param(
    [string]$Source = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Source)) {
    $Source = Join-Path $root "target\x86_64-pc-windows-msvc\release"
}

$app = Join-Path $Source "circadian-app.exe"
$daemon = Join-Path $Source "circadian-daemon.exe"
$icon = Join-Path $root "crates\daemon\assets\circadian.ico"
foreach ($file in @($app, $daemon, $icon)) {
    if (-not (Test-Path $file)) {
        throw "Missing file: $file. Build the release binaries first."
    }
}

$installDir = Join-Path $env:LOCALAPPDATA "Programs\Circadian"
$assetsDir = Join-Path $installDir "assets"

Get-Process circadian, circadiand, circadian-app, circadian-daemon -ErrorAction SilentlyContinue |
    Stop-Process -Force
Start-Sleep -Milliseconds 300

New-Item -ItemType Directory -Force -Path $installDir, $assetsDir | Out-Null
Copy-Item $app (Join-Path $installDir "circadian.exe") -Force
Copy-Item $daemon (Join-Path $installDir "circadiand.exe") -Force
Copy-Item $icon (Join-Path $assetsDir "circadian.ico") -Force

$startMenu = [Environment]::GetFolderPath("Programs")
$shortcutPath = Join-Path $startMenu "Circadian.lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = Join-Path $installDir "circadian.exe"
$shortcut.WorkingDirectory = $installDir
$shortcut.IconLocation = Join-Path $assetsDir "circadian.ico"
$shortcut.Description = "Circadian sleep tracker"
$shortcut.Save()

Start-Process (Join-Path $installDir "circadiand.exe")
Start-Sleep -Milliseconds 500
Start-Process (Join-Path $installDir "circadian.exe")

Write-Host "Installed to $installDir"
Write-Host "Start menu shortcut: $shortcutPath"
