$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path 'target/release/pike.exe').Path
$outputDirectory = 'docs/verification'
New-Item -ItemType Directory -Force $outputDirectory | Out-Null
$launchWatch = [System.Diagnostics.Stopwatch]::StartNew()
$appProcess = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
try {
    do {
        Start-Sleep -Milliseconds 50
        $appProcess.Refresh()
        if ($appProcess.HasExited) { throw 'Pike exited before creating its window.' }
    } while (($appProcess.MainWindowHandle -eq 0 -or $appProcess.MainWindowTitle -ne 'Pike') -and $launchWatch.Elapsed.TotalSeconds -lt 10)
    if ($appProcess.MainWindowTitle -ne 'Pike') { throw 'Expected the Pike window.' }
    $windowDetectedMs = $launchWatch.Elapsed.TotalMilliseconds
    Start-Sleep -Seconds 10
    $idleWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $samples = @(for ($sample = 0; $sample -le 60; $sample++) {
        if ($sample -gt 0) { Start-Sleep -Seconds 1 }
        $appProcess.Refresh()
        if ($appProcess.HasExited) { throw 'Pike exited during the idle observation.' }
        [pscustomobject]@{
            Seconds = $idleWatch.Elapsed.TotalSeconds
            CpuSeconds = $appProcess.TotalProcessorTime.TotalSeconds
            PrivateBytes = $appProcess.PrivateMemorySize64
            WorkingSetBytes = $appProcess.WorkingSet64
            Threads = $appProcess.Threads.Count
            Handles = $appProcess.HandleCount
            Responding = $appProcess.Responding
        }
    })
    $samples | Export-Csv "$outputDirectory/issue-1-idle.csv" -NoTypeInformation -Encoding UTF8
    $first = $samples[0]
    $last = $samples[-1]
    $closeWatch = [System.Diagnostics.Stopwatch]::StartNew()
    if (-not $appProcess.CloseMainWindow()) { throw 'Window close request failed.' }
    if (-not $appProcess.WaitForExit(10000)) { throw 'Pike failed to exit within 10 seconds.' }
    $appProcess.WaitForExit()
    [pscustomobject]@{
        WindowDetectedMs = $windowDetectedMs
        IdleWallSeconds = $last.Seconds - $first.Seconds
        IdleCpuSeconds = $last.CpuSeconds - $first.CpuSeconds
        IdleOneCoreCpuPercent = 100 * ($last.CpuSeconds - $first.CpuSeconds) / ($last.Seconds - $first.Seconds)
        PrivateBytesMinimum = ($samples.PrivateBytes | Measure-Object -Minimum).Minimum
        PrivateBytesMaximum = ($samples.PrivateBytes | Measure-Object -Maximum).Maximum
        WorkingSetBytesMinimum = ($samples.WorkingSetBytes | Measure-Object -Minimum).Minimum
        WorkingSetBytesMaximum = ($samples.WorkingSetBytes | Measure-Object -Maximum).Maximum
        ThreadsMinimum = ($samples.Threads | Measure-Object -Minimum).Minimum
        ThreadsMaximum = ($samples.Threads | Measure-Object -Maximum).Maximum
        HandlesMinimum = ($samples.Handles | Measure-Object -Minimum).Minimum
        HandlesMaximum = ($samples.Handles | Measure-Object -Maximum).Maximum
        AllSamplesResponding = @($samples | Where-Object { -not $_.Responding }).Count -eq 0
        CloseMs = $closeWatch.Elapsed.TotalMilliseconds
        ExitCode = $appProcess.ExitCode
    } | ConvertTo-Json | Set-Content "$outputDirectory/issue-1-smoke.json" -Encoding UTF8
    Get-Content "$outputDirectory/issue-1-smoke.json"
} finally {
    if (-not $appProcess.HasExited) { Stop-Process -Id $appProcess.Id }
    $appProcess.Dispose()
}
