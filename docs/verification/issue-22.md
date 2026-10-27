# Issue #22: independent workspace, layout and session state

Based on `origin/main` `dc50610e0f3bc6c47087cc52bb8cca78017f576f`, on
`orchestrator/issue-22-state-model` in `.worktrees/trial-issue-22`.
The supplied issue, architecture excerpt and repository rules are the baseline;
this worktree contains neither AGENTS.md nor PLAN.md. No other worktree was used.

## API and ownership

`workspace::Model` is the pure transaction boundary. It owns separate
`Workspace` and `SessionRegistry` values and exposes shared references to both.
Workspace owns ordered TabIds, a tab map and an optional active tab. Each
`layout::TabState` owns its canonical `pane_grid::State<PaneState>` and focused
pane; zoom is stored only in the grid. PaneState contains only SessionId.
Neither layout nor registry owns a Terminal, prepared backend, process, OS
handle, worker, timer, transcript, launch command or environment.

`identity` unifies the existing launch allocator and the new state allocator.
SessionId, RequestId and TabId use one process-wide checked AtomicU64 counter.
A reservation pairs the request/session IDs as existing launch callers do.
IDs are never reconstructed from pane handles, reused, reset per model, or
wrapped. Exhaustion permanently returns an error; IDs consumed by a rejected
transaction may be skipped. The existing `launch::{SessionId, RequestId}`
public paths re-export the exact shared types. Existing `LaunchPool::start`
callers still allocate fresh IDs after bounded admission. `start_reserved`
uses a claimed reservation's exact IDs through the same admission/worker path.

Commands and results:

- `create_tab` and `split` reserve one Starting entry and return its scoped
  PaneTarget and `Effect::Spawn(Reservation)`. The limit (default 16) counts
  every retained entry, including Starting, Closing, Exited and Failed.
  Invalid targets and exhausted budgets leave layout/registry unchanged.
- `focus` selects a pane and activates its tab. Selecting a hidden pane restores
  the full layout; refocusing the maximized pane preserves zoom. `activate`
  changes tabs while retaining their valid focus/zoom selection.
  `swap` moves positions within a tab while retaining pane handles.
  `move_to_tab` splits the destination with the existing session before
  removing the source and returns its new scoped handle. Within the same
  tab it swaps positions. No layout-only method can return a lifecycle effect.
- `resize` validates a tab-scoped Split and finite ratio in [0, 1], then
  clamps to [0.1, 0.9]. `zoom` validates a scoped pane and toggles grid zoom.
  It always focuses that pane within its tab; zooming a background tab does not
  activate it.
- `close_pane`/`close_tab` remove leaves immediately and issue precisely one
  `Effect::Close(Reservation)` per Starting/Running session. Close means cancel
  this startup attempt or close this attached session. Closing entries remain
  budgeted until complete disposal is acknowledged with `closed(SessionId)`.
  Repeated stale closes return errors and issue no effects. Closing an already
  Failed/Exited pane removes its entry without an OS close effect.
- Final-pane removal deletes the tab explicitly; pane_grid::close is never
  asked to remove its final leaf. Removing an active tab selects its next tab,
  or its previous tab at the end. Removing a background tab preserves the
  active tab. An empty workspace is valid with active=None, even while
  orphaned Closing entries await acknowledgement.
- `accepts_spawn` validates a pending reservation before UI attachment.
  After attachment succeeds, `spawn_succeeded` commits Starting to Running
  and returns Attach. It classifies stale, duplicate, failed or canceled
  successful results as Dispose without attaching them.
  Dispose refers to the supplied prepared result, never the current attached
  backend. A canceled successful result still occupies its Closing slot until
  disposal acknowledgement. A contradictory late failure cannot release it.
- `spawn_failed` accepts a matching Starting reservation (retaining a Failed
  placeholder) or settles a canceled reservation still awaiting its first
  result. It rejects mismatched requests and failures after successful launch.
  `exited(SessionId, Option<i32>)` transitions only Running to Exited and retains
  its pane, including in background tabs. None does not imply successful exit.

Every tab maintains `grid.maximized().is_none_or(|pane| pane == focused)`.
Successful splits and cross-tab insertions restore the destination's full
layout and focus the new leaf. Swaps preserve pane handles, focus and zoom.
Removing or moving the zoomed source restores its remaining layout and selects
a valid sibling; removing a hidden sibling preserves the visible focused pane.
Failed split/move transactions preserve the original selection. These rules
also apply to background tabs, so later activation cannot select hidden input.

