# Issue #23 — first session through the workspace registry

Based on merged main `be04b8d495fb5af31d77272f39d28c48cf560819` and its
current #22 identity/model and #10 prepare/attach contracts. Updated AGENTS.md
and PLAN sections3/4 govern this change. The plan's initial empty-repository
baseline is historical. #14's remaining physical checks and #13's DPI hold are
explicitly nonblocking scheduling gates; no unmerged PR63 code is imported.

GitHub Actions is disabled by owner request. Verification is targeted and local;
no workflow, replacement remote CI or final CI round is used. Old issue/plan
blanket checklists do not override the current AGENTS policy.

## Ownership and behavior

`App` owns the pure workspace `Model`, the existing bounded `LaunchPool` and a
SessionId-keyed map of backend owners. PaneState still contains only SessionId;
no Terminal, native handle or transcript enters the layout. Boot reserves one
Starting pane and consumes its single-use launch token with `start_reserved`.
Native preparation runs on the existing shared executor's bounded blocking
workers. Only the matching pending result attaches font/cache/UI state on the
update thread. There is one existing Alacritty PTY/emulator per session.

The same owner-bound WindowFocus is bound before activation; unknown window
focus remains unfocused. Initial widget focus is explicit. A two-pass focus
operation checks that its widget still exists before invoking Iced's exclusive
focus operation. An obsolete request therefore cannot unfocus a replacement.

The application renders the active canonical pane_grid and composes subscriptions
from every attached owner until its reliable backend-completion event is handled,
including hidden and closing entries. Typed subscription/widget SessionId and
raw backend u64 must agree. Background protocol replies continue to reach their
origin; obsolete hidden widget input and stale removed/restarted IDs cannot target
the active terminal implicitly. Layout and backend ownership remain separate.

Close cancels the exact Starting attempt or requests close on its attached owner;
closing entries keep their subscriptions until native disposal is acknowledged.
A late prepared success is dropped without UI attachment and its own shutdown
handle is awaited before acknowledging that canceled owner. Duplicate disposal
receipts cannot acknowledge a different attached owner. LaunchPool retains its
native capacity until cleanup independently of message delivery.

ChildExit closes backend admission before UI delivery. The host treats closed
admission as read-only even if its bridge receipt is still queued, avoiding late
focus reports during EOF drain. Output remains visible while finishing; registry
Exited is committed at backend completion with the actual optional code. The
original bounded terminal model and viewport remain selectable/copyable; there
is no app-owned transcript. Completed entries have no terminal subscription.
Restart allocates a fresh SessionId. Failed launch shows redacted metadata,
Retry and explicit New default terminal rather than silently changing a profile.
Final-pane closure leaves an empty window with New terminal.

The existing minimal window-close behavior stops admission, closes/cancels all
owned sessions and waits asynchronously for LaunchPool's aggregate disposal
acknowledgement before exiting. Queued UI bridge receipts cannot leave registry
metadata behind after that acknowledgement. This is not #28's confirmation,
wait/force-close or detached-descendant policy. No splits, tab UI, settings,
periodic timer, new runtime, native PTY or dependency is added.

The merged #16 deferred paste/copy/input contracts are preserved. Paste failures
remain visible metadata on their origin; stale, hidden or ended paste requests
are canceled, invalidating clones and releasing their bounded pending slot.
Merged #17 is integrated through its actual mouse/history commands; see the
integration record below. Unmerged #19 APIs are not consumed.
Later clipboard/menu integration must retain captured SessionId and bounded,
redacted errors; full menu chrome remains #33.

## Targeted local verification

Use stable Rust, `CARGO_BUILD_JOBS=4` and this worktree's existing target directory.
The original host integration checks cover the application controller/focus
tests, its binary and the opt-in fixture; unchanged workspace/launch/vendor
suites were not repeated for that integration. The cancellation correction below
also changes and checks the launch pool. Commands/results and source heads are
recorded in the reports:

```sh
cargo fmt --package pike -- --check
cargo test --locked --lib app:: -- --test-threads=1
cargo test --locked --lib app::tests::paste::real_widget -- --ignored --test-threads=1
cargo clippy --locked --lib --bin pike --example first_session_host --tests -- -D warnings
cargo clippy --locked --lib --bin pike --example first_session_host --target x86_64-pc-windows-msvc -- -D warnings
cargo build --release --locked --bin pike --example first_session_host
```

Fifteen focused tests exercise actual reservation/preparation/attachment,
redacted invalid launch and fresh retry, stale/mismatched origins/results,
queued cancellation, canceled delivered owners, duplicate-disposal guards,
hidden protocol replies and background completion, rapid final output,
aggregate shutdown with queued bridge receipts, already-exited focus, and
obsolete/exclusive widget focus operations and origin-scoped input/paste errors.
Real widget paste tokens are canceled across stale/hidden/ended origins.
Queued preparation uses deterministic
worker gates; tests do not add a production polling subsystem.
The real-widget token test is opt-in because it requires a local graphics device;
it is run explicitly in addition to the fourteen regular controller/focus tests.

