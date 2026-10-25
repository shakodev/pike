# Pure validation and bounded diagnostics shared by the collector and its tests.
function Initialize-CompatibilityDiagnostics {
    if ('Pike.Research.BoundedDiagnostics' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;

namespace Pike.Research {
    public static class BoundedDiagnostics {
        public static async Task<string> ReadAsync(TextReader reader, int limit) {
            if (limit < 1) throw new ArgumentOutOfRangeException("limit");
            var buffer = new char[512];
            var prefix = new StringBuilder(limit);
            int count;
            while ((count = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false)) != 0) {
                int retained = Math.Min(count, limit - prefix.Length);
                if (retained > 0) prefix.Append(buffer, 0, retained);
            }
            return prefix.ToString();
        }
    }
}
'@
}

function Read-CompatibilityStderr {
    param([System.IO.TextReader]$Reader, [int]$Limit = 4096)
    Initialize-CompatibilityDiagnostics
    return [Pike.Research.BoundedDiagnostics]::ReadAsync($Reader, $Limit)
}

function Test-CompatibilityTrace {
    param(
        [object[]]$Events,
        [ValidateRange(1, 2)][int]$Sessions,
        [ValidateSet('smoke', 'idle', 'startup')][string]$Mode
    )
    if ($Events.Count -eq 0 -or $Events.Count -gt 512) { throw 'Missing or oversized harness trace.' }
    if (@($Events | Where-Object event -eq 'timeout').Count -gt 0) { throw 'Harness close timed out.' }
    $finished = @($Events | Where-Object event -eq 'finished')
    if ($finished.Count -ne 1 -or $finished[0].a -ne '0' -or $finished[0].b -ne 'false') {
        throw 'Harness did not complete successfully.'
    }
    if (@($Events | Where-Object event -eq 'exit').Count -ne $Sessions) { throw 'Missing originating session exit observations.' }
    foreach ($sessionId in 1..$Sessions) {
        $ready = @($Events | Where-Object { $_.event -eq 'ready' -and [int]$_.session -eq $sessionId })
        if ($ready.Count -ne 1) { throw "Missing or duplicate readiness for session $sessionId." }
        $exit = @($Events | Where-Object { $_.event -eq 'exit' -and [int]$_.session -eq $sessionId })
        if ($exit.Count -ne 1) { throw "Missing or duplicate exit for session $sessionId." }
        $childExit = @($Events | Where-Object { $_.event -eq 'child_exit' -and [int]$_.session -eq $sessionId })
        if ($childExit.Count -ne 1 -or [int]$childExit[0].a -ne (40 + $sessionId)) {
            throw "Unexpected or missing controlled exit code for session $sessionId."
        }
        if ($Mode -eq 'smoke') {
            $sizes = @($Events | Where-Object { $_.event -eq 'size' -and [int]$_.session -eq $sessionId })
            if ($sizes.Count -lt 2) { throw "Missing before/after dimensions for session $sessionId." }
            foreach ($size in $sizes) {
                if ([int]$size.a -lt 1 -or [int]$size.a -gt 512 -or [int]$size.b -lt 1 -or [int]$size.b -gt 1000) {
                    throw "Invalid dimensions for session $sessionId."
                }
            }
            if ($sizes[0].a -eq $sizes[-1].a -and $sizes[0].b -eq $sizes[-1].b) {
                throw "Dimensions did not change for session $sessionId."
            }
        }
    }
    if ($Mode -eq 'smoke') {
        $screenshots = @(0..($Events.Count - 1) | Where-Object { $Events[$_].event -eq 'screenshot' })
        if ($screenshots.Count -ne 2) { throw 'Smoke trace is missing two screenshot observations.' }
        if ($Sessions -eq 2) {
            $swaps = @(0..($Events.Count - 1) | Where-Object { $Events[$_].event -eq 'swap' })
            if ($swaps.Count -ne 1 -or $swaps[0] -le $screenshots[0] -or $swaps[0] -ge $screenshots[1]) {
                throw 'Smoke trace is missing the swap between screenshots.'
            }
            $backgroundExit = @(0..($Events.Count - 1) | Where-Object { $Events[$_].event -eq 'exit' -and [int]$Events[$_].session -eq 2 })[0]
            $foregroundExit = @(0..($Events.Count - 1) | Where-Object { $Events[$_].event -eq 'exit' -and [int]$Events[$_].session -eq 1 })[0]
            if ($backgroundExit -ge $foregroundExit -or $Events[$backgroundExit].a -ne '1') {
                throw 'Smoke trace did not exit session 2 first while session 1 was focused.'
            }
        }
    }
    # Missing final markers remain observations of the unpatched baseline.
}
