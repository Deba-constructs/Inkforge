# INKFORGE — Windows release packaging (run on Windows with the MSVC toolchain).
#
#   powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
#
# Builds the optimised Windows executable, stages a portable folder under dist\
# and zips it as dist\INKFORGE-<version>-windows-x86_64.zip. Nothing outside
# dist\ is written except Cargo's own target\ output.

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"').Matches[0].Groups[1].Value
if (-not $version) { throw "could not read the version from Cargo.toml" }

$target = "x86_64-pc-windows-msvc"
$exe = "target\$target\release\inkforge.exe"
$stage = "dist\INKFORGE-$version-windows-x86_64"
$zip = "$stage.zip"

Write-Host "INKFORGE $version — building $target release"
cargo build --release --target $target
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
if (-not (Test-Path $exe)) { throw "expected $exe to exist" }

if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

# The portable build is one self-contained executable plus the paperwork.
Copy-Item $exe "$stage\INKFORGE.exe"
Copy-Item "packaging\README.txt" "$stage\README.txt"
Copy-Item "LICENSE-MIT" "$stage\LICENSE-MIT.txt"

if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host ""
Write-Host "Portable build: $stage\INKFORGE.exe"
Write-Host "Archive:        $zip"
Write-Host ("Archive size:   {0:N1} MB" -f ((Get-Item $zip).Length / 1MB))
