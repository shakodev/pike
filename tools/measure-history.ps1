param(
    [Parameter(Mandatory = $true)][string]$TestExecutable,
    [string]$OutputDirectory = 'target/history-profile'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'compatibility-support.ps1')
Initialize-CompatibilityDiagnostics
function Read-HistoryPhase {
    param([string]$Path)
    # ReadAllText's default Windows sharing can transiently deny the probe's
    # marker writes. The sampler must not lock a file its producer updates.
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    $reader = New-Object IO.StreamReader($stream)
    try { $reader.ReadToEnd() } finally { $reader.Dispose() }
}
$exe = (Resolve-Path -LiteralPath $TestExecutable).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw 'Choose a fresh output directory.' }
New-Item -ItemType Directory -Path $output | Out-Null
$cases = @(
    @{ Name = 'r03-full-10000'; Mode = 'full'; History = 10000; Combining = 0 },
    @{ Name = 'full-2000'; Mode = 'full'; History = 2000; Combining = 0 },
    @{ Name = 'viewport-2000'; Mode = 'viewport'; History = 2000; Combining = 0 },
    @{ Name = 'viewport-10000'; Mode = 'viewport'; History = 10000; Combining = 0 },
    @{ Name = 'hidden-2000'; Mode = 'hidden'; History = 2000; Combining = 0 },
    @{ Name = 'zero-history'; Mode = 'viewport'; History = 0; Combining = 0 },
    @{ Name = 'combining-512'; Mode = 'viewport'; History = 2000; Combining = 512 },
    @{ Name = 'combining-4096'; Mode = 'viewport'; History = 2000; Combining = 4096 }
)
foreach ($case in $cases) {
    $directory = Join-Path $output $case.Name
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $exe
    $start.Arguments = '--exact snapshot::profile::profile_history_snapshot_cost --ignored --nocapture'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardError = $true
    $start.EnvironmentVariables['PIKE_PROFILE_MODE'] = $case.Mode
    $start.EnvironmentVariables['PIKE_PROFILE_HISTORY'] = [string]$case.History
    $start.EnvironmentVariables['PIKE_PROFILE_COMBINING'] = [string]$case.Combining
    $start.EnvironmentVariables['PIKE_PROFILE_DIRECTORY'] = $directory
    $process = [Diagnostics.Process]::Start($start)
    $stderr = Read-CompatibilityStderr -Reader $process.StandardError
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $phase = 'startup'
    try {
        $samples = @(while (-not $process.HasExited) {
            if ($watch.Elapsed.TotalSeconds -gt 60) { throw 'History profile timed out.' }
            $process.Refresh()
            if ($process.HasExited) { break }
            $marker = Join-Path $directory 'phase.txt'
            if (Test-Path -LiteralPath $marker) {
                try { $next = Read-HistoryPhase $marker; if ($next) { $phase = $next } }
                catch [IO.IOException] { }
            }
            $sample = [pscustomobject]@{
                Seconds = $watch.Elapsed.TotalSeconds; Phase = $phase
                CpuSeconds = $process.TotalProcessorTime.TotalSeconds
                PrivateBytes = $process.PrivateMemorySize64; WorkingSetBytes = $process.WorkingSet64
                Threads = $process.Threads.Count; Handles = $process.HandleCount
            }
            if ($process.HasExited -or $sample.PrivateBytes -le 0) { break }
            if (Test-Path -LiteralPath $marker) {
                try { if ((Read-HistoryPhase $marker) -ne $phase) { continue } }
                catch [IO.IOException] { continue }
            }
            $sample
            Start-Sleep -Milliseconds 10
        })
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "History profile failed with exit $($process.ExitCode)." }
        $profile = Import-Csv -LiteralPath (Join-Path $directory 'profile.csv')
        if ($profile.retained_history -ne [string]$case.History) { throw 'History did not saturate at the configured cap.' }
        $samples | Export-Csv -LiteralPath (Join-Path $directory 'process.csv') -NoTypeInformation
        $phases = @(foreach ($name in 'empty','saturated','plateau','snapshot','released') {
            $rows = @($samples | Where-Object Phase -eq $name)
            if ($rows.Count -lt 3) { throw "Insufficient process samples for phase $name." }
            $middle = [int][Math]::Floor($rows.Count / 2)
            [pscustomobject]@{ Phase = $name; Samples = $rows.Count
                PrivateBytes = @($rows.PrivateBytes | Sort-Object)[$middle]
                MinimumPrivateBytes = ($rows.PrivateBytes | Measure-Object -Minimum).Minimum
                MaximumPrivateBytes = ($rows.PrivateBytes | Measure-Object -Maximum).Maximum
                WorkingSetBytes = @($rows.WorkingSetBytes | Sort-Object)[$middle]
                Threads = $rows[-1].Threads; Handles = $rows[-1].Handles }
        })
        [ordered]@{
            Case = $case.Name; Profile = $profile; Phases = $phases
            MaximumSampledPrivateBytes = ($samples.PrivateBytes | Measure-Object -Maximum).Maximum
            CpuSeconds = $process.TotalProcessorTime.TotalSeconds; WallSeconds = $watch.Elapsed.TotalSeconds
            ExecutableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $exe).Hash
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $directory 'summary.json') -Encoding UTF8
        $profile | Format-List
        $phases | Format-Table
    } finally {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
        [IO.File]::WriteAllText((Join-Path $directory 'stderr.txt'), $stderr.GetAwaiter().GetResult())
        $process.Dispose()
    }
}
