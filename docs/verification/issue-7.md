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
A failed poller preparation also disposes its already-created PTY on a transient
worker. Full asynchronous startup and OS thread-creation exhaustion remain
outside this change. A worker unwind reports Panicked after destruction. Immediate process aborts
cannot be reported by an in-process API.

## Windows closing requirements

Microsoft documents that ConPTY close can emit final output and can block on
older Windows. The patch closes concurrently with draining, and joins before
backend completion. It also stops leaking the parent's two pipe-end copies:
those copies otherwise prevent EOF and make joined reader cleanup impossible.
This is the narrow handle prerequisite for #7. Immediate explicit cancellation
also terminates the owned root child before closing ConPTY and waits for its
process handle on the cleanup worker. Natural exits do not use this cancellation
path. This prevents the reproduced pre-attachment close from leaving the output
reader pending forever; explicit close does not promise graceful child shutdown
or a flushed output stream. Broader process/thread handles,
callback failure paths, and detached descendants remain the audit in #8.
References: [creation and pipe ownership](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session),
[ClosePseudoConsole behavior](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole).

## Verification

Local host: macOS arm64, pinned Rust 1.97.1. Passed formatting, application
Clippy with warnings denied, application tests (4), examples (4), explicit
Alacritty tests (144 unit, 45 reference, 1 doctest) and explicit adapter tests
for each renderer (44 on macOS). Alacritty's explicit all-target Clippy passes.
The broader adapter-test Clippy command also encounters upstream view-test
style lints; this is outside the required app/Alacritty checks.

New regressions cover multiple drain chunks/final sentinel, repeated close
with one stream result, cancellation of subscriptions/waiters/owner, disposal
of unobserved startup results, a gated destructor proving off-thread disposal
before completion, broken-pipe registration failure without Exit, worker unwind,
and ordering of source/backend completion, including a pending-output timeout.
Windows adds actual pipe-worker backpressure/EOF and read-error tests alongside
its existing ConPTY regressions.

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

Both renderer jobs passed in [Windows CI run 40297043809](https://github.com/shakodev/pike/actions/runs/40297043809)
on source `a87905a`. This includes the application and example checks, feature
isolation guards, 46 adapter tests, 150 Alacritty unit tests, 45 reference tests,
one doctest, both Clippy checks, release builds, the existing bounded-input
probe, and the new lifecycle probe. Windows used x64 MSVC Rust 1.97.1 and build
26100. Raw release CSV/JSON and binary hashes are retained in
[baselines/issue-7](../baselines/issue-7/).

Each renderer ran ten sessions per scenario; p95 is nearest rank (the maximum
for ten samples). Every natural exit retained the sentinel and code 37; every
owned fixture PID was gone after completion. All 60 sessions met the one-second
completion target.

| Completion observation | wgpu median / p95 | tiny-skia median / p95 |
| --- | ---: | ---: |
| Natural (includes startup/output/snapshot) | 46.758 / 59.142 ms | 52.900 / 61.208 ms |
| Explicit close during output | 3.918 / 5.090 ms | 4.341 / 5.312 ms |
| Owner cancellation after subscription cancellation | 3.612 / 4.786 ms | 4.626 / 5.162 ms |

The repeated close request loop itself took at most 0.032 ms, and owner drop at
most 0.099 ms. Across this active probe, maximum sampled private bytes were
14.45/15.42 MiB (wgpu/tiny-skia), working set 17.04/18.39 MiB, threads 8/8, and
handles 162/162. Application CPU time was 0.484375/0.5625 seconds over
1.438354/1.584681 wall seconds (33.68%/35.50% of one core). Samples were observed
about every 20 ms; these are sampled maxima rather than absolute peaks. Both
builds initialize no renderer, so these runs are not a renderer comparison.
The counters do not establish idle compliance, long-term memory/handle stability,
or whole-tree resource bounds; the broader Windows handle audit remains #8.
Older Windows close behavior and GUI resource/responsiveness measurements
remain unverified on this macOS host.

## Early-cancellation regression found by Windows CI

The first Windows run passed natural exit and retained the sentinel, but three
immediate-close/cancellation tests exceeded five seconds. An opt-in metadata-only
`PIKE_TRACE_CLEANUP=1` trace showed ConPTY close returning while its output reader
stayed pending beyond a diagnostic 15-second deadline. The fix cancels the owned
root process before explicit disposal and waits for process exit off thread;
the tests returned to their original five-second deadline. Trace points cover
close entry/return, reader/close/writer joins, and child exit. Normal operation
has tracing disabled, and terminal contents are never included.