`first_session_host MODE FRESH_OUTPUT` runs the real App inside an actual Iced
window with its own controlled executable as the child. Modes: success, invalid,
delayed (200ms child readiness) and rapid exit. The source's one-shot10s harness
deadline is fixture-only; it never acknowledges failed cleanup as completed.
A separate unit test covers delayed *preparation*, rather than confusing a slow
child prompt with blocked UI startup. Successful/delayed scenarios query the
actual widget focus operation, send known synthetic bytes through the typed host
route and compare the child's captured bytes exactly. They are not physical
keyboard/encoder tests. Rapid exit retains the known final sentinel/code7;
invalid launch retries with fresh IDs and returns to empty state. All paths
wait for real native completion and record registry0 at application shutdown.
Files contain only these synthetic scenes/IDs/metadata, never a user's terminal.

On local Windows, prepare the existing pinned runtime first and run the same
fixture with fresh paths, for example:

```powershell
./tools/prepare-conpty.ps1
./target/release/examples/first_session_host.exe success ./target/issue-23/windows-success
```

Local macOS execution and Windows-target Clippy are not Windows runtime proof.
Selected Windows shell execution, physical keyboard/DPI/monitor behavior and
whole-app CPU/RAM/latency budgets remain unmeasured here. Prior #13/#14 manual
holds remain parked, not new gates. The fixture's registry counts/focus/sentinels
and cleanup receipts establish those observations only, not release budgets.
An initial macOS token fixture canceled a child with a partial tty line and hit
its unchanged10s cleanup deadline. The passing fixture completes that synthetic
line before awaiting native completion; partial-line Unix cancellation remains
a limitation, with no timeout increase or unrelated Unix repair in this change.

## PR72 cancellation correction — author round 1

