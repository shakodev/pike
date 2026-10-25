# Issue #5: bounded, nonblocking terminal event delivery

Verified on 2026-10-25 on Windows 11 Pro build 26200, x64 MSVC, Rust 1.97.1.
Prerequisite #4 is closed and merged in base commit `7b55c1d`. Work was done
in an isolated worktree; existing uncommitted resource/renderer changes in
the original checkout were preserved and are not part of this branch.

## Bridge policy and ownership

`vendor/iced_term/src/event_bridge.rs` replaces the blocking 100-event Tokio
channel and the 1,000-event forwarding channel. Each session owns its own
bridge. A brief std mutex protects bounded state; neither side accesses the
terminal, performs I/O, or awaits while holding that bridge mutex. Producers
never wait for UI queue capacity, including when called under Alacritty's
terminal mutex. Tokio Notify wakes a directly polled, fused subscription
stream; no new worker, runtime, forwarding task or periodic timer is added.

| Traffic | Admission and retention |
| --- | --- |
| Wakeup, mouse/cursor dirty, bell | One boolean each; repeated events coalesce |
| Title / ResetTitle | One latest-value slot, at most 4,096 UTF-8 bytes; truncation preserves character boundaries and releases the original allocation |
| PtyWrite, clipboard store/load, color and size requests | FIFO, at most 256 entries; retained String capacities total at most 65,536 bytes |
| ChildExit, Exit, source completion, first error | Separate reserved slots, unaffected by control/payload saturation |

The FIFO budget counts String **capacity**, including clipboard-store data,
and releases accounting when an event is dequeued. The entry limit also
bounds inline event/Arc storage. The pinned Alacritty formatter closures
capture only fixed protocol metadata (indices/terminators); their opaque
allocations are not measured as String payload. The title uses its own bound.
These are bridge budgets, not whole-process RAM limits. The installed
`iced_futures` 0.14.0 tracker awaits each output send; `iced_winit` 0.14.1's
Sink proxy has bounded 100-entry staging and 100 outstanding window messages,
plus sender/in-flight slots. Framework messages already dequeued from the
bridge have their own finite storage. This path does not use the proxy's
unbounded raw-send API. Alacritty's input queue remains separate work in #6.

On either limit violation, the bridge retains the first redacted error and
stops ordinary admission. It does not silently discard a protocol reply and
continue claiming a healthy session. Already admitted controls drain in FIFO
order, followed by latest presentation state, child-exit, error, Exit, then
source completion. Once Exit/error is recorded, ordinary producers cannot
starve termination by refilling those slots. A later ChildExit remains
deliverable. The first failure stays sticky even after its notification is
consumed. Other sessions have independent state and locks.

Applications receive `BackendCommand::EventBridge(EventBridgeEvent::Error(..))`
with their normal originating session ID. Handling it disables further
input/resize commands for that backend, requests only its PTY's shutdown,
and returns `Action::EventBridgeError`. The compatibility harness displays
and records this redacted failure. Callers should continue routing lifecycle
events until completion; dropping a subscription cancels observation, while
the terminal retains pending bounded state for resubscription.

The event-loop proxy owns a completion-on-drop marker; clones retained in
`Term` do not inherit it. This detects an event-loop return/unwind even while
an exited terminal model remains retained. EOF without Exit becomes
`BackendStopped`; EOF after Exit ends cleanly. `EventBridgeEvent::Completed`
means event production ended. It is **not** a join handle, exit-code fallback,
output-drain guarantee, or off-thread cleanup protocol; those remain #7/#8.

