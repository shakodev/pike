param([string]$TargetDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target'))

$ErrorActionPreference = 'Stop'
$manifest = Get-Content -Raw (Join-Path $PSScriptRoot 'conpty-runtime.json') | ConvertFrom-Json
$package = Join-Path $TargetDirectory ('conpty-runtime/' + $manifest.package + '.' + $manifest.version + '.nupkg')
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('pike runtime ' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $destination = Join-Path $scratch 'runtime with spaces'
    # Verify offline restoration, idempotence, and repair of a modified asset.
    & "$PSScriptRoot/prepare-conpty.ps1" -TargetDirectory $TargetDirectory -DestinationDirectories @($destination) -PackagePath $package | Out-Null
    [IO.File]::WriteAllText((Join-Path $destination 'conpty.dll'), 'synthetic corruption')
    & "$PSScriptRoot/prepare-conpty.ps1" -TargetDirectory $TargetDirectory -DestinationDirectories @($destination) -PackagePath $package | Out-Null
    foreach ($asset in $manifest.files) {
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $destination $asset.name)).Hash -ne $asset.sha256) {
            throw 'Offline runtime restoration failed'
        }
    }
    if (-not (Test-Path (Join-Path $destination 'ConPTY-LICENSE.txt'))) { throw 'Missing runtime license' }
    $corrupt = Join-Path $scratch 'corrupt.nupkg'
    [IO.File]::WriteAllText($corrupt, 'synthetic corrupt archive')
    $rejected = $false
    try {
        & "$PSScriptRoot/prepare-conpty.ps1" -TargetDirectory $TargetDirectory -DestinationDirectories @((Join-Path $scratch 'rejected')) -PackagePath $corrupt | Out-Null
    } catch {
        if ($_.Exception.Message -notmatch 'checksum mismatch') { throw }
        $rejected = $true
    }
    if (-not $rejected -or (Test-Path (Join-Path $scratch 'rejected'))) { throw 'Corrupt archive was deployed' }
    # A user-provided executable must match the native runtime architecture.
    # Use a controlled ARM64 PE header; ordinary release CI packages real x64 Pike.
    $armExecutable = Join-Path $scratch 'arm64.exe'
    $header = New-Object byte[] 512
    [BitConverter]::GetBytes([uint16]0x5A4D).CopyTo($header, 0)
    [BitConverter]::GetBytes([uint32]128).CopyTo($header, 0x3C)
    [BitConverter]::GetBytes([uint32]0x00004550).CopyTo($header, 128)
    [BitConverter]::GetBytes([uint16]0xAA64).CopyTo($header, 132)
    [IO.File]::WriteAllBytes($armExecutable, $header)
    foreach ($invalidExecutable in @($armExecutable, $corrupt, (Join-Path $destination 'conpty.dll'))) {
        $rejected = $false
        $packageDirectory = Join-Path $scratch ([Guid]::NewGuid().ToString())
        try {
            & "$PSScriptRoot/package-windows.ps1" -Executable $invalidExecutable -OutputDirectory $packageDirectory -TargetDirectory $TargetDirectory | Out-Null
        } catch {
            if ($_.Exception.Message -notmatch 'Expected an x64 Windows executable') { throw }
            $rejected = $true
        }
        if (-not $rejected -or (Test-Path $packageDirectory) -or (Test-Path ($packageDirectory + '.zip'))) {
            throw 'Invalid executable reached release packaging'
        }
    }
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
Write-Host 'ConPTY deployment checks passed'
