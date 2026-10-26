param(
    [string]$Executable = 'target/release/examples/shutdown_lifecycle.exe',
    [string]$OutputDirectory = 'target/shutdown-lifecycle-measurements'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'compatibility-support.ps1')
Initialize-CompatibilityDiagnostics
$exe = (Resolve-Path -LiteralPath $Executable).Path
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $exe
$start.Arguments = '"{0}"' -f $output
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardError = $true
$process = [System.Diagnostics.Process]::Start($start)
$stderr = Read-CompatibilityStderr -Reader $process.StandardError
$watch = [System.Diagnostics.Stopwatch]::StartNew()
try {
    $samples = @(while (-not $process.HasExited) {
        if ($watch.Elapsed.TotalSeconds -gt 120) { throw 'Shutdown lifecycle probe timed out.' }
        $process.Refresh()
        if ($process.HasExited) { break }
        [pscustomobject]@{
            Seconds = $watch.Elapsed.TotalSeconds
            CpuSeconds = $process.TotalProcessorTime.TotalSeconds
            PrivateBytes = $process.PrivateMemorySize64
            WorkingSetBytes = $process.WorkingSet64
            Threads = $process.Threads.Count
            Handles = $process.HandleCount
        }
        Start-Sleep -Milliseconds 20
    })
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw "Shutdown lifecycle probe exited $($process.ExitCode)." }
    $samples | Export-Csv -LiteralPath (Join-Path $output 'process.csv') -NoTypeInformation
    $rows = @(Import-Csv (Join-Path $output 'samples.csv'))
    foreach ($row in $rows) {
        $child = Get-Process -Id ([int]$row.pid) -ErrorAction SilentlyContinue
        if ($child) {
            $stillOwned = $child.ProcessName -eq 'shutdown_lifecycle'
            $child.Dispose()
            if ($stillOwned) { throw "Synthetic child survived backend completion: $($row.pid)" }
        }
    }
    $summary = [ordered]@{
        OS = [System.Environment]::OSVersion.VersionString
        CpuSeconds = $process.TotalProcessorTime.TotalSeconds
        WallSeconds = $watch.Elapsed.TotalSeconds
        SampleCount = $samples.Count
        MaximumSampledPrivateBytes = ($samples.PrivateBytes | Measure-Object -Maximum).Maximum
        MaximumSampledWorkingSetBytes = ($samples.WorkingSetBytes | Measure-Object -Maximum).Maximum
        MaximumSampledThreads = ($samples.Threads | Measure-Object -Maximum).Maximum
        MaximumSampledHandles = ($samples.Handles | Measure-Object -Maximum).Maximum
        CooperativeTargetMisses = @($rows | Where-Object { $_.within_one_second -eq 'false' }).Count
        ExecutableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $exe).Hash
    }
    $summary | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'summary.json')
    Get-Content (Join-Path $output 'samples.csv')
    Get-Content (Join-Path $output 'summary.json')
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $output 'stderr.txt'), $stderr.GetAwaiter().GetResult())
    $process.Dispose()
}
