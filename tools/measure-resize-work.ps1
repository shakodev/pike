param(
    [string]$Executable = 'target/release/examples/resize_work.exe',
    [string]$OutputDirectory = 'target/resize-work'
)

$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Executable).Path
$directory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $directory) { throw 'Choose a fresh output directory.' }
New-Item -ItemType Directory -Path $directory | Out-Null
$start = New-Object Diagnostics.ProcessStartInfo
$start.FileName = $exe
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$probeProcess = [Diagnostics.Process]::Start($start)
$stdout = $probeProcess.StandardOutput.ReadToEndAsync()
$stderr = $probeProcess.StandardError.ReadToEndAsync()
$watch = [Diagnostics.Stopwatch]::StartNew()
try {
    $samples = @(while (-not $probeProcess.HasExited) {
        if ($watch.Elapsed.TotalSeconds -gt 40) { throw 'Resize-work probe timed out.' }
        $probeProcess.Refresh()
        if ($probeProcess.HasExited) { break }
        [pscustomobject]@{
            Seconds = $watch.Elapsed.TotalSeconds
            CpuSeconds = $probeProcess.TotalProcessorTime.TotalSeconds
            PrivateBytes = $probeProcess.PrivateMemorySize64
            WorkingSetBytes = $probeProcess.WorkingSet64
            Threads = $probeProcess.Threads.Count
            Handles = $probeProcess.HandleCount
        }
        Start-Sleep -Milliseconds 10
    })
    $probeProcess.WaitForExit()
    [IO.File]::WriteAllText((Join-Path $directory 'counters.csv'), $stdout.GetAwaiter().GetResult())
    [IO.File]::WriteAllText((Join-Path $directory 'stderr.txt'), $stderr.GetAwaiter().GetResult())
    if ($probeProcess.ExitCode -ne 0) { throw "Resize-work probe failed with exit $($probeProcess.ExitCode)." }
    $rows = @(Import-Csv -LiteralPath (Join-Path $directory 'counters.csv'))
    if ($rows[-1].case -ne 'completed') { throw 'Missing native completion evidence.' }
    $samples | Export-Csv -LiteralPath (Join-Path $directory 'process.csv') -NoTypeInformation
    [ordered]@{
        Scope = 'Headless adapter + real ConPTY; synthetic size queries/floods; excludes GUI rendering and child-process resources'
        Cases = $rows.Count
        WallSeconds = $watch.Elapsed.TotalSeconds
        CpuSeconds = $probeProcess.TotalProcessorTime.TotalSeconds
        MaximumSampledPrivateBytes = ($samples.PrivateBytes | Measure-Object -Maximum).Maximum
        MaximumSampledWorkingSetBytes = ($samples.WorkingSetBytes | Measure-Object -Maximum).Maximum
        ExecutableSha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        FixtureSha256 = (Get-FileHash -LiteralPath (Join-Path (Split-Path $exe) 'pty_fixture.exe') -Algorithm SHA256).Hash
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $directory 'summary.json') -Encoding UTF8
    $rows | Format-Table
} finally {
    if (-not $probeProcess.HasExited) { $probeProcess.Kill(); $probeProcess.WaitForExit() }
    $probeProcess.Dispose()
}