The current single-terminal UI is not integrated with the new workspace.
Profile/dimension validation, native lifecycle implementation, rendering,
input, snapshots, clipboard and application shutdown remain outside this issue.

## Executing a reserved launch

`Effect::Spawn` remains Copy metadata, not launch authority. The adapter calls
`Model::claim_launch(reservation)` immediately before submission, on the same
update thread. A matching Starting entry can issue exactly one opaque
`LaunchToken`; a boolean in that bounded entry guards issuance. Stale,
mismatched, failed, closed and already-claimed effects return None. Moves do
not change this authority. Tokens have no public constructor and implement
neither Clone nor Copy; there is no consumed-ID set or global per-attempt state.

`LaunchPool::start_reserved(token, settings)` consumes this token, including
on rejection. It does not allocate IDs. Its LaunchRequest, PreparedSession,
PreparedTerminal, attached Terminal and backend events all retain the model's
request/session identity. Both pool entry points use the same runtime check,
job/session permits, cancellation and disposal code. In particular, failure to
acquire session capacity releases any acquired job permit, and rejection queues
no native preparation work.

The adapter must settle every outcome explicitly:

1. Before claiming, a canceled/stale effect cannot obtain a token. Ignore
   rejected replays: their original owner still reports the outcome; a duplicate
   effect must not mark an active launch Failed. When processing Close, the
   adapter can settle a known unlaunched attempt with
   `spawn_failed(ids, Failure::Canceled)`. An abandoned claimed token has no
   native resource; its owner reports cancellation rather than leaving Starting.
2. Claim and submit synchronously in one update. On admission error, report
   `spawn_failed(ids, Failure::Admission)`. The claim stays spent; close the
   Failed placeholder and reserve fresh IDs for retry. A token is not returned
   for resubmission to this or another pool.
3. After submission, retain the request's existing cancellation handle. Close
   cancels that exact attempt; dropping the receive future also cancels it.
   Queued work checks cancellation before preparation; in-progress preparation
   can complete late and is disposed by the pool. Pool permits stay occupied
   through native disposal, independently of model failure notification.
4. On a prepared success, check `accepts_spawn(ids)` before attaching. If false,
   classify it with `spawn_succeeded` as Dispose and drop the supplied prepared
   owner. `PreparedSession::shutdown_handle()` exposes existing cleanup
   observation so the adapter can save it before dropping, await disposal
   asynchronously and then acknowledge `closed(id)`. Do not close a currently
   attached session merely because a duplicate result has the same IDs.
5. If still pending, attach first and commit with `spawn_succeeded` in the same
   update, without an intervening await or model mutation. Attachment failure
   is reported with `spawn_failed` while the entry is still Starting; it must
   not leave a falsely Running entry. In particular, cancellation racing font
   attachment is handled by the existing guarded handoff. Backend child exit
   alone is not a disposal acknowledgement.

Tokens deliberately do not own the model or automatically mutate it on Drop.
Claimed tokens should not be queued across updates: after authority is issued,
the adapter is responsible for reporting abandonment or forwarding cancellation
to the submitted LaunchRequest. Late results remain checked by both IDs.

## Invariants and deterministic verification

After every generated operation, tests independently traverse the actual grid
tree, pane map, tab order, active/focused/maximized handles and registry.
Every leaf has one existing entry, every displayed session is unique, all
non-Closing entries have exactly one pane and Closing entries have none.
If a tab is maximized, that pane must equal its focused pane. The central
checker asserts this relationship after every generated operation.
All retained resources are bounded by the model's configured entry limit.

The generated test uses 32 fixed seeds x 2,048 operations = 65,536 steps,
including stale handles/results, cross-tab moves, closes, failures and exits.
It also checks monotonically increasing session IDs and at-most-once close
effects. Focused fixtures cover equal Pane(0) handles in different tabs,
last-pane/tab removal, background exit, failed/canceled reservations, late and
duplicate results, stale splits, invalid ratios, movement during startup,
and grid allocator exhaustion without losing a source session. Zoom fixtures
cover zoom(A) then focus(B), zooming a nonfocused pane, background focus and
activation, and split/move/swap/removal or failed insertion while zoomed. The actual
shared ID allocation helper is tested at u64 exhaustion without changing the
process-global counter. Launch tests interleave pure and LaunchPool allocations
and execute a model effect through real bounded preparation, attachment and the
Terminal subscription's backend completion event, asserting exact IDs throughout.
Further fixtures verify one-time claims, stale/mismatched effects, all admission
rejection paths and permit rollback, abandonment, cancellation with late success,
a fresh retry, and cancellation during attachment before committing the model.
Two compile-fail doctests enforce token non-clonability and use-after-move
rejection. The existing generated sequence also exercises one-time claims.

