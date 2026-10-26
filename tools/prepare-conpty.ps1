param(
    [string]$TargetDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target'),
    [string[]]$DestinationDirectories = @(),
    [string]$PackagePath
)

$ErrorActionPreference = 'Stop'
$manifestPath = Join-Path $PSScriptRoot 'conpty-runtime.json'
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$cache = Join-Path ([IO.Path]::GetFullPath($TargetDirectory)) 'conpty-runtime'
New-Item -ItemType Directory -Force -Path $cache | Out-Null
if (-not $PackagePath) {
    $PackagePath = Join-Path $cache ($manifest.package + '.' + $manifest.version + '.nupkg')
    if (-not (Test-Path -LiteralPath $PackagePath)) {
        # Download to a separate file so interruption never leaves a valid cache entry.
        $download = Join-Path $cache ([Guid]::NewGuid().ToString() + '.download')
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $manifest.url -OutFile $download
            if ((Get-FileHash -Algorithm SHA256 -LiteralPath $download).Hash -ne $manifest.sha256) {
                throw 'ConPTY package checksum mismatch'
            }
            Move-Item -LiteralPath $download -Destination $PackagePath -Force
        } finally {
            if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download }
        }
    }
}
$PackagePath = (Resolve-Path -LiteralPath $PackagePath).Path
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $PackagePath).Hash -ne $manifest.sha256) {
    throw 'ConPTY package checksum mismatch; remove the corrupt cache or supply the pinned package'
}
if ($DestinationDirectories.Count -eq 0) {
    foreach ($profileName in @('debug', 'release')) {
        $profileDirectory = Join-Path $TargetDirectory $profileName
        $DestinationDirectories += $profileDirectory
        $DestinationDirectories += Join-Path $profileDirectory 'deps'
        $DestinationDirectories += Join-Path $profileDirectory 'examples'
    }
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$staging = Join-Path $cache ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    $archive = [IO.Compression.ZipFile]::OpenRead($PackagePath)
    try {
        foreach ($asset in $manifest.files) {
            $entry = $archive.GetEntry($asset.entry)
            if ($null -eq $entry) { throw "ConPTY package missing $($asset.entry)" }
            $destination = Join-Path $staging $asset.name
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $destination)
            if ((Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash -ne $asset.sha256) {
                throw "ConPTY asset checksum mismatch: $($asset.name)"
            }
        }
    } finally {
        $archive.Dispose()
    }
    # Only a fully verified pair reaches executable directories. No NuGet client,
    # .NET application runtime, SDK import library, or other architecture is shipped.
    foreach ($directory in $DestinationDirectories) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
        foreach ($asset in $manifest.files) {
            Copy-Item -LiteralPath (Join-Path $staging $asset.name) -Destination $directory -Force
        }
        Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $directory 'conpty-runtime.json') -Force
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../docs/dependencies/conpty/LICENSE') -Destination (Join-Path $directory 'ConPTY-LICENSE.txt') -Force
    }
} finally {
    Remove-Item -LiteralPath $staging -Recurse -Force
}
[pscustomobject]@{
    Package = $manifest.package
    Version = $manifest.version
    PackageSHA256 = $manifest.sha256
    NativeBytes = ($manifest.files | Measure-Object -Property bytes -Sum).Sum
    Destinations = $DestinationDirectories
}
