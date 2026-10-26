param(
    [string]$Executable = 'target/release/pike.exe',
    [string]$OutputDirectory = 'target/package-windows-x64',
    [string]$TargetDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target')
)

$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
if ((Test-Path -LiteralPath $output) -or (Test-Path -LiteralPath ($output + '.zip'))) {
    throw 'Choose a fresh package output directory'
}
# Package an already-built release executable; building and network restoration
# remain explicit steps. The runtime script verifies cached bytes before copying.
& "$PSScriptRoot/prepare-conpty.ps1" -TargetDirectory $TargetDirectory -DestinationDirectories @($output) | Out-Null
Copy-Item -LiteralPath $executablePath -Destination (Join-Path $output 'pike.exe')
Compress-Archive -Path (Join-Path $output '*') -DestinationPath ($output + '.zip')
Get-FileHash -Algorithm SHA256 -LiteralPath ($output + '.zip')
