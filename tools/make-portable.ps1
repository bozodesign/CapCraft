# Makes zip files of CapCraft to give to friends: unzip and run, nothing to install.
#
#   powershell -ExecutionPolicy Bypass -File tools\make-portable.ps1               # the app
#   powershell -ExecutionPolicy Bypass -File tools\make-portable.ps1 -WithCaptions # also the auto-captions pack (about 3 GB)
#
# Writes into dist\:
#   CapCraft-<version>-windows.zip            CapCraft.exe, ffmpeg.exe, ffprobe.exe, read-me and licences
#   CapCraft-<version>-auto-captions.zip      the whisper folder (speech engine and models), optional
param(
    [switch]$WithCaptions,
    [switch]$NoBuild,
    # where ffmpeg.exe and ffprobe.exe (a static build) are taken from
    [string]$FFmpegDir = "C:\ffmpeg"
)
$ErrorActionPreference = "Stop"
$app = Split-Path -Parent $PSScriptRoot
Set-Location $app

$version = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$name = "CapCraft-$version-windows"
$dist = Join-Path $app "dist"
$stage = Join-Path $dist $name

if (-not $NoBuild) {
    $env:PATH = "$HOME\.cargo\bin;$env:PATH"
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "The build failed." }
}
$exe = Join-Path $app "target\release\CapCraft.exe"
if (-not (Test-Path $exe)) { throw "target\release\CapCraft.exe is missing: build first." }
foreach ($t in "ffmpeg.exe", "ffprobe.exe") {
    if (-not (Test-Path (Join-Path $FFmpegDir $t))) { throw "$t was not found in $FFmpegDir (use -FFmpegDir)." }
}

# a fresh folder for this version
New-Item -ItemType Directory -Force $dist | Out-Null
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
$lic = New-Item -ItemType Directory -Force (Join-Path $stage "licences")

Copy-Item $exe $stage
Copy-Item (Join-Path $FFmpegDir "ffmpeg.exe") $stage
Copy-Item (Join-Path $FFmpegDir "ffprobe.exe") $stage
Copy-Item (Join-Path $app "tools\portable-readme.txt") (Join-Path $stage "README.txt")
Copy-Item (Join-Path $app "LICENSE") (Join-Path $lic "CapCraft-MIT.txt")
Copy-Item (Join-Path $app "CREDITS.md") (Join-Path $lic "CREDITS.md")
Copy-Item (Join-Path $app "assets\fonts\OFL-Kanit.txt") (Join-Path $lic "Kanit-OFL.txt")
$gpl = Get-ChildItem -Path $FFmpegDir -Recurse -Filter "LICENSE*.txt" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($gpl) { Copy-Item $gpl.FullName (Join-Path $lic "FFmpeg-GPLv3.txt") }
$ffver = (& (Join-Path $FFmpegDir "ffmpeg.exe") -hide_banner -version | Select-Object -First 1)
Set-Content -Encoding utf8 (Join-Path $lic "FFmpeg.txt") @(
    $ffver,
    "FFmpeg is free software under the GNU General Public License version 3 (FFmpeg-GPLv3.txt).",
    "Source code: https://ffmpeg.org/download.html   This build: https://www.gyan.dev/ffmpeg/builds/"
)

$zip = Join-Path $dist "$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
# Windows' own tar writes zip files of any size
tar.exe -a -c -f $zip -C $dist $name
if ($LASTEXITCODE -ne 0) { throw "Making the zip failed." }
Write-Host ("Made {0} ({1:N0} MB)" -f $zip, ((Get-Item $zip).Length / 1MB))

if ($WithCaptions) {
    $w = Join-Path $app "whisper"
    if (-not (Test-Path (Join-Path $w "cuda\whisper-cli.exe"))) { throw "The whisper folder with cuda\whisper-cli.exe is missing." }
    $cname = "CapCraft-$version-auto-captions"
    $czip = Join-Path $dist "$cname.zip"
    if (Test-Path $czip) { Remove-Item -Force $czip }
    tar.exe -a -c -f $czip -C $app "whisper/cuda" "whisper/models" "whisper/models-general"
    if ($LASTEXITCODE -ne 0) { throw "Making the captions zip failed." }
    Write-Host ("Made {0} ({1:N0} MB)" -f $czip, ((Get-Item $czip).Length / 1MB))
}
