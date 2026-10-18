# Lays out the MSIX package for the Lockdown desktop app and packs it,
# unsigned (docs/plan/24-release.md §1). CI runs it on every pull request
# as a dry run: makeappx validates the manifest and the layout.
#
#   pwsh packaging/msix/build-layout.ps1 -Version 0.1.0 -AppExe <lockdown-desktop.exe> -HostExe <lockdown-native-host.exe> -Out <dir>
#
# The package holds the app and the browser host side by side, as the
# installers do, because the app looks for the host beside itself.
param(
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $true)][string]$AppExe,
  [Parameter(Mandatory = $true)][string]$HostExe,
  [Parameter(Mandatory = $true)][string]$Out,
  [string]$Publisher = "CN=Sean Sands"
)
$ErrorActionPreference = "Stop"

if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "build-layout: version '$Version' isn't x.y.z" }
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$icons = Join-Path $here "..\..\apps\desktop\src-tauri\icons"
$layout = Join-Path $Out "layout"
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force -Path (Join-Path $layout "Assets") | Out-Null

Copy-Item $AppExe (Join-Path $layout "lockdown-desktop.exe")
Copy-Item $HostExe (Join-Path $layout "lockdown-native-host.exe")
foreach ($asset in "StoreLogo.png", "Square150x150Logo.png", "Square44x44Logo.png") {
  Copy-Item (Join-Path $icons $asset) (Join-Path $layout "Assets\$asset")
}
$manifest = Get-Content -Raw (Join-Path $here "AppxManifest.xml")
$manifest = $manifest.Replace("@VERSION@", "$Version.0").Replace("@PUBLISHER@", $Publisher)
Set-Content -NoNewline -Encoding utf8 -Path (Join-Path $layout "AppxManifest.xml") -Value $manifest

$kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
$makeappx = Get-ChildItem -Path $kits -Recurse -Filter makeappx.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -match '\\x64\\' } |
  Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw "build-layout: makeappx.exe not found under $kits" }
$msix = Join-Path $Out "Lockdown_${Version}_x64.msix"
& $makeappx.FullName pack /o /d $layout /p $msix
if ($LASTEXITCODE -ne 0) { throw "build-layout: makeappx failed ($LASTEXITCODE)" }
Write-Output "build-layout: $msix (unsigned)"
