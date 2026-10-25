$ErrorActionPreference = 'Stop'

# Feed synthetic Cargo output through the real guard without changing manifests
# or fetching/building dependencies. Earlier versions must not be overwritten.
function Test-FeatureGuard {
    param([string]$Name, [string[]]$ExtraRows = @(), [string]$ExpectedError = '')

    function cargo {
        $global:LASTEXITCODE = 0
        $renderer = if ($args -contains 'renderer-tiny-skia') { 'tiny-skia' } else { 'wgpu' }
        $ExtraRows
        "iced v0.14.0|advanced,canvas,crisp,lazy,tokio,$renderer"
        'iced_futures v0.14.0|tokio'
        "iced_renderer v0.14.0|$renderer"
        'iced_term v0.8.0 (<repository>/vendor/iced_term)|'
        'tokio v1.54.1|default,rt,rt-multi-thread,sync,time'
    }

    $failure = ''
    try {
        & (Join-Path $PSScriptRoot 'test-feature-profiles.ps1') | Out-Null
    } catch {
        $failure = $_.Exception.Message
    }
    if (($ExpectedError -eq '' -and $failure -ne '') -or
        ($ExpectedError -ne '' -and $failure -notlike "*$ExpectedError*")) {
        throw "FAIL: $Name (error='$failure', expected='$ExpectedError')"
    }
    Write-Output "PASS: $Name"
}

Test-FeatureGuard -Name 'isolated profiles pass'
Test-FeatureGuard -Name 'rejects full on an earlier Tokio version' -ExtraRows @('tokio v1.50.0|full,rt,sync') -ExpectedError 'Unexpected tokio/full'
Test-FeatureGuard -Name 'rejects defaults on an earlier Iced version' -ExtraRows @('iced v0.13.0|default,wgpu') -ExpectedError 'Unexpected iced/default'
Test-FeatureGuard -Name 'rejects a renderer dependency leak' -ExtraRows @('wgpu v27.0.1|default') -ExpectedError 'wgpu leaked into tiny-skia profile'