The [independent review](https://github.com/shakodev/pike/pull/72#issuecomment-6481193167)
reproduced cancellation during native preparation: `LaunchPool` returned
`Err(Canceled)` before `ShutdownHandle::wait`, so the host removed its Closing
reservation and owner while native disposal was still pending. The pool's native
permits and aggregate shutdown were already correct; this was premature host
acknowledgement, not an unbounded orphan or global shutdown failure.

The canceled-success branch now awaits the existing native disposal handle and
releases its job/session permits before sending that cancellation result. Thus
the host keeps its Starting owner and Closing model slot until the receipt is
safe to acknowledge, and a retry can acquire the released capacity. Cancellation
before native creation and genuine preparation failures retain their immediate
error path. No timer, polling, queue, runtime, vendor patch or PTY implementation
is added to production; other handoff and attachment paths are unchanged.

`src/app/tests/cancellation.rs` exercises the real pool, model and native backend
through a test-only preparation gate. Its Unix child opens a FIFO, ignores HUP,
then announces readiness. The test closes the pane during preparation and holds
native cleanup until it closes the FIFO's sole writer; fixture teardown also
releases that gate on unwind. This proves delayed disposal without depending on
a fixed child sleep. While cleanup is pending, the result remains pending, both
host entries and the sole model/native capacity slots remain owned, and wrong
request/premature disposal receipts cannot release them. After actual completion,
the error settles once, duplicate/late receipts leave the replacement untouched,
retry receives fresh IDs, and aggregate shutdown leaves both registries empty.
The test keeps the existing ten-second deadline; readiness sampling and the
100ms pending-result observation exist only in the test.

Local verification on macOS arm64, stable Rust, `CARGO_BUILD_JOBS=4`, default wgpu,
and this worktree's target directory:

```sh
cargo test --locked --lib app::tests::cancellation:: -- --test-threads=1 --nocapture
cargo test --locked --lib app:: -- --test-threads=1
cargo test --locked --lib launch:: -- --test-threads=1
cargo clippy --locked --lib --tests -- -D warnings
cargo check --locked --lib --target x86_64-pc-windows-msvc
cargo fmt --package pike -- --check
git diff --check
```

The first command failed on the old worker ordering with “cancellation was
delivered before disposal,” after releasing and awaiting native cleanup. After
the fix, the host run passed 15 tests, including the new regression; its one
graphics test stayed ignored here. All 21 regular launch tests passed, including
pre-creation cancellation, actual preparation failure, delayed success, retry,
reservation identity, attachment races and aggregate shutdown; the one opt-in
performance measurement stayed ignored. Affected library/test Clippy,
Windows-target library compilation, formatting and whitespace checks passed.
Logs and exact head provenance are retained under `target/issue-23/fix-round1/`.

The earlier real-widget paste test, four GUI scenarios, binary/example lint and
release-build evidence are reused for their unchanged paths; they were not rerun
or relabeled as new-head evidence. No vendor code changed, so no vendor suite was
required. No Actions run was enabled, dispatched or rerun. The FIFO regression
is Unix-only and Windows-target compilation is not native ConPTY verification;
Windows runtime/physical keyboard/DPI and release resource budgets remain
unmeasured for this correction. The reviewer also reproduced the partial-line
Unix timeout on the equivalent main shutdown path and classified it as a
pre-existing intermittent limitation; this correction does not repair it or
increase its deadline. #13/#14 physical holds remain nonblocking.

## PR72 integration with merged #17

The independently approved `c16f5a33414bf4a3c4b5ba14b1d98b549d0a9568`
was merged with current main `be04b8d495fb5af31d77272f39d28c48cf560819`
in the same branch/PR, without conflicts. The vendor files are exactly main's
reviewed bytes; no mouse/paste implementation is duplicated or rewritten.

The host now rechecks pane and owning-window focus before consuming queued
keyboard/paste input. Rejected paste tokens are canceled through their shared
payload, so held clones cannot send and their bounded slot becomes reusable.
Typed SessionId, raw backend ID, terminal ID and model reservation must agree.
Mouse reports and wheel input require a visible live owner, but do not require
or change keyboard focus. Hidden local history/selection commands are ignored.
The merged `Scroll` command can emit alternate-screen cursor keys; after exit,
the host converts it to `ScrollHistory::Lines`, the adapter's explicitly local
path. Exited selection and copy remain usable. Protocol replies and lifecycle
receipts stay independent of keyboard focus/visibility and retain their origin.

Targeted verification ran locally on macOS arm64, Rust 1.97.1,
`CARGO_BUILD_JOBS=4`, default wgpu only, in this worktree's existing target:

```sh
cargo test --locked --lib app::tests::interaction:: -- --test-threads=1
cargo test --locked --lib app::tests::paste::real_widget -- --ignored --test-threads=1
cargo test --locked --lib -- app::tests::all_live_entries app::tests::rapid_exit app::tests::input_failure_metadata app::tests::paste::paste_errors --test-threads=1
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio -- backend::mouse_history_tests::local_history_is_no_input view::pointer_tests::fractional_wheel view::pointer_tests::history_shortcut view::paste_tests:: --test-threads=1
cargo clippy --locked --lib --bin pike --example first_session_host --tests -- -D warnings
cargo clippy --locked --lib --bin pike --example first_session_host --tests --target x86_64-pc-windows-msvc -- -D warnings
cargo fmt --package pike -- --check
git diff --check
```

The new interaction tests passed three cases: queued keyboard focus, live hovered
mouse/alternate wheel ownership without focus stealing, and hidden/exited history
with selection/copy. The keyboard regression first failed against the old host
with “queued keyboard wrote after pane focus changed,” after native cleanup.
One explicit device-backed paste test passed six rejection cases, including
window blur, pane unfocus and matching IDs with no bound host owner; held clones
admitted zero bytes and a subsequent valid request admitted once. Four existing
host routing tests and four explicit vendor integration tests passed with nonzero
filter matches. After tightening the no-owner cases, only those two affected
tests were repeated. Native and Windows-target Clippy passed; final test-source
lint (`cargo clippy --locked --lib --tests -- -D warnings`), formatting and
whitespace checks also passed. The existing `block 0.1.6` future-compatibility
notice remains dependency-only. Logs, exact command results and final source/head hashes
are retained in `target/issue-23/integration-be04/`.

The prior 15-app/21-launch runs and cancellation red/green at `c16f5a3`, and prior
GUI/focus/final-output evidence at `68187299`/`73c33944`, are reused only for
unchanged behavior, never relabeled as this integration's execution. No new
launch-pool, focus-effect, GUI, release-build or resource-measurement run is
claimed. Compilation/lint covers the changed host consumers and tests; the four
vendor filters exercise the merged local-history/alternate-write distinction,
hover focus, history shortcut and deferred paste contracts without a blanket
vendor suite. No full-workspace, second-renderer or remote CI run was needed.

The two VT/history host cases use Unix native children; the queued-focus test
and existing host consumers also compile for Windows. Cross compilation does
not establish Windows/ConPTY runtime behavior. Physical keyboard/monitor/DPI,
Windows shells, and release CPU/RAM/latency remain unmeasured for this head.
The previously documented intermittent Unix partial-line cleanup limitation is
unchanged; fixtures finish controlled lines and await real completion without
raising deadlines. The cancellation acknowledgement fix, initial focus and
retained-output ownership remain intact. #24/#29/#28/#31 and future #19 are
outside this integration. CI remains OFF and independent re-review precedes merge.
