param(
    [string]$Executable = 'target/release/examples/keyboard_layout.exe',
    [Parameter(Mandatory)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
# Test-only driver. Send actual scan codes to the fixture's own focused Iced
# HWND; do not post fabricated WM_CHAR messages or call the encoder directly.
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class PikeKeyboardInput {
    [StructLayout(LayoutKind.Sequential)] public struct KeyInput { public ushort vk, scan; public uint flags, time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] public struct MouseInput { public int x, y; public uint data, flags, time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Explicit)] public struct InputUnion { [FieldOffset(0)] public KeyInput key; [FieldOffset(0)] public MouseInput mouse; }
    [StructLayout(LayoutKind.Sequential)] public struct Input { public uint type; public InputUnion data; }
    [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, Input[] inputs, int size);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string className, string title);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint thread);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr LoadKeyboardLayoutW(string layout, uint flags);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool UnloadKeyboardLayout(IntPtr layout);
    [DllImport("user32.dll", SetLastError=true)] static extern IntPtr SendMessageTimeoutW(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);
    public static void Layout(IntPtr window, IntPtr layout) {
        UIntPtr result;
        if (SendMessageTimeoutW(window, 0x50, UIntPtr.Zero, layout, 2, 5000, out result) == IntPtr.Zero)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "WM_INPUTLANGCHANGEREQUEST failed");
    }
    public static void Scan(ushort scan, bool up, bool extended) {
        var input = new Input { type = 1, data = new InputUnion { key = new KeyInput { scan = scan, flags = 8u | (up ? 2u : 0u) | (extended ? 1u : 0u) } } };
        if (SendInput(1, new [] { input }, Marshal.SizeOf(typeof(Input))) != 1)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "SendInput unavailable; an interactive input desktop is required");
    }
}
'@

function Wait-FixtureFile([string]$Name) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while (-not (Test-Path -LiteralPath (Join-Path $output $Name))) {
        if ($process.HasExited) { throw "OS keyboard fixture exited $($process.ExitCode) before $Name." }
        if ($timer.Elapsed.TotalSeconds -gt 35) { throw "OS keyboard fixture timed out waiting for $Name." }
        Start-Sleep -Milliseconds 25
    }
}
function Set-FixtureLayout([string]$Id) {
    $layout = [PikeKeyboardInput]::LoadKeyboardLayoutW($Id, 0)
    if ($layout -eq [IntPtr]::Zero) { throw "Windows keyboard layout $Id is unavailable." }
    $script:loaded += $layout
    [PikeKeyboardInput]::Layout($window, $layout)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ([PikeKeyboardInput]::GetKeyboardLayout($thread) -ne $layout) {
        if ($timer.Elapsed.TotalSeconds -gt 5) { throw "Fixture thread did not activate keyboard layout $Id." }
        Start-Sleep -Milliseconds 25
    }
}
function Scan([int]$Code, [bool]$Up = $false, [bool]$Extended = $false) {
    [PikeKeyboardInput]::Scan([ushort]$Code, $Up, $Extended)
}

$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw 'Choose a fresh output directory.' }
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = (Resolve-Path -LiteralPath $Executable).Path
$start.ArgumentList.Add($output)
$start.UseShellExecute = $false
$process = [Diagnostics.Process]::Start($start)
$window = [IntPtr]::Zero
$original = [IntPtr]::Zero
$loaded = @()
try {
    Wait-FixtureFile 'ready'
    $window = [PikeKeyboardInput]::FindWindowW($null, 'Pike synthetic keyboard layout probe')
    if ($window -eq [IntPtr]::Zero) { throw 'The fixture Iced window is unavailable on this Windows desktop.' }
    [uint32]$owner = 0
    $thread = [PikeKeyboardInput]::GetWindowThreadProcessId($window, [ref]$owner)
    if ($owner -ne $process.Id) { throw 'Refusing to inject keys into a window not owned by this fixture.' }
    $original = [PikeKeyboardInput]::GetKeyboardLayout($thread)
    if (-not [PikeKeyboardInput]::SetForegroundWindow($window)) { throw 'Cannot focus fixture; an interactive input desktop is required.' }

    Set-FixtureLayout '00000409' # US: three presses with no intermediate key-up.
    Scan 0x1e
    Scan 0x1e
    Scan 0x1e
    Scan 0x1e $true
    Wait-FixtureFile 'write-3'
    Scan 0x38 # Left Alt+B must remain legacy Meta input.
    Scan 0x30
    Scan 0x30 $true
    Scan 0x38 $true
    Wait-FixtureFile 'write-4'

    Set-FixtureLayout '00000407' # German: right Alt (AltGr)+Q -> @.
    Scan 0x1d
    Scan 0x38 $false $true
    Scan 0x10
    Scan 0x10 $true
    Scan 0x38 $true $true
    Scan 0x1d $true
    Wait-FixtureFile 'write-5'

    Set-FixtureLayout '00020409' # US International: apostrophe dead key, then E.
    Scan 0x28
    Scan 0x28 $true
    Scan 0x12
    Scan 0x12 $true
    Wait-FixtureFile 'write-6'
    Scan 0x2a # Shift+1 sentinel.
    Scan 0x02
    Scan 0x02 $true
    Scan 0x2a $true
    Wait-FixtureFile 'completed.txt'
    if (-not $process.WaitForExit(10000)) { throw 'OS keyboard fixture did not exit after completion.' }
    if ($process.ExitCode -ne 0) { throw "OS keyboard fixture exited $($process.ExitCode)." }
    $fixtureId = [int](Get-Content -LiteralPath (Join-Path $output 'raw/ready') -Raw)
    $child = Get-Process -Id $fixtureId -ErrorAction SilentlyContinue
    if ($child) {
        try { if ($child.ProcessName -eq 'input_fixture') { throw 'Raw OS-layout child survived acknowledged cleanup.' } }
        finally { $child.Dispose() }
    }
    [ordered]@{
        Layouts = @('00000409 US', '00000407 German AltGr', '00020409 US International dead key')
        Scope = 'SendInput scan codes through actual Windows layout, pinned Winit/Iced, terminal widget and native ConPTY; no physical keyboard hardware claim.'
        FixtureCleanup = 'Raw fixture PID absent after acknowledged shutdown.'
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'layouts.json') -Encoding UTF8
} finally {
    if ($window -ne [IntPtr]::Zero -and -not $process.HasExited) {
        foreach ($key in @(@(0x1e, $false), @(0x30, $false), @(0x10, $false), @(0x12, $false), @(0x28, $false), @(0x02, $false), @(0x2a, $false), @(0x38, $false), @(0x38, $true), @(0x1d, $false))) {
            try { Scan $key[0] $true $key[1] } catch { }
        }
        if ($original -ne [IntPtr]::Zero) { try { [PikeKeyboardInput]::Layout($window, $original) } catch { } }
    }
    foreach ($layout in $loaded) { [void][PikeKeyboardInput]::UnloadKeyboardLayout($layout) }
    if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    $process.Dispose()
}
