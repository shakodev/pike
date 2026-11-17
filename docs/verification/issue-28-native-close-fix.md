# R28 native output-close correction

Old published PR75 head: `bccf900b4f75fcf313b40fc68bc32b1cb779f61e`.
Actual main: `5d34cb74b99f5e7e6865789f645e5fe519ba2d94`.
Verified corrected source: `924fc02edad382fa108c24c180b280ade53e4336`.
The final publication commit adds evidence and reports only, on the same
`issue-28-close-policy` branch and [PR75](https://github.com/shakodev/pike/pull/75).
This report supplements, and preserves, the original [R28 report](issue-28.md)
and every earlier receipt. The reviewer failures are required native cleanup
failures; they are not the separately reported GUI keyboard-injection failure.

## Reproduction and cause

All investigation ran locally on macOS arm64 with Rust1.97.1, Cargo1.97.1,
locked dependencies, `CARGO_BUILD_JOBS=4` and this worktree's existing target.
`CODEX_HOME=/Users/daan.hoedjes/.codex-personal` matched the required personal
home, and `CODEX_HOME="$HOME/.codex-personal" codex login status` returned
`Logged in using ChatGPT`. No account configuration was changed.

The exact original application test was run before production changes:

```text
cargo test --locked -p pike --lib app::tests::closing::close_during_output_is_nonblocking_idempotent_and_waits_for_native_receipt -- --exact --nocapture --test-threads=1
```

In the Codex sandbox this passed (dispatch123µs, cleanup602ms), but native
execution outside that sandbox failed at the unchanged ten-second registry
removal deadline (10.11s). A diagnostic-only native repeat failed10.14s.
These reproduce the author's passing observation and the reviewer's native
failure under different confinement. They do not establish the exact original
author run's process environment or make that passing receipt native proof.

The native observation captured parent44994 owning `/dev/ptmx` descriptor3,
and its exact direct child44995 in exiting state `?Es` (`bash`, the system
`/bin/sh`). A bounded one-second sample shows the PTY worker in
`spawn_disposing → drop(event_loop) → Pty::drop → Child::wait → wait4`.
The stopped event source is distinct from native disposal: the app correctly
keeps its Closing registry entry while the native completion is absent.

The destructor sends SIGHUP and waits for the child **before** Rust drops its
master `File` field. With the event loop stopped and unread output queued, the
dying macOS child can wait while closing its terminal; the parent retains the
master and no longer drains it. A separate minimal native PTY probe remained
pending for one second after HUP, then reaped that child immediately after
closing the master. This directly tests the ownership order without the app,
widget focus, injected input, launch pool or event bridge.

The old Unix source and event-loop path are byte-identical to actual main:

| Path | Blob at both old PR head and actual main |
| --- | --- |
| `vendor/alacritty_terminal/src/tty/unix.rs` | `e49cc16d350758ab156672e9189446c799da0a33` |
| `vendor/alacritty_terminal/src/event_loop.rs` | `5bf6048a020864407912649f47a57338b5dcbba9` |

The direct vendor regression uses that same native constructor/destructor,
independent of R28 application policy. Before the correction it also failed
at ten seconds (10.14s). This is source-equivalent evidence of the defect on
main's native path, not a claim that main's entire application was executed.
The flood fixture sends completed output lines and no keyboard input, so the
previous partial-input and GUI-focus explanations do not account for it.

All old red receipts, the sandbox pass, process/descriptor observations,
bounded sample and the independent probe's source/output are retained in
[`docs/baselines/issue-28/native-close-fix/`](../baselines/issue-28/native-close-fix/).
The reviewer's original `/private/tmp/pike-reviewer2-issue28-*.log` and sample
were read only and remain intact. All three failures and the sample are also
copied byte-for-byte into that durable directory with `reviewer-` names.

## Correction and verification

The coordinator explicitly approved the narrow native proposal through Orca
before the production change. `Pty` now stores its master in `Option<File>`
only so exclusive destruction can take and drop the file before `Child::wait`.
The documented invariant keeps the same safe `File` access for every live
reader, writer, registration, deregistration and resize method. Existing HUP,
signal unregistration and actual child reaping remain on the existing PTY
disposal worker. There is no new unsafe production code. Natural exit still
drains output before disposal; explicit cancellation may discard unread output
and does not claim `output_drained` merely because close completed.

Changed source files are `vendor/alacritty_terminal/src/tty/unix.rs` and its
new `unix/tests.rs`; `PATCHES.md` records baseline, reason and upstream-removal
condition. Application code, original application regression and its ten-second
deadline are unchanged. The new resource regression runs alone in a subprocess
to keep concurrent tests from reusing its numeric descriptor during the closed
fd assertion. The pre-isolation body demonstrated old red; only that new fixture
and vendor lint/format were repeated after adding the isolation wrapper.

All verification below ran **locally**. Native tests explicitly ran outside
the Codex sandbox; lint/format ran in it. Commands use `CARGO_BUILD_JOBS=4`,
locked dependencies and this worktree's existing target. App tests use its
default wgpu configuration; no alternate-renderer application run, full
workspace suite or remote CI ran. Full filter lists are retained in the three
neighbor logs' `COMMAND:` headers.

| Targeted command / purpose | Result / retained receipt |
| --- | --- |
| Exact original application command above, native | Old red10.11s; corrected **1 passed**, dispatch110µs, cleanup1ms; `original-native.log`, `application-fixed-green.log`. |
| `cargo test --locked -p alacritty_terminal --lib tty::unix::tests::dropping_output_backlog_closes_master_and_reaps_child_before_returning -- --exact --nocapture --test-threads=1` | Old red10.14s; corrected **1 passed**. Final isolated run observed child51886 reaped (`waitpid=-1/ECHILD`) and masterfd3 closed (`fcntl=-1/EBADF`), disposal2,959µs; `vendor-original-red.log`, `vendor-fixed-green.log`, `vendor-final-isolated-green.log`. |
| `cargo test --locked -p alacritty_terminal --lib --` with the disposal-after-error/unwind, chunk-drain and EOF/late-child-exit filters | **3 passed**; `vendor-disposal-neighbors.log`. Its printed synthetic panic is intentionally caught and asserted by the passing unwind test. |
| `cargo test --locked -p iced_term --lib --` with natural exit, repeated close/fused stream, canceled waiter/startup, prepared attachment/drop and sticky completion filters | **7 passed**; `adapter-disposal-stream.log`. Natural final code37/text/drained state retained. |
| `cargo test --locked -p pike --lib -- app::tests::cancellation:: app::tests::aggregate_shutdown app::tests::closing::stalled_reader app::tests::closing::success_error_exit --nocapture --test-threads=1` | **5 passed**; `application-disposal-neighbors.log`. Actual preparation and aggregate ownership remain pending until native cleanup; success/error final output retained. |
| `cargo clippy --locked -p alacritty_terminal --lib --tests -- -D warnings` | Passed; `vendor-clippy.log`, then affected-test-only repeat in `vendor-final-isolated-clippy.log`. |
| `cargo clippy --locked -p pike --lib --tests -- -D warnings` | Passed; `application-clippy.log`. No second repeat after the test-only isolation change. |
| `cargo fmt --package alacritty_terminal -- --check`; `git diff bccf900b4f75fcf313b40fc68bc32b1cb779f61e..HEAD --check -- vendor/alacritty_terminal docs/verification` | Passed for changed source/reports; `format.log`, `final-format.log`. |

The whole-diff whitespace check returned2 for native harness trailing spaces
and blank EOF lines in the byte-preserved raw receipts. That failed check is
retained in `raw-receipt-whitespace.log`; the logs are not rewritten to silence
it, and the source/report check above passes. Receipt hashes remain unchanged.

The held-reader fixture was rerun because closing the native master is relevant
to its deliberately HUP-ignoring FIFO child. It still remains pending until the
FIFO is released: close113µs, actual deadline3,002ms, Wait9µs, final completion
3,005ms. These, the app close110µs/1ms and native destructor2,959µs are individual
debug synthetic/macOS samples, not percentiles or release resource budgets.

Unchanged close-policy, Force, routing, graphics controls and held-IME evidence
from the original report and independent review is reused, not relabeled as a
fresh pass. The affected native cases above cover changed ownership behavior;
there was no reason to repeat the entire critical set, both renderers or the
Windows compilation checklist. Existing `iced_winit::try_next` deprecations and
the `block0.1.6` future-compatibility notice remain visible in the logs.

`provenance.json` records the exact old/main/corrected source SHAs, unchanged
original app-test blob and retained reviewer receipt hashes. Independent review
of the corrected published head and the coordinator's publication gate remain
pending; this author has not merged PR75.

## Limits and preserved holds

Actions remains OFF; no remote workflow was enabled, dispatched or rerun.
No additional agent, worktree, second PTY, runtime, production polling loop,
dependency, lockfile change or unmerged R25/R26 code was introduced. Published
refs, preintegration checkpoints, old reports, failed receipts and caches are
preserved. No PR merge is authorized or performed by this author.

There is no native Windows/ConPTY, Linux, physical keyboard/monitor or release
RAM/CPU measurement in this correction. The prior GUI injection failure stays
visible and unresolved; no cleanup/input oracle or deadline is relaxed. The
#13/#14 physical holds and #14 scheduling waiver remain unchanged.
