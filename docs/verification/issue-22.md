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
public paths re-export the exact shared types. Existing LaunchPool callers
still allocate through the same contract. No startup/PTY behavior was changed.

Commands and results:

- `create_tab` and `split` reserve one Starting entry and return its scoped
  PaneTarget and `Effect::Spawn(Reservation)`. The limit (default 16) counts
  every retained entry, including Starting, Closing, Exited and Failed.
  Invalid targets and exhausted budgets leave layout/registry unchanged.
- `focus` selects a pane and activates its tab; `activate` changes tabs.
  `swap` moves positions within a tab while retaining pane handles.
  `move_to_tab` splits the destination with the existing session before
  removing the source and returns its new scoped handle. Within the same
  tab it swaps positions. No layout-only method can return a lifecycle effect.
- `resize` validates a tab-scoped Split and finite ratio in [0, 1], then
  clamps to [0.1, 0.9]. `zoom` validates a scoped pane and toggles grid zoom.
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
- `spawn_succeeded` validates both IDs. Starting becomes Running and returns
  Attach; all stale, duplicate, failed or canceled completions return Dispose.
  Dispose refers to the supplied prepared result, never the current attached
  backend. A canceled successful result still occupies its Closing slot until
  disposal acknowledgement. A contradictory late failure cannot release it.
- `spawn_failed` accepts a matching Starting reservation (retaining a Failed
  placeholder) or settles a canceled reservation still awaiting its first
  result. It rejects mismatched requests and failures after successful launch.
  `exited(SessionId, Option<i32>)` transitions only Running to Exited and retains
  its pane, including in background tabs. None does not imply successful exit.

These effects are contracts for a future app adapter. They are not executed
here. That adapter must preserve reserved IDs, enforce bounded backend
admission, dispose rejected prepared owners and acknowledge full cleanup.
The current single-terminal app and LaunchPool entry points are not integrated
with the new workspace. Profile/dimension validation, native lifecycle,
rendering, input, snapshots, clipboard and application shutdown are outside
this issue.

## Invariants and deterministic verification

After every generated operation, tests independently traverse the actual grid
tree, pane map, tab order, active/focused/maximized handles and registry.
Every leaf has one existing entry, every displayed session is unique, all
non-Closing entries have exactly one pane and Closing entries have none.
All retained resources are bounded by the model's configured entry limit.

The generated test uses 32 fixed seeds x 2,048 operations = 65,536 steps,
including stale handles/results, cross-tab moves, closes, failures and exits.
It also checks monotonically increasing session IDs and at-most-once close
effects. Focused fixtures cover equal Pane(0) handles in different tabs,
last-pane/tab removal, background exit, failed/canceled reservations, late and
duplicate results, stale splits, invalid ratios, movement during startup,
and grid allocator exhaustion without losing a source session. The actual
shared ID allocation helper is tested at u64 exhaustion without changing the
process-global counter. A launch test interleaves pure and LaunchPool
reservations to verify the shared types, monotonicity and compatibility.

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
| `cargo test --locked` | Pass: 28 library + 2 app tests; 1 existing measurement ignored; 0 doc tests |
| `cargo test --locked --lib workspace::tests` | Pass: 8 tests, including all 65,536 generated steps |
| `cargo test --locked --lib identity::tests` | Pass: 1 test |
| `cargo test --locked --lib launch::tests` | Pass: 15 tests; 1 existing measurement ignored |
| `cargo build --locked --release --bins --example state_model_measure` | Pass |
| Release measurement executable | Exit 0 |

An initial full test run failed eight existing launch fixtures because this
fresh target directory lacked the packaged ConPTY pair. The repository's
preparation script failed to download it ("Unable to connect to the remote
server"). Before the final runs, the expected native files became available
in target/debug/deps; the explicit launch rerun and full suite then passed.
No runtime fallback, fixture bypass or security setting was changed.

The reproducible `examples/state_model_measure.rs` driver executes no effects
and opens no window or PTY. It measures 20,000 samples per operation group at
16, 128 and 1,024 synthetic sessions across two tabs. Groups are cross-tab
move; focus + zoom + resize; and split + close + disposal acknowledgement.
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
| 16 | Cross-tab move | 200 | 300 | 46,500 |
| 16 | Focus + zoom + resize | 0 | 100 | 4,300 |
| 16 | Split + close + acknowledgement | 300 | 900 | 15,800 |
| 128 | Cross-tab move | 300 | 300 | 7,800 |
| 128 | Focus + zoom + resize | 100 | 100 | 500 |
| 128 | Split + close + acknowledgement | 400 | 500 | 15,000 |
| 1,024 | Cross-tab move | 300 | 400 | 20,000 |
| 1,024 | Focus + zoom + resize | 100 | 400 | 11,800 |
| 1,024 | Split + close + acknowledgement | 500 | 1,200 | 16,300 |

These are 180,000 measured operation groups overall. Observations quantized to
100 ns; a zero result is below timer resolution, not zero execution time.
Inline sizes were PaneState=8, SessionEntry=24 and Model=96 bytes.
An external PowerShell System.Diagnostics.Process wrapper, with redirected
output and CreateNoWindow, observed a kernel peak working set of 4,575,232
bytes using 10 ms sampling. This is an observed lower bound on the final
process peak, not attributable state heap usage; no per-pane heap claim is
made. Total process CPU time was 78.125 ms, including setup and sample sorting.
This measures active synthetic work, not idle CPU. The driver retains no
measurement hooks in production state and uses only synthetic IDs/metadata.
