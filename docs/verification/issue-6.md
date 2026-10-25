# Issue #6: bounded input through the PTY writer

Verified 2026-10-26 (Europe/Amsterdam) on Windows 11 Pro build 26200, x64 MSVC, Rust 1.97.1.
Prerequisite #5 is merged in base commit `b9df04f`. An isolated worktree
preserves the original checkout's uncommitted resource/renderer changes.

## Admission and accounting

Each terminal has a **2 MiB outstanding byte budget**. The patched Alacritty
sender checks an entire input before allocating queued chunks, either accepting
all bytes or returning `InputError::{Full,Closed,WakeFailed}`. A failed poller
wake admits nothing. Rejection contains only counts/reason, never input text.
Pastes preflight before publishing a payload, and admission repeats that check
atomically when the command reaches the backend. The clipboard API must still
return its String; that original allocation is outside the writer budget.

Accepted bytes pack into ordered **8 KiB chunks**. Small key writes share a
partial tail chunk rather than allocating one message per key. `Msg::Input`
compatibility also enters this queue: no input payload enters Alacritty's
unbounded std message queue, and the old unbounded write list is removed.
The event loop owns at most one active source chunk. Chunk boundaries may
split a UTF-8 scalar; they never change, duplicate, decode or reorder bytes.

Moving bytes out of the queue, or into Windows's fixed 1 MiB piper buffer,
does **not** release the budget. The existing Windows worker increments shared
progress only after the actual `AnonWrite::write` returns success, including
partial writes. Synchronous PTYs use the event loop's write return values.
Thus a blocked writer retains its credit until consumption or explicit
cancellation. No per-byte acknowledgements are enqueued to the UI, and no new
worker/runtime/timer/polling subscription is introduced.

Admission is serialized by a short queue mutex. That lock never covers a
terminal lock or blocking PTY I/O. The actual writer only updates atomics.
The source allocation bound is the outstanding budget plus partial-chunk
capacity/metadata; one active source chunk and existing fixed backend buffers
remain bounded. Caller-owned messages and OS buffers have separate ownership.
The budget is a bound through the **OS writer**, not proof that the child
application has read the data. The ConPTY-host observation below matters.

All iced_term keyboard/paste/protocol/mouse/alternate-scroll writes use the
fallible sender. `Action::InputRejected` reports rejection; both widget and
scripted compatibility-harness inputs surface it. Rejected input does not
scroll the terminal, remeasure fonts, snapshot history, or clear the canvas
cache. Alternate-scroll expansion checks the full size before allocating and
does not overflow on `i32::MIN`. Application integrations should show a
rejection and let the user retry; they must not silently retry a partial paste.

## Close, cancellation and failure

`BackendCommand::RequestShutdown` uses the existing PTY shutdown path. It
closes admission, clears queued chunks and wakes the loop directly, ahead of
metadata traffic. The loop checks cancellation before processing each metadata
message and before each write. Explicit close cancels unwritten accepted
input; it does not pretend to flush it. Healthy input remains FIFO.

`InputStatus` exposes outstanding/queued/written/cancelled byte counts and
closed state. An OS write already executing at cancellation can still finish,
so the cancelled count can fall until the source stops. Pending input is
also cancelled on source destruction or writer failure. A Windows writer
failure wakes the loop even if writable interest was disabled after moving
all queued bytes into piper. Later input receives Closed rather than quietly
accumulating behind a dead writer.

