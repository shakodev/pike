# Issue #23 — first session through the workspace registry

Based on merged main `8d9d8be177db88434e731a0a0557dddb502e7c68` and its
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

The current merged widget paste/copy/input contracts are preserved. Concurrent
#16/#17 APIs are not prerequisites and unmerged branches are not consumed. Any
later clipboard/menu integration must retain captured SessionId and bounded,
redacted errors; full menu chrome remains #33.

## Targeted local verification

Use stable Rust, `CARGO_BUILD_JOBS=4` and this worktree's existing target directory.
The relevant checks are the application controller/focus tests, its binary and
the opt-in fixture; unchanged workspace/launch/vendor suites are not repeated.
The exact commands/results and final source head are recorded in the PR/report:

```sh
cargo fmt --package pike -- --check
cargo test --locked --lib app:: -- --test-threads=1
cargo clippy --locked --lib --bin pike --example first_session_host --tests -- -D warnings
cargo clippy --locked --lib --bin pike --example first_session_host --target x86_64-pc-windows-msvc -- -D warnings
cargo build --release --locked --bin pike --example first_session_host
```

Twelve focused tests exercise actual reservation/preparation/attachment,
redacted invalid launch and fresh retry, stale/mismatched origins/results,
queued cancellation, canceled delivered owners, duplicate-disposal guards,
hidden protocol replies and background completion, rapid final output,
aggregate shutdown with queued bridge receipts, already-exited focus, and
obsolete/exclusive widget focus operations. Queued preparation uses deterministic
worker gates; tests do not add a production polling subsystem.

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
