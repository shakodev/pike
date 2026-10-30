# R27 — header pane moves and maximize/restore

Branch `issue-27-pane-move-zoom` starts at actual merged main
`7d715b792c7120b363abc9500ffbfb0837f0a414` (R26 / PR78), including R24, R25,
R28 and R29. Retained R29 branch `issue-29-live-tabs` at
`d6efdb77566a6e82a95cded93c1345493fdf39d4`, checkpoints, reports and target
caches remain intact. No reset, rebase, force push, Actions execution or merge
was performed. The coordinator owns relevant integration and independent review.

## Behavior and source

Pane title headers now use the pinned Iced 0.14 / iced_widget 0.14.2 TitleBar
drag path. The passive title is draggable, while Max/Restore and Close remain
separate header controls. Content dragging continues to select text or emit the
terminal's mouse protocol. A completed header drag emits one focus action and
one layout action, with zero terminal input, including in mouse-reporting mode.
Center drops use pinned State::drop's swap semantics; pane edges and grid edges
use its existing target semantics. The model validates source/destination and
commits only a grid with exactly the same pane/session ownership map.

Gesture receipts carry the scoped pane, current Reservation and a cancellation
serial. Raw/App blur, owner/tab/modal loss, Escape, outside cancellation and
restart cannot revive a held drag after focus returns. An unavailable synthetic
release clears the pinned widget's private drag action while a shared body gate
keeps that cancellation and all header pointer movement out of terminal widgets.
The existing keyboard mapper, divider capture and focus Authority are reused.

Header Max/Restore selects its stable pane through the existing focus consumer
and invokes R26's ToggleZoom. The grid retains the moved split tree and ratios;
restoration keeps the zoomed pane focused. No Terminal, session, launch token,
subscription or PTY is moved/replaced by layout commands. Hidden panes keep
processing output, queries and native exit, with no render snapshot or geometry
work. Restore uses current bounds/cells, final hidden output and existing cleanup.

- `src/workspace/moving.rs` and its module declaration: atomic pinned drop
  transactions, complete region/identity/zoom/stale invariants.
- `src/app/panes.rs` / `panes/{view,gesture,tests}.rs`: extracted pane subtree,
  header controls, gesture admission/cancellation, host error and late-attach tests.
- `src/app.rs`: agreed additive module/control/state/synchronize/handler glue.
- `src/app/view.rs`: only the agreed pane extraction and removed imports/methods;
  root tab/modal composition remains owned by C30.
- `examples/pane_moving*`: finite actual-App/native-PTY fixture, reusing the
  unchanged R26 task/subscription/UserInterface driver.

There is **zero vendor, dependency or lockfile diff** in final R27.

## Selection scope and preserved experiment

Exact selection/copy survives a move without a column-width change. The pinned
Alacritty implementation explicitly clears selection on a real column resize;
Max/Restore and size-changing moves retain that upstream behavior. Fresh content
selection/copy works afterward. General selection persistence through width
reflow is **not claimed**.

The coordinator corrected its initially stronger handoff after auditing issue27
and PLAN rows 112/189. The complete explored two-anchor emulator remap, tests,
ledger, original reds and candidate greens remain separately inspectable on local
checkpoint `checkpoint/b-r27-selection-reflow-candidate` at
`337f2f52b0a7eb225bd88822619ec5fa3cd30aef`. This checkpoint is not part of
R27's branch ancestry. Its native results are exploratory evidence, not final
unpatched-source verification. Final R27 restores every vendor path to main.

The original missing-header-drag failure is retained in
[`native-red`](../baselines/issue-27/native-red/) with model/extraction checkpoint
`67cfd82`. The stronger width-selection oracle's failure is retained in
[`native-expanded-1`](../baselines/issue-27/native-expanded-1/). An initial
unpatched fixture checked backend columns before the post-Max redraw;
[`native-unpatched`](../baselines/issue-27/native-unpatched/) retains that fixture
sequencing failure. The final oracle explicitly drives the redraw before checking
maximized cells; no production sizing rule was changed for it.

## Local verification