The inspected lockfile selects iced 0.14.0, iced_widget 0.14.2 and iced_term
0.8.0. APIs were checked in installed pane_grid state/node sources and the
existing compatibility example. In particular, Iced's fallible split is used
before removing a move source; its drop helper closes first, so it is avoided.

## Checks and measurements

Host: Windows build 26200, x86_64 MSVC, rustc 1.97.1 / cargo 1.97.1.
All Cargo output is confined to this worktree's default target directory.

| Check | Final result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass |
| `cargo test --locked` | Pass: 38 library + 2 app tests + 2 compile-fail doctests; 1 existing measurement ignored |
| `cargo test --locked --lib workspace::tests` | Pass: 13 tests, including all 65,536 generated steps |
| `cargo test --locked --lib launch::tests` | Pass: 20 tests; 1 existing measurement ignored |
| `cargo build --locked --release --bins --example state_model_measure` | Pass |
| Release measurement executable | Exit 0 |

Before the final checks, `tools/prepare-conpty.ps1` restored the pinned runtime
from this worktree's cached package into debug/release and their deps/examples
directories. It verified archive SHA-256
`02b07b349af66d801159bdf9e440d4a1ce78bb951f37fc8609731665afdae7ee`
and the manifest's native file hashes. No runtime fallback, fixture bypass or
security setting was changed. The native tests exercise only the existing
synthetic cmd.exe fixture and adapter contract, not new UI behavior.

The reproducible `examples/state_model_measure.rs` driver executes no effects
and opens no window or PTY. It measures 20,000 samples per operation group at
16, 128 and 1,024 synthetic sessions across two tabs. Groups are cross-tab
move; focus(A) + zoom(A) + focus(other) + resize; and split + claim + close +
disposal acknowledgement. The focus group exercises restoring a hidden pane's
full layout after zoom on every sample.
The latter retains at most one temporary extra reservation. Nanosecond values
include Instant timing overhead, allocation and scheduler noise. Process peak
memory includes the executable, allocator and timing samples; inline type
sizes exclude map/tree allocations and are not total per-session memory.
These measurements establish no GUI latency, idle CPU, PTY memory or Windows
interaction claim. No dependency was changed, so dependency test suites were
not run.

One release run (nanoseconds, including timer overhead):

| Sessions | Operation group | p50 | p99 | Maximum |
| ---: | --- | ---: | ---: | ---: |
| 16 | Cross-tab move | 300 | 300 | 7,300 |
| 16 | Focus + zoom + focus other + resize | 100 | 100 | 6,700 |
| 16 | Split + claim + close + acknowledgement | 400 | 500 | 5,000 |
| 128 | Cross-tab move | 300 | 300 | 6,900 |
| 128 | Focus + zoom + focus other + resize | 100 | 100 | 3,700 |
| 128 | Split + claim + close + acknowledgement | 500 | 500 | 5,000 |
| 1,024 | Cross-tab move | 300 | 400 | 19,800 |
| 1,024 | Focus + zoom + focus other + resize | 100 | 100 | 3,100 |
| 1,024 | Split + claim + close + acknowledgement | 500 | 700 | 9,600 |

These are 180,000 measured operation groups overall. Observations quantized to
100 ns; a zero result is below timer resolution, not zero execution time.
Inline sizes were PaneState=8, SessionEntry=32 and Model=96 bytes. The bounded
claim guard adds 8 bytes per entry after alignment (previously 24), with no new
heap allocation, native handle, timer or background worker.
An external PowerShell System.Diagnostics.Process wrapper, with redirected
output and CreateNoWindow, observed a kernel peak working set of 4,595,712
bytes using 10 ms sampling. This is an observed lower bound on the final
process peak, not attributable state heap usage; no per-pane heap claim is
made. Total process CPU time was 62.5 ms, including setup and sample sorting.
This measures active synthetic work, not idle CPU. The driver retains no
measurement hooks in production state and uses only synthetic IDs/metadata.
