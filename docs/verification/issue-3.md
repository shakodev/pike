# Issue #3: unpatched Windows compatibility baseline

Recorded 2026-10-25. See [the harness guide](../compatibility.md) for commands,
protocol limits, timing definitions, and the separation from Pike's app.
Prerequisite #1 and Windows CI #2 are merged. No upstream sources are patched.

## Verification and scope

The developer examples compile Iced application, pane_grid, terminal focus,
and iced_term subscription APIs on x64 Windows MSVC. Four focused tests
cover origin routing, malformed/untrusted markers, bounded dimension state,
and controlled shell-command quoting. No vendor test suites apply.

Both cmd and Windows PowerShell run the deterministic native child through
Alacritty's existing ConPTY. Two-session smoke traces show a same-sized pane
swap, window resize, dimensions changing from 58x29 to 67x32 cells, and
session 2 exiting with code 42 while session 1 remains focused. Session 1
then exits with code 41. The harness window exits with code 0 and no fixture
PID survives. These checks do not establish joined PTY cleanup or absence
of leaks over repeated create/close cycles.

The cmd smoke observed both final OSC title markers; PowerShell observed
neither. All four idle runs also missed their final title markers despite
receiving the controlled child exit codes. Final title markers are an
observation aid, not an exact oracle for all final output bytes. The public
unpatched API cannot extract the final terminal text. Missing markers are
recorded rather than treated as a fixed-behavior pass.

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

## F01-F15 findings and follow-up

The installed 0.8.0/0.25.1 sources were checked again. Source findings are
distinguished from runtime observations and deliberately unverified cases.

| Finding | Evidence / reproduction or explicit unverified status |
| --- | --- |
| F01 bounded producer can block under the terminal lock | Source-confirmed `EventProxy::blocking_send` into capacity 100. **Runtime overload/deadlock unverified**: no deliberate queue-saturation run; belongs to #5. |
| F02 subscription cancellation/EOF | Source-confirmed 1,000-event output channel, panic on send failure, and loop after shutdown/EOF. Harness cancels the originating subscription on exit; **isolated closed-channel spin/panic unverified**, not fixed here. |
| F03 whole-grid history cloning | Source-confirmed `internal_sync` clones `terminal.grid()`. **History saturation cost unverified**: current small fixture history is not a 2,000/10,000-line stress test. |
| F04 repeated font/resize/cache work | Source-confirmed every handle remeasures, resizes, clones, and clears cache without deduplication. Idle traces count thousands of wakeups with only two public layout messages per terminal. A resize/wakeup feedback mechanism is plausible but **causal attribution/internal resize counts are unverified**. |
| F05 widget identity/absolute cached coordinates | Same-size swap screenshots show the expected session labels/content in this sequence. **Selection/scrollback identity, same-size tab reuse, clipping during movement and mixed DPI remain unverified**; this is not an exhaustive regression pass. |
| F06 output drain and teardown acknowledgement | Child exits 41/42 are routed by origin. Final markers are absent in PowerShell smoke and all idle traces; present in cmd smoke. **Exact final-byte drain and PTY joins unverified**. `EventLoop::new(..., false, false)` and discarded worker handle are source-confirmed. |
| F07 ConPTY handle/failure edges | Normal child PIDs are gone after each completed run on Windows 11 build 26200. **Failure injection, older Windows close behavior, handle attribution, and repeated-cycle leak tests unverified**. |
| F08 executable/argument quoting | Controlled cmd and PowerShell launchers work on this repository path; pure tests cover generated quoted fixture paths and PowerShell apostrophes. **Native launch with arbitrary special paths/arguments or a spaced executable program field unverified**; no general quoting fix is claimed. |
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
