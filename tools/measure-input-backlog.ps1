param(
    [string]$Executable = 'target/release/examples/input_backlog.exe',
    [string]$OutputDirectory = 'target/input-backlog-measurements',
    [ValidateRange(1, 20)][int]$Runs = 1,
    [switch]$IncludeProcessTree
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'compatibility-support.ps1')
Initialize-CompatibilityDiagnostics
$exe = (Resolve-Path -LiteralPath $Executable).Path
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $output -Force | Out-Null
foreach ($run in 1..$Runs) {
    $directory = Join-Path $output ('run-{0:d2}' -f $run)
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $exe
    $start.Arguments = '"{0}"' -f $directory
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::Start($start)
    $stderr = Read-CompatibilityStderr -Reader $process.StandardError
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $childIds = @()
    $treeRecorded = $false
    try {
        $samples = @(while (-not $process.HasExited) {
            if ($watch.Elapsed.TotalSeconds -gt 40) { throw 'Input backlog probe timed out.' }
            $process.Refresh()
            if ($process.HasExited) { break }
            if ($IncludeProcessTree -and -not $treeRecorded -and (Test-Path -LiteralPath (Join-Path $directory 'stall/ready'))) {
                $rows = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name | Select-Object ProcessId,ParentProcessId,Name)
                $ownedIds = @($process.Id)
                do {
                    $found = @($rows | Where-Object { $ownedIds -contains [int]$_.ParentProcessId -and $ownedIds -notcontains [int]$_.ProcessId } | ForEach-Object { [int]$_.ProcessId })
                    $ownedIds += $found
                } while ($found.Count -gt 0)
                $childIds = @($ownedIds | Where-Object { $_ -ne $process.Id })
                $rows | Where-Object { $ownedIds -contains [int]$_.ProcessId } | Export-Csv -LiteralPath (Join-Path $directory 'process-tree.csv') -NoTypeInformation
                $treeRecorded = $true
            }
            $childrenPrivate = 0L
            $childrenCpu = 0.0
            foreach ($childId in $childIds) {
                $child = Get-Process -Id $childId -ErrorAction SilentlyContinue
                if ($child) {
                    try {
                        $childrenPrivate += $child.PrivateMemorySize64
                        $childrenCpu += $child.TotalProcessorTime.TotalSeconds
                    } finally { $child.Dispose() }
                }
            }
            [pscustomobject]@{
                Seconds = $watch.Elapsed.TotalSeconds
                CpuSeconds = $process.TotalProcessorTime.TotalSeconds
                PrivateBytes = $process.PrivateMemorySize64
                WorkingSetBytes = $process.WorkingSet64
                Threads = $process.Threads.Count
                Handles = $process.HandleCount
                ChildrenPrivateBytes = $childrenPrivate
                ChildrenCpuSeconds = $childrenCpu
            }
            Start-Sleep -Milliseconds 100
        })
        $process.WaitForExit()
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
        $samples | Export-Csv -LiteralPath (Join-Path $directory 'process.csv') -NoTypeInformation
        if ($process.ExitCode -ne 0) { throw "Input backlog probe exited $($process.ExitCode)." }
        foreach ($fixture in @('stall', 'echo')) {
            $readyPath = Join-Path (Join-Path $directory $fixture) 'ready'
            $fixtureId = [int](Get-Content -LiteralPath $readyPath -Raw)
            $cleanup = [System.Diagnostics.Stopwatch]::StartNew()
            while ($true) {
                $child = Get-Process -Id $fixtureId -ErrorAction SilentlyContinue
                if (-not $child) { break }
                $ownedFixture = $child.ProcessName -eq 'input_fixture'
                $child.Dispose()
                if (-not $ownedFixture) { break }
                if ($cleanup.Elapsed.TotalSeconds -gt 5) {
                    throw "Fixture $fixture remained after cleanup deadline."
                }
                Start-Sleep -Milliseconds 20
            }
        }
        $plateau = @($samples | Where-Object { $_.Seconds -ge 3 -and $_.Seconds -le 10 })
        $first = $plateau[0]
        $last = $plateau[-1]
        $summary = [ordered]@{
            Samples = $samples.Count
            PlateauSamples = $plateau.Count
            PlateauPrivateMinimum = ($plateau.PrivateBytes | Measure-Object -Minimum).Minimum
            PlateauPrivateMaximum = ($plateau.PrivateBytes | Measure-Object -Maximum).Maximum
            PlateauWorkingSetMinimum = ($plateau.WorkingSetBytes | Measure-Object -Minimum).Minimum
            PlateauWorkingSetMaximum = ($plateau.WorkingSetBytes | Measure-Object -Maximum).Maximum
            PlateauOneCoreCpuPercent = 100 * ($last.CpuSeconds - $first.CpuSeconds) / ($last.Seconds - $first.Seconds)
            ExitCode = $process.ExitCode
            ProcessTreeMeasured = $treeRecorded
            PlateauChildrenPrivateMinimum = $(if ($treeRecorded) { ($plateau.ChildrenPrivateBytes | Measure-Object -Minimum).Minimum } else { $null })
            PlateauChildrenPrivateMaximum = $(if ($treeRecorded) { ($plateau.ChildrenPrivateBytes | Measure-Object -Maximum).Maximum } else { $null })
            PlateauChildrenOneCoreCpuPercent = $(if ($treeRecorded) { 100 * ($last.ChildrenCpuSeconds - $first.ChildrenCpuSeconds) / ($last.Seconds - $first.Seconds) } else { $null })
            ExecutableSha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        }
        $summary | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $directory 'process-summary.json') -Encoding UTF8
        Get-Content -LiteralPath (Join-Path $directory 'summary.csv')
    } finally {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
        [System.IO.File]::WriteAllText((Join-Path $directory 'stderr.txt'), $stderr.GetAwaiter().GetResult())
        $process.Dispose()
    }
}