This does not add joined/off-thread PTY cleanup or final-output draining (#7),
nor promise termination of detached descendants (#8). The probe waits for
the existing event-source completion, separately measures the request call,
and checks that its owned fixture PIDs disappear within five seconds.
Requested close is marked as expected in the event bridge: completion without
a natural Exit is clean, while an existing overload/error remains sticky.
An unrequested source end still reports `BackendStopped`.

## Provenance and dependencies

The exact published Alacritty **0.25.1** archive is retained under
`vendor/alacritty_terminal`, including its Apache-2.0 license, revision,
original manifest/lockfile and all reference fixtures. Its archive SHA-256 is
`46319972e74179d707445f64aaa2893bbf6a111de3a9af29b7eb382f8b39e282`.
The [patch ledger](../../vendor/alacritty_terminal/PATCHES.md) distinguishes
functional changes from stable-rustfmt normalization. It remains the existing
Alacritty/ConPTY implementation, selected with `[patch.crates-io]`.

No runtime dependencies or resolved runtime versions changed. Explicit
upstream tests introduce the existing `serde_json` dev-dependency into the
workspace lockfile: serde_json 1.0.154, itoa 1.0.18 and zmij 1.0.23. The
packaged lockfile remains untouched provenance; tests use Pike's root lock.

## Automated verification

Passed:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo build --release --locked --bins --examples
cargo clippy --locked -p alacritty_terminal --all-targets -- -D warnings
cargo test --locked -p alacritty_terminal
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
powershell -NoProfile -File tools/test-feature-profiles.ps1
powershell -NoProfile -File tools/test-feature-profile-guard.ps1
powershell -NoProfile -File tools/test-compatibility-support.ps1
```

The root application has four executor tests; examples have four existing
compatibility tests. Explicit iced_term tests pass **41 per renderer**.
Alacritty passes **145 unit tests** (134 upstream plus 11 new), **45 upstream
reference tests**, and **one doctest**. Tiny-skia application Clippy/tests also
pass. The new regressions cover full-paste rejection, credit retained after
dequeue/buffering, UTF-8 across chunk boundaries, packed tiny writes, concurrent
admission, counter wrap, failed wakes, cancellation/legacy routing, actual
Windows buffered-write stalls and partial writes followed by failure. The
additional iced_term regression checks expected close without suppressing an
already recorded overload.

CI now runs the Alacritty tests/lints explicitly and the release ConPTY input
probe for each renderer. No hardware-sensitive timing threshold is enforced.
PR review found that putting probe output under cached `target` could restore
an old run directory and make the probe reject a rerun. CI now writes to the
runner's temporary directory, scoped by workflow run ID and attempt. Local
validation retained a stale target output directory while a fresh temporary
probe run succeeded; the cached directory was left untouched.

## Windows release observations

Raw CSV/JSON, machine/build metadata and binary/source hashes are retained in
[baselines/issue-6](../baselines/issue-6/). The release probe is headless:
wgpu is compiled but no renderer/device/window is initialized. Measurements
are of the actual terminal backend and existing Windows workers, not GUI
idle-resource compliance. The synthetic children disable line buffering/echo;
one never reads, while another reads controlled Unicode and reports only
byte count/hash. No terminal transcripts or credentials are recorded.

Five runs each attempt one oversized paste, accept an initial 2 MiB, then
attempt 400 additional 64 KiB writes at 25 ms intervals. This is external test
traffic, not a production timer. Outstanding bytes never exceed **2,097,152**.
Each run rejects 256–286 further writes explicitly. The second session first
rejects an oversized input without changing its backlog, then receives
**98,195 UTF-8 bytes** with the exact expected hash in every run. This verifies
that rejected data did not leak into the accepted stream and that another
session remains usable while the first is saturated.

| Measurement | Samples | Median | p95 (nearest rank) |
| --- | ---: | ---: | ---: |
| 98,195-byte send through fixture result observation | 5 runs | 194.823 ms | 259.474 ms |
| Saturated-session shutdown request call | 5 runs | 0.454 ms | 0.674 ms |
| Request through event-source completion | 5 runs | 0.621 ms | 0.938 ms |

The echo time includes console Unicode conversion, fixture reads, file output
and 5 ms observation polling; it is not keystroke-to-render latency. Source
completion is not a joined-shutdown contract. Queued bytes become zero at
cancellation, later writes are explicitly Closed, and no fixture PID remained
after the cleanup deadline in any run.

External process counters were sampled about every 100 ms (90–93 samples per
run). In the predefined seconds 3–10 plateau, application private bytes were
flat **within every run**, at 8.61–8.64 MiB across the five runs. Application
CPU during those active repeated-input attempts was 0.46–1.60% of one core,
calculated as `100 * delta CPU seconds / delta wall seconds`. This is not an
idle CPU measurement or a whole-process-tree memory claim.

A separate CIM process-tree run measured the stalled session's owned child
and ConPTY hosts. Application private bytes stayed flat at **9.16 MiB**, but
child-tree private bytes rose from **239.03 to 839.78 MiB**, with **94.80% of
one CPU core** over the same sample window. ConPTY can accept bytes into its
own console-input buffer before the child reads them; OS-write completion
does not acknowledge child consumption. This patch bounds application
message/write queues and retains the requested 2 MiB writer budget, but
**does not bound ConPTY's internal input-record memory or CPU**. Preserve this
reproduction for the broader Windows stress/resource gates (#37/#38); no
whole-tree plateau or end-to-child admission guarantee is claimed.

The ordinary two-session GUI smoke checks also passed for cmd and Windows
PowerShell: swap, resize, originating background exit, codes 42/41, and fixture
cleanup. First-window/ready observations were 1,550.89/1,148.52 ms respectively
(one run each), and each observed one final marker. Exact final-output draining
remains #7. Prior GUI idle-budget failures are not resolved by this change.

Reproduce with fresh output directories and the built adjacent fixture:

```powershell
cargo build --release --locked --bins --examples
powershell -NoProfile -File tools/measure-input-backlog.ps1 -Runs 5 -OutputDirectory target/input-backlog-runs
powershell -NoProfile -File tools/measure-input-backlog.ps1 -IncludeProcessTree -OutputDirectory target/input-backlog-tree
```

The isolated worktree passed `-Executable` pointing at the shared root target
directory. CIM counters require process-query access; the separate tree run
used the approved command outside sandbox restrictions. Windows 10/older
ConPTY, Linux/macOS synchronous PTYs, long-duration paste stress, broader pane
counts, and GUI input/render latency percentiles were not measured here.
Tiny-skia has compile/lint/unit/ConPTY coverage; its GUI release resource
profile was not remeasured.
