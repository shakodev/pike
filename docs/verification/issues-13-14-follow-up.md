# Issues #13 and #14: native Windows follow-up

2026-10-28. This follow-up starts from PR #63 head `68322b2`, including the
merged #13 implementation. The owner requested automated checks and explicitly
left physical keyboard/monitor checks pending. Neither issue is closed here.

## Corrected defects

- Native keyboard probes intermittently stalled with an exact prefix received
  by the child. One captured failure had 48 accepted bytes still queued, a
  healthy writer and no cancellation. Windows uses posted readiness packets;
  input arriving between the last chunk and the pending-input check could leave
  writable interest enabled with its packet already consumed. Rearm pending
  Windows writes. No production timer, task, dependency or buffer was added.
- The raw-child probe now services backend protocol events while waiting for
  its report. Deadline diagnostics retain only numeric synthetic progress,
  prefix hashes and writer counters. Byte counts/hashes are unchanged.
- PSReadLine readiness previously preceded input-mode initialization. Ctrl+C
  could become a process signal, and early editing input could be discarded.
  The isolated child sets Ctrl+C-as-input before announcing the next case;
  a fixture-only F12 handler acknowledges that PSReadLine is actually reading.
  Ctrl+U, Ctrl+C, subsequent editing and navigation still use shipped handlers.
- The OS-layout driver waits for a process-owned HWND rather than performing
  a single title lookup immediately after backend readiness. It verifies actual
  foreground focus before every scan code, and stops if another app gains focus.

## Verification and limits

Native release byte matrices, Vim and profile-free PowerShell 7.6.9 / PSReadLine
2.4.5 run through the existing pinned ConPTY runtime. Each complete run exercises
168 cases in each of four cursor/newline modes. Expected counts are 338, 338,
340 and 340 bytes, including the FIFO sentinel. Repeated runs also verify that
each raw fixture PID is absent after acknowledged shutdown. Evidence is in
`docs/baselines/issues-13-14/`.

Ten consecutive complete runs passed per renderer: 13,440 matrix cases total,
20 Vim edits, 20 complete PSReadLine sequences, and all 80 raw fixture PIDs
absent. Earlier diagnostic runs remain in `target/`; a failed eight-run batch
exposed the PSReadLine readiness race before the handshake correction. The
retained final CSVs contain only the 20 consecutive runs after that correction.
Repetition-run peak working set could not be read after exit and is left empty;
use the live idle samples for measured memory instead.

Required formatting, all-target denied-warning application Clippy and tests
pass, including both renderer profiles and example suites. Explicit adapter
suites pass 88 tests per profile (one existing profile test ignored). Explicit
adapter Clippy passes for both. Alacritty passes 166 unit tests (five existing
opt-in tests ignored), 45 reference fixtures and its doctest; its denied-warning
all-target Clippy passes. Root doctests pass as well.

Both native geometry profiles pass pixel translation/clipping checks at
125%, 150% and 200%, equal-size identity replacement, nested layout changes,
overlay clipping, font/theme changes and three acknowledged child cleanups.
These are offscreen synthetic scales, not physical `WM_DPICHANGED` evidence.

The actual-window layout driver reaches its focus check locally, but Windows
does not grant foreground focus to this execution context. It fails before
injecting scan codes. This proves neither a lack of an interactive desktop nor
correct physical keyboard/layout behavior. Run it from a foreground interactive
PowerShell session before treating the OS-layout gate as passed.

Release idle observations use the existing compatibility harness with one cmd
fixture, ten seconds settling, and 61 externally sampled observations. CPU is
normalized to one core; private bytes and working set are separate. These are
single runs on a busy development machine, not isolated performance benchmarks.
The wgpu observation was 0.381% CPU over 69.73 seconds, with private bytes
205,549,568–205,680,640 and working set 166,318,080–166,764,544. These exceed the
proposed one-session CPU/memory targets; no performance-budget pass is claimed.
Tiny-skia recorded 0.0498% CPU over 62.73 seconds, private bytes
12,853,248–12,976,128 and working set 24,625,152–24,678,400. The two single runs
have different background load and do not establish comparative percentiles.
The machine is Windows 11 Pro build 26200, Ryzen 5 2600X (12 logical CPUs),
RTX 2070 driver 32.0.16.1742 and pinned Rust 1.97.1. Power mode, physical monitor
scaling and refresh were not controlled for these opt-in harness observations.

Remaining acceptance: actual Windows layout injection must pass on a foreground
desktop, plus the owner's physical US/AltGr/dead-key/repeat checks, PSReadLine
and editor interaction, real mixed-DPI monitor transitions and interactive
move/resize/focus/clipping checks. Keep #13/#14 open and PR #63 draft pending
that evidence. Production IME and extended keyboard protocols remain separate
roadmap work.

## Hosted follow-up at implementation commit `9484acd`

The [hosted Windows run](https://github.com/shakodev/pike/actions/runs/40750247873)
passes the actual-window US/German/US International scan-code test with the
corrected HWND readiness handling on tiny-skia, including byte/hash matching
and raw-child cleanup. This resolves the local foreground limitation for that
automated gate; physical hardware verification remains pending. Hosted
PowerShell 7.6.7 / PSReadLine 2.4.5 also passes the editing/cancellation checks.
The exact downloaded synthetic evidence is retained in `hosted/tiny-skia`.

The wgpu job at this commit passes raw bytes/Vim/PSReadLine, but its layout
controller fails synchronously sending `WM_INPUTLANGCHANGEREQUEST` when switching
from US to German (after the first four writes). The follow-up driver posts this
message, matching the documented Windows input-language request path, and still
waits for the target thread's verified active layout before any scan code.
[Microsoft's message contract](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-inputlangchangerequest)
specifies a posted message with an ignored return value. The five-second layout
verification deadline and all byte expectations remain unchanged.

Both [native geometry jobs](https://github.com/shakodev/pike/actions/runs/40750252425)
pass, as do the [Windows 2022/2025 ConPTY audits](https://github.com/shakodev/pike/actions/runs/40750256974).
Local bounded-input/stalled-reader and 30-scenario shutdown probes pass as well:
98,195 synthetic echo bytes, whole-input rejection under stall, requested source
completion in 713 ms, and zero cooperative shutdown target misses. Their
resource/latency metadata are retained under `input-backlog/` and `shutdown/`.
