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
Set-PSReadLineKeyHandler -Key F12 -ScriptBlock {
    [Console]::Write("$([char]27)]2;pike:psreadline-$script:probeCase$([char]7)")
}
foreach ($case in @(
    @{ Name = 'ctrl-u'; Expected = 'kept' },
    @{ Name = 'ctrl-c'; Expected = '' },
    @{ Name = 'after-cancel'; Expected = 'after' },
    @{ Name = 'navigation'; Expected = 'zax' }
)) {
    # Publish readiness only once Ctrl+C is input, rather than a console signal.
    # ReadLine restores console modes when it returns; a fast parent could send
    # ETX between the next title and ReadLine's initialization and abort this script.
    [Console]::TreatControlCAsInput = $true
    $script:probeCase = $case.Name
    [Console]::Write("$([char]27)]2;pike:psreadline-arming-$($case.Name)$([char]7)")
    $line = PSConsoleHostReadLine
    if ([string]$line -cne $case.Expected) { throw "Synthetic PSReadLine $($case.Name) result mismatch." }
    "passed: $($case.Name)" | Add-Content -LiteralPath $report -Encoding UTF8
}
[Console]::Write("$([char]27)]2;pike:psreadline-done$([char]7)")
exit 0