All work uses personal `CODEX_HOME=/Users/daan.hoedjes/.codex-personal`;
`CODEX_HOME="$HOME/.codex-personal" codex login status` returned
`Logged in using ChatGPT`. Current AGENTS.md, relevant PLAN/issue text,
merged R25/R26/R29 verification and exact installed APIs were inspected first.
Cargo uses stable Rust, locked/offline dependencies, one default wgpu renderer,
`CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR="$PWD/target"`. Native PTY/graphics execution
runs outside the Codex sandbox. No full-workspace/both-renderer checklist runs.

Verified so far:

| Command / scope | Result |
| --- | --- |
| `cargo test --offline --locked -p pike --lib workspace::moving::tests -- --nocapture` | 3 passed: all pinned edge regions, center/swap, stable reservations/handles/lifecycles, moved zoom tree and atomic stale/self/zoom refusal. |
| `cargo test --offline --locked -p pike --lib app::panes::tests -- --nocapture` on the initial 3-test module | 3 passed; [gesture-tests.log](../baselines/issue-27/gesture-tests.log). |
| Exact `app::panes::tests::moved_starting_owner_attaches_hidden_without_reviving_old_focus_intent` | 1 passed; [late-attach-first.log](../baselines/issue-27/late-attach-first.log). Same App code, no selection/width-reflow dependency; native completion awaited for both children. |
| Exact `app::panes::tests::failed_panes_keep_host_move_zoom_controls_and_stale_controls_are_harmless` | 1 passed on unpatched source; [failed-panes.log](../baselines/issue-27/failed-panes.log). |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example pane_moving -- -D warnings` | Passed on unpatched source; [development-clippy.log](../baselines/issue-27/development-clippy.log). |
| `cargo build --offline --locked -p pike --example pane_moving` and `target/debug/examples/pane_moving docs/baselines/issue-27/native-unpatched-complete` | Passed; [native evidence](../baselines/issue-27/native-unpatched-complete/), [build](../baselines/issue-27/unpatched-complete-build.log). |

The complete unpatched fixture checks center/pane-edge/grid-edge moves, scoped
SessionId/Reservation/widget identity and synthetic PID files; unchanged-column
selection, real-width clearing and restored copy; header-only zero-byte input;
exact SGR content press/release; raw/App blur, owner/modal round trips, Escape,
outside cancel, fresh press and pre-restart release rejection. It verifies
restored/maximized cell counts against actual widget bounds and checks actual
native-rendered green pixels remain inside the moved owner clip before/after a
swap (191,835 controlled pixels in each captured frame).

A real counter continues across moves/zoom/restore (observed tick 2 -> 3). A
maximized-away sibling floods 3,000 fixture lines, receives a real cursor query
reply, exits 37 and drains its final sentinel. Hidden snapshot count stays
**19 -> 19**, viewport cells remain **0**, and geometry/PTY resize/cache counters
stay unchanged. History remains bounded at 2,000 lines. Restore reveals the final
sentinel, correct footer-adjusted cells and remembered focus; repeated redraws
create no pending-output snapshots. Native shutdown receipts are retained for
every attached child, including the exited source and its fresh restart.

The remaining affected R25/R26 native neighbors, cross-Windows Clippy, final
format/diff review and publication receipts will be added before completion.
Previously passing unchanged vendor, geometry, lifecycle and R29 cache evidence
is reused with its original limits; no vendor tests are required by final R27's
zero vendor diff. The separate experiment's explicit vendor test results remain
on its checkpoint and are not presented as necessary final R27 checks.

## Limits

This is native macOS execution with synthetic Iced events, the headless native
renderer, actual App/runtime tasks/subscriptions and real controlled PTYs. It is
not physical keyboard/monitor, a native Windows/ConPTY run or a release resource
benchmark. Windows compilation will be reported separately from native execution.
No release RAM/CPU/latency claim is made. Existing inherited iced_winit deprecation
warnings and the block future-compatibility notice remain in receipts.

Actions remains OFF; workflows are preserved and none are enabled/dispatched.
The #13/#14 physical holds and #14 scheduling waiver remain unchanged. No
cross-tab pane moves, tab rename/reorder/settings, second PTY or background
polling subsystem is included. Independent review and merge belong to the
coordinator/reviewer; this author does not self-merge.
