param(
    [string]$Executable = 'target/release/examples/startup_lifecycle.exe',
    [string]$OutputDirectory = 'target/startup-lifecycle-measurements'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'compatibility-support.ps1')
Initialize-CompatibilityDiagnostics
$exe = (Resolve-Path -LiteralPath $Executable).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw 'Choose a fresh output directory.' }
New-Item -ItemType Directory -Path $output | Out-Null

foreach ($scenario in @('target_close', 'window_close', 'failure')) {
    $directory = Join-Path $output $scenario
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $exe
    $start.Arguments = '{0} "{1}"' -f $scenario, $directory
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::Start($start)
    $stderr = Read-CompatibilityStderr -Reader $process.StandardError
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $phase = 'startup'
    try {
        $samples = @(while (-not $process.HasExited) {
            if ($watch.Elapsed.TotalSeconds -gt 30) { throw 'Startup lifecycle probe timed out; cleanup not acknowledged.' }
            $process.Refresh()
            if ($process.HasExited) { break }
            $phasePath = Join-Path $directory 'phase.txt'
            if (Test-Path -LiteralPath $phasePath) {
                # The UI may be replacing this small marker while it is read.
                try {
                    $nextPhase = [IO.File]::ReadAllText($phasePath)
                    if ($nextPhase) { $phase = $nextPhase }
                } catch [IO.IOException] { }
            }
            [pscustomobject]@{
                Seconds = $watch.Elapsed.TotalSeconds
                Phase = $phase
                CpuSeconds = $process.TotalProcessorTime.TotalSeconds
                PrivateBytes = $process.PrivateMemorySize64
                WorkingSetBytes = $process.WorkingSet64
                Threads = $process.Threads.Count
                Handles = $process.HandleCount
                Responding = $process.Responding
            }
            Start-Sleep -Milliseconds 20
        })
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "Startup lifecycle probe exited $($process.ExitCode)." }
        if ((Get-Content -Raw (Join-Path $directory 'phase.txt')) -ne 'finished') { throw 'Probe did not finish its assertions.' }
        $samples | Export-Csv -LiteralPath (Join-Path $directory 'process.csv') -NoTypeInformation
        $timing = @(Import-Csv -LiteralPath (Join-Path $directory 'timing.csv'))
        if ($timing.Count -ne 3 -or @($timing | Where-Object {
            $_.requested -ne '32' -or $_.admitted -ne '2' -or $_.rejected -ne '30'
        }).Count -ne 0) {
            throw 'Startup admission count mismatch.'
        }
        $pids = @(Get-ChildItem -LiteralPath $directory -Filter '*.pid' | ForEach-Object {
            [int](Get-Content -Raw -LiteralPath $_.FullName)
        })
        if ($pids.Count -ne $(if ($scenario -eq 'failure') { 0 } else { 6 })) { throw 'Synthetic child count mismatch.' }
        foreach ($fixturePid in $pids) {
            $child = Get-Process -Id $fixturePid -ErrorAction SilentlyContinue
            if ($child) {
                try { if ($child.Path -eq $exe) { throw "Synthetic child survived backend completion: $fixturePid" } }
                finally { $child.Dispose() }
            }
        }
        $recoveryByCycle = @(foreach ($cycle in 0..2) {
            $baseline = @($samples | Where-Object Phase -eq "baseline-$cycle")
            $recovered = @($samples | Where-Object Phase -eq "recovered-$cycle")
            if ($baseline.Count -lt 3 -or $recovered.Count -lt 3) { throw 'Insufficient baseline/recovery process samples.' }
            [pscustomobject]@{
                Cycle = $cycle
                BaselinePrivateBytes = $baseline[-1].PrivateBytes
                RecoveredPrivateBytes = $recovered[-1].PrivateBytes
                BaselineThreads = $baseline[-1].Threads
                RecoveredThreads = $recovered[-1].Threads
                BaselineHandles = $baseline[-1].Handles
                RecoveredHandles = $recovered[-1].Handles
            }
        })
        $summary = [ordered]@{
            Scenario = $scenario
            OS = [Environment]::OSVersion.VersionString
            CpuSeconds = $process.TotalProcessorTime.TotalSeconds
            WallSeconds = $watch.Elapsed.TotalSeconds
            SampleCount = $samples.Count
            UnresponsiveSamples = @($samples | Where-Object { -not $_.Responding }).Count
            MaximumSampledPrivateBytes = ($samples.PrivateBytes | Measure-Object -Maximum).Maximum
            MaximumSampledWorkingSetBytes = ($samples.WorkingSetBytes | Measure-Object -Maximum).Maximum
            MaximumSampledThreads = ($samples.Threads | Measure-Object -Maximum).Maximum
            MaximumSampledHandles = ($samples.Handles | Measure-Object -Maximum).Maximum
            RecoveryByCycle = $recoveryByCycle
            FixturePids = $pids
            SurvivingFixtures = 0
            ExecutableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $exe).Hash
        }
        $summary | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $directory 'summary.json') -Encoding UTF8
        $timing | Format-List
        $summary | Format-List
    } finally {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $directory 'stderr.txt'), $stderr.GetAwaiter().GetResult())
        $process.Dispose()
        # A failed assertion/timeout must not strand this probe's native fixtures.
        Get-ChildItem -LiteralPath $directory -Filter '*.pid' | ForEach-Object {
            $child = Get-Process -Id ([int](Get-Content -Raw -LiteralPath $_.FullName)) -ErrorAction SilentlyContinue
            if ($child) {
                try { if ($child.Path -eq $exe) { $child.Kill(); $child.WaitForExit() } }
                finally { $child.Dispose() }
            }
        }
    }
}
