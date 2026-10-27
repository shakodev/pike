param([Parameter(Mandatory)][string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
Import-Module PSReadLine -ErrorAction Stop
Set-PSReadLineOption -EditMode Emacs -HistorySaveStyle SaveNothing -BellStyle None -PredictionSource None
function prompt { 'probe> ' }
$report = Join-Path $OutputDirectory 'psreadline.txt'
"PowerShell=$($PSVersionTable.PSVersion); PSReadLine=$((Get-Module PSReadLine).Version); EditMode=Emacs" |
    Set-Content -LiteralPath $report -Encoding UTF8
# Use actual shipped key handlers and ReadLine, never invoke editing methods
# directly or replace the input implementation with mocked ConsoleKeyInfo.
foreach ($case in @(
    @{ Name = 'ctrl-u'; Expected = 'kept' },
    @{ Name = 'ctrl-c'; Expected = '' },
    @{ Name = 'after-cancel'; Expected = 'after' },
    @{ Name = 'navigation'; Expected = 'zax' }
)) {
    [Console]::Write("$([char]27)]2;pike:psreadline-$($case.Name)$([char]7)")
    $line = PSConsoleHostReadLine
    if ($line -cne $case.Expected) { throw "Synthetic PSReadLine $($case.Name) result mismatch." }
    "passed: $($case.Name)" | Add-Content -LiteralPath $report -Encoding UTF8
}
[Console]::Write("$([char]27)]2;pike:psreadline-done$([char]7)")
exit 0
