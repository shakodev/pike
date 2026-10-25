param(
    [ValidateSet('cmd', 'powershell', 'pwsh')][string]$Shell = 'cmd',
    [ValidateRange(1, 2)][int]$Sessions = 2,
    [ValidateSet('smoke', 'idle', 'startup')][string]$Mode = 'smoke',
    [ValidateRange(1, 100)][int]$Runs = 20,
    [string]$OutputDirectory = 'target/compatibility/results'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'compatibility-support.ps1')
# Compile the tiny BCL-only reader before starting any measured stopwatch.
Initialize-CompatibilityDiagnostics
$Shell = $Shell.ToLowerInvariant()
$Mode = $Mode.ToLowerInvariant()
$exe = (Resolve-Path -LiteralPath 'target/release/examples/compatibility.exe').Path
$rootOutput = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $rootOutput | Out-Null
if (-not (Get-Command "$Shell.exe" -ErrorAction SilentlyContinue)) {
    throw "SKIPPED: $Shell.exe is not installed. Choose an available shell."
}
$totalRuns = if ($Mode -eq 'startup') { $Runs } else { 1 }
$summaries = @(for ($run = 1; $run -le $totalRuns; $run++) {
    $directory = Join-Path $rootOutput ("{0}-{1}-{2}-{3:d2}" -f $Shell, $Sessions, $Mode, $run)
    New-Item -ItemType Directory -Force $directory | Out-Null
    # No process-global environment mutation or shell evaluation is needed.
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $exe
    $harnessMode = if ($Mode -eq 'smoke') { 'smoke' } else { 'idle' }
    $startInfo.Arguments = '--shell {0} --sessions {1} --mode {2} --output "{3}"' -f $Shell, $Sessions, $harnessMode, $directory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $startInfo.RedirectStandardError = $true
    $startInfo.EnvironmentVariables['ICED_BACKEND'] = 'wgpu'
    $appProcess = New-Object System.Diagnostics.Process
    $appProcess.StartInfo = $startInfo
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    if (-not $appProcess.Start()) { throw 'Could not start compatibility harness.' }
    $stderr = Read-CompatibilityStderr -Reader $appProcess.StandardError
    try {
        $eventsPath = Join-Path $directory 'events.csv'
        do {
            Start-Sleep -Milliseconds 20
            $appProcess.Refresh()
            if ($appProcess.HasExited) { throw 'Harness exited before fixtures became ready.' }
            $ready = @()
            if (Test-Path -LiteralPath $eventsPath) {
                $ready = @(Import-Csv -LiteralPath $eventsPath | Where-Object { $_.event -eq 'ready' -and $_.a -match '^\d+$' })
            }
        } while (($ready.Count -ne $Sessions -or $appProcess.MainWindowTitle -ne 'Pike compatibility harness') -and $watch.Elapsed.TotalSeconds -lt 20)
        if ($ready.Count -ne $Sessions -or $appProcess.MainWindowTitle -ne 'Pike compatibility harness') { throw 'Window/fixture readiness timed out.' }
        $readyMs = $watch.Elapsed.TotalMilliseconds
        $fixturePids = @($ready | ForEach-Object { [int]$_.a })

        # Discover only the owned tree's PIDs/names, never command lines or env.
        $processRows = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name | Select-Object ProcessId, ParentProcessId, Name
        $ownedPids = @($appProcess.Id)
        do {
            $newPids = @($processRows | Where-Object { $ownedPids -contains [int]$_.ParentProcessId -and $ownedPids -notcontains [int]$_.ProcessId } | ForEach-Object { [int]$_.ProcessId })
            $ownedPids += $newPids
        } while ($newPids.Count -gt 0)
        $processRows | Where-Object { $ownedPids -contains [int]$_.ProcessId } | Export-Csv (Join-Path $directory 'process-tree.csv') -NoTypeInformation
        $childPids = @($ownedPids | Where-Object { $_ -ne $appProcess.Id })

        if ($Mode -eq 'idle') {
            Start-Sleep -Seconds 10
            $idleWatch = [System.Diagnostics.Stopwatch]::StartNew()
            $samples = @(for ($sample = 0; $sample -le 60; $sample++) {
                if ($sample -gt 0) { Start-Sleep -Seconds 1 }
                $appProcess.Refresh()
                if ($appProcess.HasExited) { throw 'Harness exited during idle measurement.' }
                $childrenPrivate = 0L
                $childrenCpu = 0.0
                foreach ($childId in $childPids) {
                    $childProcess = Get-Process -Id $childId -ErrorAction SilentlyContinue
                    if ($childProcess) {
                        try {
                            $childrenPrivate += $childProcess.PrivateMemorySize64
                            $childrenCpu += $childProcess.TotalProcessorTime.TotalSeconds
                        } finally { $childProcess.Dispose() }
                    }
                }
                [pscustomobject]@{
                    Seconds = $idleWatch.Elapsed.TotalSeconds
                    CpuSeconds = $appProcess.TotalProcessorTime.TotalSeconds
                    PrivateBytes = $appProcess.PrivateMemorySize64
                    WorkingSetBytes = $appProcess.WorkingSet64
                    Threads = $appProcess.Threads.Count
                    Handles = $appProcess.HandleCount
                    Responding = $appProcess.Responding
                    ChildrenPrivateBytes = $childrenPrivate
                    ChildrenCpuSeconds = $childrenCpu
                }
            })
            $samples | Export-Csv (Join-Path $directory 'idle.csv') -NoTypeInformation
        }
        $closeWatch = [System.Diagnostics.Stopwatch]::StartNew()
        if ($Mode -ne 'smoke' -and -not $appProcess.CloseMainWindow()) { throw 'Window close request failed.' }
        if (-not $appProcess.WaitForExit(15000)) { throw 'Harness did not exit within 15 seconds.' }
        $appProcess.WaitForExit()
        $closeMs = $closeWatch.Elapsed.TotalMilliseconds
        if ($appProcess.ExitCode -ne 0) { throw "Harness exited with code $($appProcess.ExitCode)." }
        $events = @(Import-Csv -LiteralPath $eventsPath)
        Test-CompatibilityTrace -Events $events -Sessions $Sessions -Mode $Mode
        $liveFixtures = @($fixturePids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue } | Where-Object ProcessName -eq 'pty_fixture')
        if ($liveFixtures.Count -gt 0) { throw 'Fixture process remained alive after harness exit.' }
        $summary = [ordered]@{
            Shell = $Shell
            Sessions = $Sessions
            Mode = $Mode
            Run = $run
            Renderer = 'wgpu (ICED_BACKEND override); default compiled backend features'
            ReadyAndWindowMs = $readyMs
            CloseObservationMs = $closeMs
            FinalMarkers = @($events | Where-Object event -eq 'final').Count
            ChildExitCodes = @($events | Where-Object event -eq 'child_exit' | ForEach-Object { [int]$_.a })
            ExitCode = $appProcess.ExitCode
        }
        if ($Mode -eq 'idle') {
            $first = $samples[0]
            $last = $samples[-1]
            if (@($samples | Where-Object { -not $_.Responding }).Count -gt 0) { throw 'Window became unresponsive.' }
            $wall = $last.Seconds - $first.Seconds
            $summary.IdleWallSeconds = $wall
            $summary.IdleOneCoreCpuPercent = 100 * ($last.CpuSeconds - $first.CpuSeconds) / $wall
            $summary.ChildrenOneCoreCpuPercent = 100 * ($last.ChildrenCpuSeconds - $first.ChildrenCpuSeconds) / $wall
            foreach ($field in @('PrivateBytes','WorkingSetBytes','Threads','Handles','ChildrenPrivateBytes')) {
                $summary["${field}Minimum"] = ($samples.$field | Measure-Object -Minimum).Minimum
                $summary["${field}Maximum"] = ($samples.$field | Measure-Object -Maximum).Maximum
            }
        }
        $summary | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $directory 'summary.json') -Encoding UTF8
        [pscustomobject]$summary
    } finally {
        if (-not $appProcess.HasExited) { $appProcess.Kill(); $appProcess.WaitForExit() }
        $errorText = $stderr.GetAwaiter().GetResult()
        [System.IO.File]::WriteAllText((Join-Path $directory 'stderr.txt'), $errorText)
        $appProcess.Dispose()
    }
})
$summaries | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $rootOutput "$Shell-$Sessions-$Mode.json") -Encoding UTF8
$summaries | Format-Table Shell,Sessions,Mode,Run,ReadyAndWindowMs,FinalMarkers,ExitCode
