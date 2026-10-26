# Issue #7: shutdown completion and final output

Prerequisite #5 is merged. The local `PLAN.md` mentioned by the issue is absent;
issue #7 explicitly describes its self-contained scope. No dependencies,
features, versions, or lockfiles change.

## Contract

```rust,ignore
let completion = terminal.shutdown_handle();
terminal.request_close(); // repeat safely; no join or terminal lock
// In an async task, with the application's chosen timeout:
let result = completion.wait().await;
```

`BackendCompletion` carries `ExitReason`, an optional child exit code, and
`output_drained`. Only EOF reached on natural exit sets the latter true.
The existing PTY worker owns EventLoop/parser destruction; Windows cleanup
also joins the existing reader/writer threads and a transient ConPTY close
thread. No Tokio blocking queue, permanent cleanup thread, busy polling, or
new PTY implementation is used. Senders hold Weak poller references so a
retained read-only model or completion handle cannot retain that OS handle.

`EventBridgeEvent::Completed` means production has stopped, and is delivered
before `BackendCompleted`. The latter also returns `Action::BackendCompleted`
when handled. `Action::Shutdown` remains a terminal Exit observation, never a
backend disposal acknowledgement. The compatibility harness now keeps sessions
until backend completion. An I/O-loop failure can complete without Exit and
retains the bridge's redacted BackendStopped error. Overload remains sticky.

The completion watch and reserved bridge slot each retain one result. Multiple
waiters observe that same result. Dropping a waiter or subscription cannot
cancel teardown. Subscription replacement alone keeps an owned session alive;
dropping the Terminal requests close even when a stream recipe survives.
An unobserved constructor result also closes on drop. Constructor execution is
still synchronous; moving preparation off the UI belongs to #10.

Natural exit drains output before sending Exit, flushes synchronized updates,
and retains the terminal model. `final_content()` refreshes it after completion
without needing a subscription. Input is closed and resizes cannot erase the
final grid. Final-content extraction is on demand and never logs its contents;
it uses the existing grid copy, whose viewport optimization belongs to #11.

The parser drain deadline is one second. Failure to reach EOF reports
`IoError(TimedOut)` with `output_drained=false`, retaining an observed exit code.
OS cleanup can take longer; external waits must report timeout as pending,
without treating it as backend completion or force-dropping resources on UI.
A worker unwind reports Panicked after destruction. Immediate process aborts
cannot be reported by an in-process API.

## Windows closing requirements

Microsoft documents that ConPTY close can emit final output and can block on
older Windows. The patch closes concurrently with draining, and joins before
backend completion. It also stops leaking the parent's two pipe-end copies:
those copies otherwise prevent EOF and make joined reader cleanup impossible.
This is the narrow handle prerequisite for #7. Broader process/thread handles,
callback failure paths, and detached descendants remain the audit in #8.
References: [creation and pipe ownership](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session),
[ClosePseudoConsole behavior](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole).

## Verification

Local host: macOS arm64, pinned Rust 1.97.1. Passed formatting, application
Clippy with warnings denied, application tests (4), examples (4), explicit
Alacritty tests (143 unit, 45 reference, 1 doctest) and explicit adapter tests
for each renderer (44 on macOS). Alacritty's explicit all-target Clippy passes.
The broader adapter-test Clippy command also encounters upstream view-test
style lints; this is outside the required app/Alacritty checks.

New regressions cover multiple drain chunks/final sentinel, repeated close
with one stream result, cancellation of subscriptions/waiters/owner, disposal
of unobserved startup results, a gated destructor proving off-thread disposal
before completion, broken-pipe registration failure without Exit, worker unwind,
and ordering of source/backend completion. Windows adds an actual pipe-worker
backpressure/EOF test alongside its existing ConPTY regressions.

The synthetic `shutdown_lifecycle` release example runs ten natural exits,
ten explicit closes during output, and ten owner cancellations. It verifies a
final sentinel after 180 KiB of output and child code 37, without recording a
terminal transcript. Close/cancel latency is measured from request/drop;
natural completion includes startup, output, final-model extraction, and drop.
Five-second safety deadlines fail with pending-cleanup diagnostics. One-second
target misses are reported, not hidden or enforced as hardware-independent CI
thresholds. `tools/measure-shutdown-lifecycle.ps1` observes release-process CPU,
private bytes, working set, threads and handles externally and verifies fixture
PIDs are gone. This active headless probe is not a GUI idle-resource result or
a whole-process-tree memory measurement.

Windows CI runs the probe for both lean renderers after the existing bounded
input probe. Windows measurements and CI links will be recorded after the PR
run. Older Windows close behavior and GUI resource/responsiveness measurements
cannot be established on this macOS host.
