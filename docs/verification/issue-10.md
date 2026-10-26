# Issue #10 — bounded asynchronous terminal startup

Dependencies #7, #8 and #9 are closed and merged on starting main `081ec9c`.
Read AGENTS.md and issue #10 (no comments). The referenced uncommitted PLAN.md
is absent from this checkout and nearby Documents/Downloads folders. The issue
explicitly declares its body self-contained, so implementation follows that scope.

## Ownership and limits

`iced_term::PreparedTerminal::new` creates only the existing Alacritty backend
and bounded event bridge. It is Send without unsafe code. `attach` constructs
the font, widget ID, bindings, theme and canvas cache on the UI thread. The old
synchronous constructor delegates to the same two steps for compatibility.

`src/launch.rs` owns startup independently of layout. Every admitted attempt has
fresh typed request and session IDs, allocated without wraparound or reuse across
pools. A semaphore bounds **all** running, queued and delivered-but-unattached
launches (default 2). Another bounds sessions including startup and disposal
(default 16). Rejection creates no task or queue entry. Work uses the existing
SharedExecutor (one async worker, at most two blocking workers); no dependency,
feature, lockfile, unsafe implementation or additional PTY is added.

The request's Drop cancels abandoned receive futures. A separate cancellation
handle covers a closed/retried target while its Iced task still waits. Queued
work checks cancellation before invoking preparation. In-progress OS creation
is allowed to finish; a canceled or undeliverable success requests close and
awaits acknowledged backend disposal. The startup permit remains held until
attachment or disposal, and until a delivered abandoned owner is dropped, so
prepared results cannot accumulate outside the budget. Session capacity remains
held until backend disposal, including after attachment. All waits are driven
by oneshot/watch notifications, with no polling or periodic production task.

One dormant ownership future per admitted session observes handoff and shutdown.
Pool Drop signals cleanup even if a receive task survives; normal app shutdown
also waits for all ownership to drain **before** exiting the shared executor.
Callers must drop or consume delivered prepared owners before awaiting that
drain. A stuck OS creation/cleanup is pending work, never a false completion;
there is no forced timeout that pretends native resources were released.

The app exposes Closed / Starting / Failed / Running states. Close and Retry
validate both IDs before attaching a result, and backend events match the current
session ID. Window close stops admission, cancels the target and waits for drain
while the UI continues processing completions. Failure messages contain only
metadata; no command, cwd, environment, terminal contents or credentials are logged.
Final terminal output remains visible after natural exit until explicitly closed.

## Verification

Checked pinned installed APIs: iced_term's Backend Drop,
ShutdownHandle and event bridge; Alacritty spawn_disposing; Iced 0.14.0's entered
boot/update context, Task::perform, close_requests, exit and button message bounds;
Tokio 1.54.1's spawn_blocking, semaphore, oneshot and watch APIs. Used the existing
Iced futures select API rather than enabling Tokio macros or adding packages.

Run locally (on Windows first prepare the pinned runtime):

```powershell
./tools/prepare-conpty.ps1 # Windows only, after clean
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
cargo test --locked --no-default-features --features renderer-tiny-skia
cargo clippy --locked --all-targets --no-default-features --features renderer-tiny-skia -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
cargo build --release --locked
cargo test --release --locked --lib measure_32_requested_launches -- --ignored --nocapture
```

Tests inject slow success/failure and worker panic, exhaust admission with 32
requests, cancel before queued work starts and while it is running, discard a
successful handoff, close the pool/window before late success, and reject stale
success/failure after retry. Real PTY fixtures use only a synthetic shell builtin
waiting for input (`read` on Unix, `set /p` on Windows), without external child
helpers. Vendor tests explicitly cover Send handoff and unattached disposal.
The opt-in measurement is intentionally excluded from the normal test suite.

Local macOS 26.7.1 arm64, Rust 1.97.1: formatting, denied-warning application
Clippy and tests pass (16 library tests, 1 app routing test, 1 ignored measurement);
5 example tests and 46 explicit iced_term tests pass. Release application build
and the opt-in release measurement pass. The same 17 application tests (plus the
ignored probe), 46 adapter tests and application Clippy pass with tiny-skia. All-target Windows MSVC cross-target
Clippy also passes. Cargo reports the existing upstream `block` 0.1.6 future
incompatibility notice for the macOS wgpu profile; it is not a denied warning.

## Release observations and limits

[Raw macOS release measurement](../baselines/issue-10/macos-release.txt) runs a
32-request burst in each of three scenarios with a two-job/two-session budget.
Both workers are deliberately held for 100 ms; target/window closure happens
before successful preparation returns. All three scenarios admit exactly 2 and
reject 30. The event-loop surrogate continues handling 5 ms timer work on the
calling thread during that delay. `max_dispatch_us` measures the longest admission
call in the burst; `recovery_ms` runs from worker release to acknowledged native
cleanup plus result consumption. It includes scheduler/fixture work.

| Scenario | Max admission (µs) | Heartbeats | Max timer gap (ms) | Recovery (ms) |
| --- | ---: | ---: | ---: | ---: |
| target_close | 72.500 | 13 | 7.551 | 16.705 |
| window_close | 20.375 | 13 | 7.617 | 11.675 |
| failure | 22.667 | 13 | 7.568 | 0.176 |

The isolated test process peaked at 16,302,080 resident bytes; `/usr/bin/time`
reported 0.72 s wall, 0.02 s user and 0.04 s system CPU for all three scenarios.
These include the test harness, timers and synthetic sessions, not a running
GUI or an idle-terminal budget. [Provenance and source hashes](../baselines/issue-10/provenance.json)
identify this single observation; it is not a percentile or a sustained-load test.

This is a synthetic dispatch/cleanup measurement, **not an Iced render/input
latency measurement**. Native Windows ConPTY execution, actual GUI stalls,
Windows handle/thread recovery and idle release RAM/CPU profiling remain
unverified: the available host is macOS and the user explicitly prohibited
GitHub Actions. Windows MSVC cross-target Clippy provides type checking only.
The tests and probe are ready for a Windows host; run the commands above and
exercise Close during Starting, Retry, typing, resize, copy/paste and window
close interactively there. No Windows performance or leak claim is made.

The PR commit carries `[skip ci]` to prevent both existing pull_request workflows
from running, per [GitHub's documented skip behavior](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs).
No workflows are changed or dispatched. Required skipped checks may remain pending.
