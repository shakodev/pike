# Issue #3: unpatched Windows compatibility baseline

Recorded 2026-10-25. See [the harness guide](../compatibility.md) for commands,
protocol limits, timing definitions, and the separation from Pike's app.
Prerequisite #1 and Windows CI #2 are merged. No upstream sources are patched.

## Verification and scope

The developer examples compile Iced application, pane_grid, terminal focus,
and iced_term subscription APIs on x64 Windows MSVC. Four focused tests
cover origin routing, malformed/untrusted markers, bounded dimension state,
and controlled shell-command quoting. No vendor test suites apply.

Formatting, Clippy (`--locked --all-targets -- -D warnings`), application
tests, example tests (4 passed), release application/example builds, and
actionlint passed locally. The [initial hosted run](https://github.com/shakodev/pike/actions/runs/40209481640)
also passed every check, including the new example test/build steps, without
launching a PTY in shared CI.

Both cmd and Windows PowerShell run the deterministic native child through
Alacritty's existing ConPTY. Two-session smoke traces show a same-sized pane
swap, window resize, dimensions changing from 58x29 to 67x32 cells, and
session 2 exiting with code 42 while session 1 remains focused. Session 1
then exits with code 41. The harness window exits with code 0 and no fixture
PID survives. These checks do not establish joined PTY cleanup or absence
of leaks over repeated create/close cycles.

The primary cmd smoke observed both final OSC title markers; the primary
PowerShell smoke observed neither. All four idle runs also missed their final title markers despite
receiving the controlled child exit codes. Final title markers are an
observation aid, not an exact oracle for all final output bytes. The public
unpatched API cannot extract the final terminal text. Missing markers are
recorded rather than treated as a fixed-behavior pass.
Fresh validation with explicit per-session exit-code assertions observed
one of two final markers under cmd and both under PowerShell, confirming
that the missing markers are not a deterministic shell-specific result.
Those additional event traces and summaries are retained in the
`cmd-2-validation-01` / `powershell-2-validation-01` artifact directories.

## Measurement environment and method

The [machine/build metadata](../baselines/issue-3/machine.json) records the
executable and lockfile SHA-256 values. Windows 11 Pro build 26200; Ryzen
5 2600X (6 cores / 12 logical processors), 17,095,331,840 bytes RAM, RTX 2070
driver 32.0.16.1742, 1920x1080 / 280 Hz, High performance power plan.
Captured screenshots report scale factor 1.0. Windows PowerShell is
5.1.26100.9711; PowerShell 7 is not installed and was skipped. Older Windows,
ARM64, RDP, and other DPI/display configurations were not tested.

Release examples run outside a debugger. Renderer selection is explicitly
`ICED_BACKEND=wgpu` in the launched process; the actual graphics API is not
instrumented. Both upstream renderer features remain compiled. Font defaults
are unchanged (monospace, 14 DIP, scale/line-height setting 1.3). Initial
window is 960x640 logical pixels; one session reports 118x29 cells, two report
58x29 each. Histories contain only the small synthetic startup fixture output;
the unpatched default 10,000-line history is not filled or saturated.

Each idle scenario is separate and sequential, with 10 seconds settling and
61 samples over at least 60 seconds. Counters are external; the idle harness
has no timer or custom worker. CPU percentages are normalized to one core.
Child-tree private bytes/CPU are recorded separately from app private bytes
and working set. CPU, memory, wakeups, and resize observations do not imply
an attribution to a specific upstream component without further profiling.

Raw idle CSVs, event traces, summaries, process-tree metadata, smoke traces,
and synthetic PNG screenshots are under [baselines/issue-3](../baselines/issue-3/).
The report records PID/exit/dimensions/counters only, never arbitrary terminal
titles, clipboard content, shell input, environment values, or real output.

### Idle observations

Each row is one independent 61-sample run; CPU is the process CPU-time delta
over its recorded wall interval, not an average of rounded Task Manager
percentages. Memory and resource counts are observed min-max ranges.

| Shell / sessions | App CPU, one core | App private MiB | App working set MiB | Threads | Handles |
| --- | ---: | ---: | ---: | ---: | ---: |
| cmd / 1 | 11.95% | 211.64-212.10 | 165.87-166.74 | 44-49 | 665-670 |
| cmd / 2 | 18.49% | 219.02-219.18 | 168.38-169.55 | 47-52 | 675-680 |
| Windows PowerShell / 1 | 9.39% | 213.48-213.64 | 165.20-165.95 | 44-49 | 664-669 |
| Windows PowerShell / 2 | 11.33% | 217.96-218.56 | 167.41-168.92 | 47-52 | 675-680 |

Separate child-tree maximum private bytes were 9.01 / 18.01 MiB for cmd
one/two sessions and 51.91 / 103.21 MiB for PowerShell one/two. Their CPU
time deltas were respectively 1.64%, 3.67%, 1.34%, and 1.27% of one core;
these are not included in the app-only column. Raw process-tree files name
the harness, OpenConsole, selected shell, and synthetic fixture processes.

The window responded in all 244 observations. This does not establish input
latency or redraw counts. Idle event traces contain thousands of wakeups
despite no fixture activity after the initial size reply; the initial
public layout event count is only two per terminal. The high ongoing work
and private bytes exceed the later one-session 0.1%-CPU / 80-MiB goals.
No target is relaxed here, and these are not saturated-history measurements.
Wakeup counters include startup and close too; an exact idle wakeup rate or
internal resize count is not claimed. Native foreground-window state and
the selected wgpu graphics API were not instrumented.

### Repeated startup observations

Twenty new one-session harness processes were measured sequentially per shell
after preceding runs had warmed executable/driver caches. The endpoint is
ready markers **and window title**, not first presented pixels or an
interactive prompt. p95 uses nearest rank (`ceil(0.95 * 20)`, the 19th sorted
observation); the median is the mean of the 10th and 11th.

| Shell | n | Ready/window median | Ready/window p95 | Close/wait median | Close/wait p95 | Final markers observed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cmd | 20 | 1,016.35 ms | 1,109.35 ms | 205.81 ms | 227.31 ms | 8/20 |
| Windows PowerShell | 20 | 1,030.93 ms | 1,233.09 ms | 212.36 ms | 233.36 ms | 4/20 |

Raw observations: [cmd](../baselines/issue-3/cmd-1-startup.json),
[PowerShell](../baselines/issue-3/powershell-1-startup.json). Every process
returned 0, every fixture exit code was 41, and no fixture PID remained.
Close/wait timing is not a joined-resource completion acknowledgement.

Same-sized swap images: [before](../baselines/issue-3/cmd-2-smoke-01/before.png),
[after](../baselines/issue-3/cmd-2-smoke-01/after.png). They show distinct
session content changing sides without recreating either process. The
PowerShell screenshots are also retained with its smoke trace.

## F01-F15 findings and follow-up

### PR review corrections

Review found that the collector could accept a completed smoke run without
checking its intermediate phases, and that stderr was truncated only after
an unbounded `ReadToEndAsync`. The collector now checks per-session readiness
and exits, screenshot/swap ordering, changed positive dimensions, and the
background-first exit/focus sequence. Diagnostics retain a bounded prefix
while draining through a fixed buffer, initialized outside measured timing.

`tools/test-compatibility-support.ps1` validates all retained traces, rejects
seven negative trace mutations, and checks bounded capture with empty, short,
and oversized input plus a real 1 MiB stderr producer. It runs in CI. Fresh
two-session cmd and Windows PowerShell smoke runs passed the stricter checks.
No Rust or fixture changes were needed; their measured binary fingerprints
and all original baseline artifacts remain unchanged.

The installed 0.8.0/0.25.1 sources were checked again. Source findings are
distinguished from runtime observations and deliberately unverified cases.

| Finding | Evidence / reproduction or explicit unverified status |
| --- | --- |
| F01 bounded producer can block under the terminal lock | Source-confirmed `EventProxy::blocking_send` into capacity 100. **Runtime overload/deadlock unverified**: no deliberate queue-saturation run; belongs to #5. |
| F02 subscription cancellation/EOF | Source-confirmed 1,000-event output channel, panic on send failure, and loop after shutdown/EOF. Harness cancels the originating subscription on exit; **isolated closed-channel spin/panic unverified**, not fixed here. |
| F03 whole-grid history cloning | Source-confirmed `internal_sync` clones `terminal.grid()`. **History saturation cost unverified**: current small fixture history is not a 2,000/10,000-line stress test. |
| F04 repeated font/resize/cache work | Source-confirmed every handle remeasures, resizes, clones, and clears cache without deduplication. Idle traces count thousands of wakeups with only two public layout messages per terminal. A resize/wakeup feedback mechanism is plausible but **causal attribution/internal resize counts are unverified**. |
| F05 widget identity/absolute cached coordinates | Same-size swap screenshots show the expected session labels/content in this sequence. **Selection/scrollback identity, same-size tab reuse, clipping during movement and mixed DPI remain unverified**; this is not an exhaustive regression pass. |
| F06 output drain and teardown acknowledgement | Child exits 41/42 are routed by origin. Final markers vary across smoke/validation runs and are absent in all four idle traces. **Exact final-byte drain and PTY joins unverified**. `EventLoop::new(..., false, false)` and discarded worker handle are source-confirmed. |
| F07 ConPTY handle/failure edges | Normal child PIDs are gone after each completed run on Windows 11 build 26200. **Failure injection, older Windows close behavior, handle attribution, and repeated-cycle leak tests unverified**. |
| F08 executable/argument quoting | Controlled cmd and PowerShell launches passed on the repository path and with a copied fixture path containing both a space and apostrophe; traces are retained in `*-spaced-path-01`. Pure tests also cover constructed quoting/rejected percent signs. **Arbitrary paths/arguments and a spaced executable program field remain unverified**; no general quoting fix is claimed. |
| F09 copy and paste semantics | Source-confirmed visible-cell copy and raw clipboard paste. **Clipboard/wrapped selection and bracketed-paste byte behavior unverified**; fixtures do not read the real clipboard. |
| F10 keyboard/modifier encoding | Focus task compiles and origin routing is tested. **Actual keyboard bytes, Ctrl+U, AltGr/dead keys, IME, and kitty negotiation unverified**; injected backend commands bypass the encoder. |
| F11 mouse and scrolling | **Tracking/scroll-direction/release-outside-pane behavior unverified**; no synthetic OS mouse-input trace was run. Source paths remain the inspected baseline. |
| F12 wide/combining glyphs and cursor ordering | Synthetic screenshots include `wide: 界` and `combining: e + U+0301`. The combining mark is not visibly rendered; source draws only `cell.c`. **Full wide-glyph placement, IME, and cursor/background permutations remain unverified**. |
| F13 protocol event policy | Source handles only Exit/Title/PtyWrite explicitly. An exploratory automated query sequence interfered with the fixture's line input and did not reach its normal exit commands. **Which queries ConPTY consumes/replies to, emulator response bytes, reset-title and bell policies remain unverified**. VT probes are now explicit/manual, separate from the clean smoke path. |
| F14 unbounded input/write backlog | Source-confirmed Alacritty queue and Windows reader/writer workers. **Stalled-reader paste, byte-admission bounds, per-worker memory, and flood/backlog stress unverified**. Measured totals include renderer/runtime/OS overhead. |
| F15 link activation | Source-confirmed built-in broad URL regex and panic on failed `open::that`. **Runtime link activation unverified**; no external links were opened and the fixture emits none. |

The source references are iced_term `terminal.rs`, `backend.rs`, `view.rs`,
`bindings.rs`, and Alacritty `event_loop.rs` / `tty/windows/*` in the installed
locked registry. This baseline does not satisfy the MVP idle CPU/RAM targets,
prove correctness under flood, or replace later lifecycle/input/rendering gates.
