# Compatibility harness

Issue #3 adds developer examples, not terminal functionality to the Pike app.
The original issue #3 harness measurements use the published Iced 0.14.0 /
iced_term 0.8.0 / Alacritty 0.25.1 graph. Issue #4 adds only an iced_term
manifest patch, renderer profiles, and a shared executor; terminal source
behavior remains unchanged. No second PTY implementation is used.

## Renderer and executor comparisons

`renderer-wgpu` is the default; build the software renderer with
`--no-default-features --features renderer-tiny-skia`. Preserve separate
executables when comparing profiles, since Cargo normally overwrites the
same output path. The measurement script accepts `-Renderer wgpu|tiny-skia`
and `-Executable <path/to/compatibility.exe>`; keep that build's `pty_fixture.exe`
beside it. For example:

```powershell
powershell -NoProfile -File tools/measure-compatibility.ps1 -Renderer tiny-skia -Executable target/issue-4/tiny-skia/compatibility.exe -Shell cmd -Sessions 2 -Mode idle -OutputDirectory target/compatibility/tiny-skia
```

Both entry points select `pike::executor::SharedExecutor` once per application.
The executor enables timers, one async worker and at most two concurrent
blocking workers. The existing Alacritty/ConPTY workers remain separate.
Runtime drop cancels async futures and joins finite blocking work. This does
not implement PTY shutdown acknowledgement/output draining (issue #7).
The worker cap is not queue admission control; startup/cleanup must not
enqueue unlimited blocking jobs (issue #10).

The historical observations below describe the unpatched issue #3 baseline.
Current results are recorded in [issue #4 verification](verification/issue-4.md).

## Build and run

From the repository root on Windows x64 MSVC:

```powershell
cargo build --release --locked --examples
cargo test --locked --examples
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-compatibility-support.ps1
cargo run --release --locked --example compatibility -- --shell cmd --sessions 2 --mode manual --output target/compatibility/manual
```

Shell choices are `cmd`, `powershell` (Windows PowerShell), and optional
`pwsh` (PowerShell 7). Shell profiles are disabled. Each shell runs the
adjacent `pty_fixture.exe`, not an interactive user profile. Missing shells
are errors/skips, never silent fallbacks. Sessions are limited to 1 or 2.

The toolbar swaps the two same-sized pane views, resizes the window, asks
the child to report its console dimensions, runs explicit VT probes, or
exits the fixtures. Pane state holds stable IDs, separately from the
terminal registry; layout operations do not recreate terminals. Subscriptions
cover every live terminal, and events/exits route by their originating ID.
Closing an exited terminal cancels its upstream subscription. The normal
exit path asks the synthetic children to exit; a three-second research
timeout records failure and exits. This does not provide the future product's
joined/off-thread PTY lifecycle guarantees.

Only synthetic markers and numeric metadata are recorded in `events.csv`:
ready PID, reported cell dimensions, resize geometry, exit code (when
available), final marker, counters, and smoke steps. Arbitrary terminal
titles, commands, clipboard data, and terminal output are not logged. The
report is limited to 512 records; wakeups are counted rather than logged.
The child accepts at most 64 command bytes per line.

The fixture uses `GetStdHandle`/`GetConsoleScreenBufferInfo` to observe the
console window in cells. The documented unsafe block borrows the standard
handle and reads initialized output only on API success. The only added
dependency is a Windows **dev-dependency** on already-locked windows-sys
0.59.0 for these calls. No packages were added or upgraded in Cargo.lock.
General Windows executable/argument quoting remains issue #9; these fixed
shell commands support quoted fixture paths with spaces, escape PowerShell
apostrophes, and reject percent signs/quotes/newlines in the controlled path.

## Repeat the checks and measurements

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 2 -Mode smoke -OutputDirectory target/compatibility/results
powershell -NoProfile -ExecutionPolicy Bypass -File tools/measure-compatibility.ps1 -Shell powershell -Sessions 2 -Mode smoke -OutputDirectory target/compatibility/results
powershell -NoProfile -ExecutionPolicy Bypass -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 1 -Mode idle -OutputDirectory target/compatibility/results
powershell -NoProfile -ExecutionPolicy Bypass -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 1 -Mode startup -Runs 20 -OutputDirectory target/compatibility/results
```

Repeat idle/startup with PowerShell and idle with two sessions. Run scenarios
sequentially outside a debugger. The script selects `ICED_BACKEND` from
`-Renderer` (default `wgpu`) only in the launched process. Build and record
the corresponding renderer profile; this environment variable does not
remove compiled features.
It waits for ready markers and a native window, settles for 10 seconds, and
takes 61 process-counter samples over at least 60 seconds. App CPU is
normalized to one core (`100 * CPU seconds / wall seconds`). Child-tree
private bytes and CPU are separate; shared working sets are not summed.
The owned process tree is identified by PID/parent PID without collecting
command lines or environment variables. Process counters are polled only by
this external research script, never by the harness's idle event loop.

Startup timing ends when all ready markers and the window title are observed
(20 ms external polling), not when a pixel is presented or an interactive
shell prompt is ready. `CloseObservationMs` measures the script's close/wait
interval; in smoke mode the app is already running its own exit sequence,
so that field is not a shutdown-latency measurement. Smoke uses finite
250 ms one-shot delays to separate rendered frames; idle mode has no timer
or custom background worker. The script checks normal app exit, every
originating session exit, responsiveness, timeout/error flags, and surviving
fixture PIDs. Smoke traces must also contain two screenshot observations,
a swap between them for two sessions, changed positive dimensions for each
session, and the expected background-first exit while session 1 is focused.
It records missing final markers without claiming they passed:
this is an unpatched baseline whose known defects are expected to remain.

Smoke mode captures `before.ppm` / `after.ppm` through Iced's screenshot API
around the same-sized swap, then resizes and asks both fixtures for dimensions.
The background session exits first while session 1 is focused. PPM is lossless
RGB without an additional image dependency. Checked-in PNG exports are only
for convenient review. The **VT probes** button is separate: ConPTY can
consume/reply to queries itself and contaminate the fixture's line input;
that explicit probe is not part of the normal automated exit sequence.

Results directories are overwritten on a rerun. Standard-error capture is
for these synthetic runs only. It retains a 4,096-character prefix while
draining the remainder through a fixed 512-character buffer. The tiny BCL-only
reader is initialized before any measured stopwatch starts. A failed scenario
preserves its event trace and exits nonzero. Shared CI compiles the examples,
runs their pure tests and measurement-support regressions, and checks a
synthetic stderr producer without opening windows or launching PTYs.

See [the recorded baseline](verification/issue-3.md) for measurements,
screenshots, and the F01-F15 reproduction/unverified matrix.
