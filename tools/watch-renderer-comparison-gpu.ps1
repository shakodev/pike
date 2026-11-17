# Optional finite native GPU Engine observer, run concurrently by the collector.
param(
    [Parameter(Mandatory)][string]$OwnedTree,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateRange(1, 600)][int]$Seconds = 60
)
$ErrorActionPreference = 'Stop'
$ownedIds = @(Import-Csv -LiteralPath $OwnedTree | ForEach-Object { [int]$_.ProcessId })
if ($ownedIds.Count -gt 64 -or $ownedIds.Count -lt 1) { throw 'Invalid bounded owned process tree' }
$rows = [Collections.Generic.List[object]]::new()
$available = $false
$failure = $null
$watch = [Diagnostics.Stopwatch]::StartNew()
try {
    # Localized/missing counter sets fail explicitly. Engine utilization values
    # may overlap; never sum engines into a fabricated total GPU percentage.
    Get-Counter -Counter '\GPU Engine(*)\Utilization Percentage' -SampleInterval 1 -MaxSamples $Seconds | ForEach-Object {
        foreach ($sample in $_.CounterSamples) {
            if ($sample.InstanceName -match '^pid_(\d+)_' -and $ownedIds -contains [int]$Matches[1]) {
                $available = $true
                if ($rows.Count -ge 32768) { throw 'GPU raw sample bound reached' }
                $rows.Add([pscustomobject]@{ Seconds = $watch.Elapsed.TotalSeconds; Pid = [int]$Matches[1]; Engine = $sample.InstanceName
                    UtilizationPercent = $sample.CookedValue; Status = $sample.Status; Timestamp = $sample.Timestamp.ToUniversalTime().ToString('o') })
            }
        }
    }
} catch { $failure = $_.Exception.Message }
finally {
    $rows | Export-Csv (Join-Path $OutputDirectory 'gpu.csv') -NoTypeInformation
    [ordered]@{ Available = $available; Failure = $failure; Samples = $rows.Count; DurationSeconds = $watch.Elapsed.TotalSeconds
        Meaning = 'per-owned-PID engine utilization; no aggregate GPU sum; absence is unavailable, not zero'
    } | ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'gpu-availability.json') -Encoding UTF8
}
