param(
    [string]$Executable = 'target/release/pike.exe',
    [string]$OutputDirectory = 'target/package-windows-x64',
    [string]$TargetDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target')
)

$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
# The bundled runtime is x64. Reject another architecture or a DLL before
# downloading anything or creating a misleading, unusable release package.
$reader = [IO.BinaryReader]::new([IO.File]::OpenRead($executablePath))
try {
    $invalid = 'Expected an x64 Windows executable for the pinned ConPTY runtime'
    if ($reader.BaseStream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { throw $invalid }
    $reader.BaseStream.Position = 0x3C
    $peOffset = $reader.ReadUInt32()
    if ($peOffset -gt ($reader.BaseStream.Length - 26)) { throw $invalid }
    $reader.BaseStream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) { throw $invalid }
    $reader.BaseStream.Position = $peOffset + 22
    $characteristics = $reader.ReadUInt16()
    if (($characteristics -band 0x2000) -ne 0 -or ($characteristics -band 0x0002) -eq 0) { throw $invalid }
    if ($reader.ReadUInt16() -ne 0x020B) { throw $invalid }
} finally {
    $reader.Dispose()
}
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