Reviewed [upstream PR 69's exact diff](https://github.com/kemokempo/iced_term/pull/69/files)
(`338e59f`): substituting `try_send` would still lose full-queue events. No
upstream or dependency versions were changed by this implementation.

## Verification

Passed:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo build --release --locked --bins --examples
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
powershell -NoProfile -File tools/test-feature-profiles.ps1
powershell -NoProfile -File tools/test-feature-profile-guard.ps1
powershell -NoProfile -File tools/test-compatibility-support.ps1
```

Application and example Clippy/tests also passed with
`--no-default-features --features renderer-tiny-skia`. Each profile runs four
application/executor tests, four example tests, and **40 explicit vendor tests**
(25 upstream plus 15 new tests). CI already runs explicit vendor tests for
both renderers. No dependency or lockfile files changed.

The new regressions cover 10,000-event dirty/title/control floods against a
stalled consumer, 256-entry and aggregate payload overflow, oversized retained
capacity, UTF-8 title bounds, ordered formatter requests, sticky lifecycle
delivery, EOF before/after Exit, pending-receive cancellation, final receiver
drop, subscription cancellation/resubscription and session IDs. Repeated polls
after EOF return None immediately; there is no closed-channel loop to spin.
A producer floods while holding a real Alacritty terminal mutex and finishes
within a five-second test deadline without any consumer polls.

Two Windows tests use the existing ConPTY backend: 2,000 synthetic output
lines with resize and controlled exit while delivery is stalled; and an
overloaded backend shutting down while a second real session remains usable
and exits independently with its expected code. Both complete the source
stream with the terminal model still retained, exercising completion-marker
ownership. Test deadlines detect stalls; they are not performance budgets.

## Release measurements

Final release harness observations and 61-sample idle counters are retained in
[baselines/issue-5](../baselines/issue-5/), with machine metadata, source and
binary hashes. The renderer is wgpu only, the shared executor has one async
worker and at most two blocking workers, and history is the unchanged
10,000-line upstream cap with small synthetic content. Window is 960x640 DIP,
scale 1.0; monitor is 1920x1080 at 280 Hz, high performance power plan.

Run outside a debugger using the existing measurement script:

```powershell
powershell -NoProfile -File tools/measure-compatibility.ps1 -Renderer wgpu -Shell cmd -Sessions 2 -Mode smoke -OutputDirectory target/issue-5
powershell -NoProfile -File tools/measure-compatibility.ps1 -Renderer wgpu -Shell powershell -Sessions 2 -Mode smoke -OutputDirectory target/issue-5
powershell -NoProfile -File tools/measure-compatibility.ps1 -Renderer wgpu -Shell cmd -Sessions 1 -Mode idle -OutputDirectory target/issue-5
```

In the isolated worktree the commands used `-Executable` pointing at the
shared root target's release/examples/compatibility.exe. That file's adjacent
fixture was built from this branch. The initial sandboxed measurement could
not read CIM process metadata; the successful runs used the approved command
outside that restriction. No measurement was inferred from architecture.

| Scenario | Samples | Ready/window ms | Observed result |
| --- | ---: | ---: | --- |
| cmd / two sessions / smoke | 1 run | 1,305.75 | Swap/resize/background-exit checks passed; codes 42 and 41; 1 final marker |
| Windows PowerShell / two sessions / smoke | 1 run | 1,005.42 | Same checks passed; codes 42 and 41; 1 final marker |
| cmd / one session / idle | 61 counters over 61.454 s | 974.97 | All responsiveness observations true; code 41; 203.03 ms close observation |

The final idle run used **10.50% of one core**, **210.79–211.10 MiB** private
bytes and **165.68–166.53 MiB** working set (min–max), 33–38 threads and
641–646 handles. Child-tree CPU was 1.02% of one core and its private bytes
were recorded separately. This **fails** the 0.1% CPU / 80 MiB one-session
targets. No general performance improvement is claimed from a single run.
Across the 61 samples, private-byte median/p95 were 211.00/211.10 MiB;
working-set median/p95 were 166.28/166.51 MiB (nearest-rank p95).

The smoke runs validate equal-sized swaps, resize/dimension changes, background
exit routing and fixture cleanup. Readiness includes shell/fixture/window
startup with 20 ms external polling; it is not input-to-render latency.
Smoke close-observation time includes the scripted smoke sequence. These are
single runs, not p95 results. No fixture processes remained after exit.

Idle uses ten seconds settling and 61 external samples over at least 60
seconds. CPU is `100 * delta CPU seconds / wall seconds` (one core); private
bytes and working set are separate, with child costs recorded separately.
These observations do not establish the MVP idle-memory/CPU budgets. The
base branch still performs redundant font measurement, PTY resize and full
history snapshots on ordinary events; the original checkout's resource fixes
were deliberately not included here. This issue removes queue deadlocks and
closed-channel loops without claiming to fix all existing redraw work.

Final output/title markers remain incomplete, as in the prior baseline:
ConPTY drain/final-output correctness remains #7. Windows 10/older ConPTY,
PowerShell 7 (absent), other DPI/GPU setups, 4/8/32-session performance,
long-duration history/paste stress, and input/render latency percentiles were
not measured in this focused change. Tiny-skia has compile/lint/unit/ConPTY
coverage here; its release resource profile was not remeasured.
