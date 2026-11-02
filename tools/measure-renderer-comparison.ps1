# Native Windows collector. A finite external observer, never a product service.
param(
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Python = 'python',
    [ValidateRange(0, 10000)][int]$FirstRun = 0,
    [ValidateRange(1, 10000)][int]$RunCount = 10000
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Native Windows required; no cross-host performance claim.' }
. (Join-Path $PSScriptRoot 'compatibility-support.ps1')
Initialize-CompatibilityDiagnostics
$plan = Get-Content -Raw -LiteralPath $Manifest | ConvertFrom-Json
if ($plan.schema -ne 1 -or $plan.runs.Count -lt 1 -or $plan.runs.Count -gt 10000) { throw 'Unknown or oversized manifest' }
$root = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $root) { throw 'Use a fresh evidence directory' }
New-Item -ItemType Directory $root | Out-Null
Copy-Item -LiteralPath $Manifest -Destination (Join-Path $root 'run-manifest.json')
# Preserve the existing feature/runtime guard unchanged and retain its trees.
& (Join-Path $PSScriptRoot 'test-feature-profiles.ps1') -OutputDirectory (Join-Path $root 'feature-guard')
if (-not $?) { throw 'Existing feature profile validation failed' }
$os = Get-CimInstance Win32_OperatingSystem -OperationTimeoutSec 5 | Select-Object Caption, Version, BuildNumber, OSArchitecture, TotalVisibleMemorySize
$cpu = @(Get-CimInstance Win32_Processor -OperationTimeoutSec 5 | Select-Object Name, NumberOfCores, NumberOfLogicalProcessors)
$gpu = @(Get-CimInstance Win32_VideoController -OperationTimeoutSec 5 | Select-Object Name, DriverVersion, AdapterRAM)
$power = @(powercfg /getactivescheme)
$fonts = @(Get-ChildItem -LiteralPath (Join-Path $env:WINDIR 'Fonts') -Filter 'consol*.ttf' | ForEach-Object {
    [ordered]@{ Name = $_.Name; Bytes = $_.Length; SHA256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash }
})
[ordered]@{ OS = $os; CPU = $cpu; GPU = $gpu; Power = $power; Fonts = $fonts
    Monitor = 'Operator must attach scale/refresh/monitor and display topology; do not infer from window geometry'
    Counters = 'PrivateMemorySize64, WorkingSet64, TotalProcessorTime, Threads.Count, HandleCount; CPU normalized to one core'
    GPUCounter = 'Unavailable unless gpu.csv from the explicit optional watcher is attached; absence is not zero'
    ActualRendererAdapter = 'Unavailable through lean pinned Iced API (sysinfo disabled); GPU device metadata is not proof of selected adapter'
} | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $root 'machine.json') -Encoding UTF8

function Read-OwnedTree([int]$AppId, [datetime]$Started) {
    # Names/IDs/timestamps only. Never query command lines, environment or argv.
    $rows = @(Get-CimInstance Win32_Process -OperationTimeoutSec 5 -Property ProcessId, ParentProcessId, Name, CreationDate)
    $owned = @($AppId)
    for ($depth = 0; $depth -lt 8; $depth++) {
        $next = @($rows | Where-Object { $owned -contains [int]$_.ParentProcessId -and $owned -notcontains [int]$_.ProcessId -and $_.CreationDate -ge $Started.AddSeconds(-1) } | ForEach-Object { [int]$_.ProcessId })
        if ($next.Count -eq 0) { break }
        $owned += $next
    }
    return @($rows | Where-Object { $owned -contains [int]$_.ProcessId })
}

