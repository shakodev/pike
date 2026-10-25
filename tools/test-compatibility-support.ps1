$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'compatibility-support.ps1')

function Expect-Rejection {
    param([scriptblock]$Action, [string]$Message)
    $rejected = $false
    try { & $Action } catch {
        if ($_.Exception.Message -notlike "*$Message*") { throw }
        $rejected = $true
    }
    if (-not $rejected) { throw "Expected rejection: $Message" }
    Write-Output "PASS: rejects $Message"
}

Initialize-CompatibilityDiagnostics
foreach ($length in @(0, 32, 1048576)) {
    $reader = New-Object System.IO.StringReader(('x' * $length))
    try {
        $task = Read-CompatibilityStderr -Reader $reader
        $prefix = $task.GetAwaiter().GetResult()
        if ($prefix.Length -ne [Math]::Min($length, 4096) -or $reader.Peek() -ne -1) {
            throw 'Capped stderr reader failed to retain a bounded prefix and drain the rest.'
        }
    } finally { $reader.Dispose() }
}
Write-Output 'PASS: stderr prefix is bounded and oversized input is fully drained'

# Exercise a real pipe too: retaining a prefix must not leave the writer blocked.
$hostExe = Join-Path $PSHOME 'pwsh.exe'
if (-not (Test-Path -LiteralPath $hostExe)) { $hostExe = Join-Path $PSHOME 'powershell.exe' }
$startInfo = New-Object System.Diagnostics.ProcessStartInfo
$startInfo.FileName = $hostExe
$startInfo.Arguments = '-NoLogo -NoProfile -NonInteractive -Command "[Console]::Error.Write((''x'' * 1048576))"'
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$startInfo.RedirectStandardError = $true
$producer = [System.Diagnostics.Process]::Start($startInfo)
try {
    $capture = Read-CompatibilityStderr -Reader $producer.StandardError
    if (-not $producer.WaitForExit(10000) -or -not $capture.Wait(5000)) { throw 'Stderr pipe did not drain.' }
    if ($producer.ExitCode -ne 0 -or $capture.GetAwaiter().GetResult().Length -ne 4096) { throw 'Unexpected pipe capture result.' }
} finally {
    if (-not $producer.HasExited) { $producer.Kill(); $producer.WaitForExit() }
    $producer.Dispose()
}
Write-Output 'PASS: 1 MiB stderr producer exits without a full-pipe stall'

$baseline = Join-Path $PSScriptRoot '../docs/baselines/issue-3'
foreach ($folder in Get-ChildItem -LiteralPath $baseline -Directory) {
    $tracePath = Join-Path $folder.FullName 'events.csv'
    if (-not (Test-Path -LiteralPath $tracePath)) { continue }
    $summaryPath = Join-Path $folder.FullName 'summary.json'
    $sessions = 1
    $mode = 'smoke'
    if (Test-Path -LiteralPath $summaryPath) {
        $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
        $sessions = $summary.Sessions
        $mode = $summary.Mode
    }
    Test-CompatibilityTrace -Events @(Import-Csv -LiteralPath $tracePath) -Sessions $sessions -Mode $mode
}
Write-Output 'PASS: all retained baseline traces satisfy scenario checks'

$trace = @(Import-Csv -LiteralPath (Join-Path $baseline 'cmd-2-smoke-01/events.csv'))
Expect-Rejection { Test-CompatibilityTrace -Events @($trace | Where-Object event -ne 'swap') -Sessions 2 -Mode smoke } 'swap between screenshots'
Expect-Rejection { Test-CompatibilityTrace -Events @($trace | Where-Object event -ne 'screenshot') -Sessions 2 -Mode smoke } 'screenshot observations'
Expect-Rejection { Test-CompatibilityTrace -Events @($trace | Where-Object event -ne 'size') -Sessions 2 -Mode smoke } 'before/after dimensions'

$unchanged = @($trace | ForEach-Object { $_ | Select-Object * })
foreach ($sessionId in 1..2) {
    $sizes = @($unchanged | Where-Object { $_.event -eq 'size' -and [int]$_.session -eq $sessionId })
    $sizes[-1].a = $sizes[0].a
    $sizes[-1].b = $sizes[0].b
}
Expect-Rejection { Test-CompatibilityTrace -Events $unchanged -Sessions 2 -Mode smoke } 'Dimensions did not change'

$wrongExit = @($trace | ForEach-Object { $_ | Select-Object * })
($wrongExit | Where-Object { $_.event -eq 'child_exit' -and $_.session -eq '2' }).a = '7'
Expect-Rejection { Test-CompatibilityTrace -Events $wrongExit -Sessions 2 -Mode smoke } 'controlled exit code'

$duplicateExit = @($trace | ForEach-Object { $_ | Select-Object * })
($duplicateExit | Where-Object { $_.event -eq 'exit' -and $_.session -eq '2' }).session = '1'
Expect-Rejection { Test-CompatibilityTrace -Events $duplicateExit -Sessions 2 -Mode smoke } 'duplicate exit'

$wrongFocus = @($trace | ForEach-Object { $_ | Select-Object * })
($wrongFocus | Where-Object { $_.event -eq 'exit' -and $_.session -eq '2' }).a = '2'
Expect-Rejection { Test-CompatibilityTrace -Events $wrongFocus -Sessions 2 -Mode smoke } 'while session 1 was focused'

# A trace can be complete even when the unpatched backend misses final markers.
Test-CompatibilityTrace -Events @($trace | Where-Object event -ne 'final') -Sessions 2 -Mode smoke
Write-Output 'PASS: missing final markers remain baseline observations'
