param([string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
foreach ($renderer in @('wgpu', 'tiny-skia')) {
    $arguments = @('--locked', '--target', 'x86_64-pc-windows-msvc',
        '--no-default-features', '--features', "renderer-$renderer")
    $tree = @(cargo tree @arguments --charset ascii -e normal,build --prefix none --format '{p}|{f}')
    if ($LASTEXITCODE -ne 0) { throw "cargo tree failed for $renderer" }
    $features = @{}
    foreach ($line in $tree) {
        if ($line -match '^([\w-]+) v[^|]+\|([^ ]*)') {
            $features[$Matches[1]] = @($Matches[2] -split ',' | Where-Object { $_ })
        }
    }
    foreach ($package in @('iced', 'iced_futures', 'iced_renderer', 'iced_term', 'tokio')) {
        if (-not $features.ContainsKey($package)) { throw "Missing $package in $renderer graph" }
    }
    $other = if ($renderer -eq 'wgpu') { 'tiny-skia' } else { 'wgpu' }
    foreach ($feature in @('default', 'thread-pool', 'linux-theme-detection', 'x11', 'wayland', 'web-colors', $other)) {
        if ($features['iced'] -contains $feature) { throw "Unexpected iced/$feature in $renderer" }
    }
    if ($features['iced'] -notcontains $renderer) { throw "Missing iced/$renderer" }
    if ($features['iced_futures'] -contains 'thread-pool') { throw 'Iced thread pool was re-enabled' }
    if ($features['iced_term'].Count -ne 0) { throw 'Vendor compatibility defaults were re-enabled' }
    foreach ($feature in @('full', 'fs', 'io-std', 'io-util', 'net', 'process', 'signal', 'macros', 'test-util')) {
        if ($features['tokio'] -contains $feature) { throw "Unexpected tokio/$feature" }
    }
    if ($renderer -eq 'tiny-skia' -and $features.ContainsKey('wgpu')) { throw 'wgpu leaked into tiny-skia profile' }
    if ($renderer -eq 'wgpu' -and $features.ContainsKey('iced_tiny_skia')) { throw 'tiny-skia leaked into wgpu profile' }
    if ($OutputDirectory) {
        New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
        $repository = (Get-Location).Path
        $tree | ForEach-Object { $_.Replace($repository, '<repository>') } |
            Set-Content (Join-Path $OutputDirectory "$renderer-resolved.txt") -Encoding UTF8
        cargo tree @arguments --charset ascii -e features |
            ForEach-Object { $_.Replace($repository, '<repository>') } |
            Set-Content (Join-Path $OutputDirectory "$renderer-features.txt") -Encoding UTF8
        if ($LASTEXITCODE -ne 0) { throw 'Could not save feature graph' }
    }
    Write-Output "$renderer feature isolation passed"
}