function Open-VerifiedOwnedProcess($Row) {
    $process = Get-Process -Id $Row.ProcessId -ErrorAction SilentlyContinue
    if (-not $process) { throw 'Owned process vanished before identity verification' }
    # Pin the native process object before comparing its creation timestamp to
    # the ownership snapshot. WMI dates retain microsecond precision.
    $null = $process.Handle
    $difference = [Math]::Abs($process.StartTime.ToUniversalTime().Ticks - $Row.CreationDate.ToUniversalTime().Ticks)
    if ($difference -gt 10) { $process.Dispose(); throw 'Owned process creation identity changed' }
    return $process
}

for ($index = $FirstRun; $index -lt [Math]::Min($plan.runs.Count, $FirstRun + $RunCount); $index++) {
    $run = $plan.runs[$index]
    $directory = Join-Path $root ('run-{0:d4}' -f $index)
    New-Item -ItemType Directory $directory | Out-Null
    $run | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $directory 'run.json') -Encoding UTF8
    $exe = (Resolve-Path -LiteralPath $run.build.executable).Path
    if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant() -ne $run.build.fixture_sha256) { throw 'Fixture binary identity changed' }
    $runtime = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'conpty-runtime.json') | ConvertFrom-Json
    foreach ($file in $runtime.files) {
        $path = Join-Path (Split-Path $exe -Parent) $file.name
        if (-not (Test-Path -LiteralPath $path) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256 -or $run.build.conpty_files.($file.name) -ne $file.sha256) { throw 'Pinned ConPTY runtime identity changed or is missing' }
    }
    $fixture = Join-Path $directory 'fixture'
    if ($fixture.Contains('"')) { throw 'Unsupported output path quote' }
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $exe
    $info.Arguments = '--output "{0}" --visible {1} --hidden {2} --history {3} --workload {4} --seconds {5} --settle {6} --probes {7}' -f $fixture, $run.visible, $run.hidden, $run.history, $run.workload, $run.seconds, $run.settle, $run.probes
    $info.UseShellExecute = $false
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables['RUST_LOG'] = 'off'
    $info.EnvironmentVariables['ICED_BACKEND'] = $run.build.renderer
    $app = New-Object Diagnostics.Process
    $app.StartInfo = $info
    $ownedProcesses = @{}
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $stderr = $null
    $samples = [Collections.Generic.List[object]]::new()
    $success = $false
    $failure = $null
    $didStart = $false
    $gpuWatcher = $null
    $stderrFinished = $false
    try {
        if (-not $app.Start()) { throw 'Could not launch finite comparison fixture' }
        $didStart = $true
        $started = $app.StartTime
        $stderr = Read-CompatibilityStderr -Reader $app.StandardError -Limit 4096
        $nextSample = 0.0
        $discovered = $false
        $events = @()
        $sample = 0
        $endSampled = $false
        $startupMs = $null
        $scenarioReadyMs = $null
        while (-not $app.HasExited -and $watch.Elapsed.TotalSeconds -lt ($run.seconds + $run.settle + 115)) {
            Start-Sleep -Milliseconds 20
            $app.Refresh()
            $eventPath = Join-Path $fixture 'events.csv'
            if (Test-Path -LiteralPath $eventPath) {
                if ((Get-Item -LiteralPath $eventPath).Length -gt 2097152) { throw 'Oversized fixture metadata' }
                $events = @(Import-Csv -LiteralPath $eventPath)
            }
            $begin = @($events | Where-Object event -eq 'measure_begin')
            if ($null -eq $scenarioReadyMs -and @($events | Where-Object event -eq 'scenario_ready').Count -eq 1 -and $app.MainWindowHandle -ne [IntPtr]::Zero -and $app.MainWindowTitle.StartsWith('Pike renderer comparison')) {
                $scenarioReadyMs = $watch.Elapsed.TotalMilliseconds
            }
            if (-not $discovered -and @($events | Where-Object event -eq 'scenario_ready').Count -eq 1) {
                $discovered = $true
                $children = @($events | Where-Object event -eq 'ready' | ForEach-Object { [int]$_.a })
                $tree = Read-OwnedTree -AppId $app.Id -Started $started
                $tree | Select-Object ProcessId, ParentProcessId, Name, CreationDate | Export-Csv (Join-Path $directory 'discovered-tree.csv') -NoTypeInformation
                $verified = @()
                foreach ($row in $tree) {
                    $process = Open-VerifiedOwnedProcess -Row $row
                    $role = if ($process.Id -eq $app.Id) { 'app' } elseif ($children -contains $process.Id) { 'fixture-child' } elseif ($row.Name -in @('conhost.exe', 'OpenConsole.exe')) { 'conpty-host' } else { 'owned-other' }
                    $ownedProcesses[$process.Id] = @{ Process = $process; CreatedTicks = $process.StartTime.Ticks; Role = $role }
                    $verified += [pscustomobject]@{ ProcessId = $process.Id; ParentProcessId = $row.ParentProcessId; Name = $row.Name; CreatedTicks = $process.StartTime.Ticks; Role = $role }
                }
                $verified | Export-Csv (Join-Path $directory 'owned-tree.csv') -NoTypeInformation
                foreach ($childId in $children) { if (-not $ownedProcesses.ContainsKey($childId)) { throw 'Fixture child is outside verified owned tree' } }
            }
            $ended = @($events | Where-Object event -eq 'measure_end').Count -eq 1
            if ($discovered -and $begin.Count -eq 1 -and ($watch.Elapsed.TotalSeconds -ge $nextSample -or ($ended -and -not $endSampled))) {
                if ($null -eq $startupMs) { $startupMs = $watch.Elapsed.TotalMilliseconds }
                if (-not $gpuWatcher) {
                    # Separate test-owned observer, outside the app tree. Start
                    # at Begin, after ownership discovery during settling.
                    $watcherScript = Join-Path $PSScriptRoot 'watch-renderer-comparison-gpu.ps1'
                    $shellExe = (Get-Process -Id $PID).Path
                    $watcherArgs = '-NoProfile -File "{0}" -OwnedTree "{1}" -OutputDirectory "{2}" -Seconds {3}' -f $watcherScript, (Join-Path $directory 'owned-tree.csv'), $directory, $run.seconds
                    $gpuWatcher = Start-Process -FilePath $shellExe -ArgumentList $watcherArgs -PassThru -WindowStyle Hidden
                }
                $phase = if (-not $ended) { 'measure' } elseif (-not $endSampled) { 'end' } else { 'drain' }
                foreach ($entry in $ownedProcesses.Values) {
                    $process = $entry.Process
                    $process.Refresh()
                    if ($process.HasExited) { continue }
                    if ($process.StartTime.Ticks -ne $entry.CreatedTicks) { throw 'Owned PID identity changed' }
                    $samples.Add([pscustomobject]@{ Sample = $sample; Seconds = $watch.Elapsed.TotalSeconds.ToString('R', [Globalization.CultureInfo]::InvariantCulture); Phase = $phase; Role = $entry.Role
                        Pid = $process.Id; CreatedTicks = $entry.CreatedTicks; CpuSeconds = $process.TotalProcessorTime.TotalSeconds.ToString('R', [Globalization.CultureInfo]::InvariantCulture)
                        PrivateBytes = $process.PrivateMemorySize64; WorkingSetBytes = $process.WorkingSet64
                        Threads = $process.Threads.Count; Handles = $process.HandleCount })
                }
                $sample++
                if ($phase -eq 'end') { $endSampled = $true }
                if ($samples.Count -gt 32768) { throw 'External process sample bound reached' }
                $nextSample = $watch.Elapsed.TotalSeconds + 1
            }
        }
        if (-not $app.WaitForExit(3000)) { throw 'Fixture exceeded external deadline' }
        if ($app.ExitCode -ne 0) { throw "Fixture exit code $($app.ExitCode)" }
        foreach ($entry in $ownedProcesses.Values) { if (-not $entry.Process.HasExited) { throw 'Owned child/ConPTY process survived app exit' } }
        if (-not $discovered) { throw 'No measurement milestone observed' }
        $success = $true
    } catch { $failure = $_.Exception.Message }
    finally {
        # Retain reds before cleanup. Kill only retained, verified process objects;
        # never search by name or trust a stale PID without its creation identity.
        $samples | Export-Csv (Join-Path $directory 'process.csv') -NoTypeInformation
        if ($didStart -and -not $app.HasExited) {
            # Also discover test-owned children on a setup failure before Begin.
            try {
                foreach ($row in @(Read-OwnedTree -AppId $app.Id -Started $started)) {
                    if (-not $ownedProcesses.ContainsKey([int]$row.ProcessId)) {
                        $process = Open-VerifiedOwnedProcess -Row $row
                        if ($process) { $ownedProcesses[$process.Id] = @{ Process = $process; CreatedTicks = $process.StartTime.Ticks; Role = 'cleanup-only' } }
                    }
                }
            } catch { $failure = "Cleanup discovery unavailable: $($_.Exception.Message); prior=$failure"; $success = $false }
        }
        $survivors = @()
        foreach ($entry in $ownedProcesses.Values) {
            $process = $entry.Process
            try {
                if (-not $process.HasExited -and $process.StartTime.Ticks -eq $entry.CreatedTicks) { $process.Kill(); if (-not $process.WaitForExit(5000)) { $survivors += $process.Id } }
            } catch { $survivors += $process.Id; $success = $false
            } finally { $process.Dispose() }
        }
        if ($didStart -and -not $app.HasExited) { $app.Kill(); if (-not $app.WaitForExit(5000)) { $survivors += $app.Id } }
        if ($gpuWatcher) {
            if (-not $gpuWatcher.WaitForExit(5000)) {
                $gpuWatcher.Kill()
                if (-not $gpuWatcher.WaitForExit(5000)) { $survivors += $gpuWatcher.Id; $success = $false; $failure = 'GPU observer termination unverified' }
                [ordered]@{ Available = $false; Failure = 'GPU watcher exceeded final deadline; any partial values are incomplete' } | ConvertTo-Json | Set-Content (Join-Path $directory 'gpu-availability.json') -Encoding UTF8
            }
            $gpuWatcher.Dispose()
        }
        try {
            if ($stderr -and $stderr.Wait(5000)) { [IO.File]::WriteAllText((Join-Path $directory 'stderr.txt'), $stderr.GetAwaiter().GetResult()); $stderrFinished = $true }
            if (-not $stderrFinished) { $success = $false; $failure = "Bounded stderr reader did not finish; prior=$failure" }
        } catch { $success = $false; $failure = "Bounded stderr unavailable: $($_.Exception.GetType().Name); prior=$failure" }
        $exitCode = if ($didStart -and $app.HasExited) { $app.ExitCode } else { $null }
        [ordered]@{ Success = $success; Failure = $failure; Survivors = $survivors; ExternalBeginObservationMs = $startupMs; ExternalSceneSnapshotReadyMs = $scenarioReadyMs
            ExternalDurationMs = $watch.Elapsed.TotalMilliseconds; AppPid = $(if ($didStart) { $app.Id } else { $null }); AppCreatedTicks = $(if ($didStart) { $started.Ticks } else { $null }); AppExitCode = $exitCode; EndSampleObserved = $endSampled; StderrReaderFinished = $stderrFinished; Cleanup = 'verified owned process objects only'
        } | ConvertTo-Json | Set-Content (Join-Path $directory 'collector.json') -Encoding UTF8
        $app.Dispose()
    }
    if (-not $success) { throw "Run $index failed: $failure (raw evidence retained)" }
    & $Python (Join-Path $PSScriptRoot 'renderer-comparison.py') summarize $directory
    if ($LASTEXITCODE -ne 0) { throw "Run $index evidence validation failed" }
}
