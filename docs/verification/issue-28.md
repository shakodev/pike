# R28 — scoped close, retained exit, restart and window shutdown

Implemented on actual merged main
`5d34cb74b99f5e7e6865789f645e5fe519ba2d94` (includes #24 at `609d702`).
Verified source commit: `7667a6dbcbdbfcb7b90de689ee5d2bcc99fccbf9`.
The final PR head additionally records this report and evidence without changing
that source. PR: https://github.com/shakodev/pike/pull/75; no merge performed by B.

## Ownership and behavior

`src/app/close.rs` owns the close transaction, decision tokens and one-shot
three-second shutdown deadline. Its view helper provides actual Cancel/Confirm,
Wait/Force and stable-session close buttons. Confirmation snapshots the precise
SessionIds, counts Starting/Running sessions, defaults to Cancel, and resolves
the same owners at confirmation even if layout coordinates have changed. Exited
and failed sessions do not prompt. Last-pane removal delegates to the model's
last-tab/empty-window rules and never automatically opens a replacement shell.

While a decision is visible, the workspace view is replaced and queued user
terminal commands are rejected by the app. Protocol, output, launch and disposal
events continue. Cancel restores the previous live session's focus. Decision
and timer tokens reject old queued actions. Window close disables work, signals
owned sessions and LaunchPool, and keeps the executor alive until its aggregate
native disposal receipt. A read-only pool observation also rejects premature
shutdown receipts. Timeout is never a cleanup receipt. Wait starts another
one-shot deadline; Force exits this application process, bypassing a possibly
blocked runtime destructor without searching or killing processes by name.

Natural exit already routes through the merged stable registry/terminal owner
and retains read-only final output, configured bounded history and exit code.
R28 verifies that contract and integrates the merged in-place fresh-ID Restart.
No second PTY, runtime, production polling, vendor input/IME changes, or new
creation/settings feature is introduced. The Windows Threading feature added
to the existing target-specific dev dependency is used only for pre-close
observation handles in the explicit attached-tree fixture.

## Preflight and preservation

- Worktree: `pike-impl-b-r14`; initial clean detached `be04b8d` is preserved by
  `checkpoint/b-issue28-be04b8d`. Initial base is merged main
  `e58838a828c9deabaa340f7119d9ce38374b4447`, fetched before branch creation.
- `CODEX_HOME=/Users/daan.hoedjes/.codex-personal` was verified and
  `CODEX_HOME="$HOME/.codex-personal" codex login status` returned
  `Logged in using ChatGPT`. No work-account fallback was used.
- #14/PR63 refs, old unpublished branches, evidence and target caches were not
  reset, dropped or deleted. #13/#14 physical holds and #14 scheduling waiver
  remain unchanged.
- The development implementation is preserved at
  `checkpoint/b-r28-development` =
  `2eba691987fedf0551c68461b0a394ec6a7d05e5`; further fixture expansions are
  separately retained before integration. The failed initial commit attempt
  (missing local Git email) changed no files; command-local identity then reused
  the repository's existing public `shakodev@users.noreply.github.com` identity,
  without changing global configuration or Codex authentication.
- This slot is local macOS arm64. The coordinator confirmed no targetable native
  Windows environment exists. No worker/terminal/helper or remote CI was added;
  Actions remains OFF.

## Development evidence before integration

All commands use `CARGO_BUILD_JOBS=4`, locked dependencies and the existing
worktree target. Logs are retained under `docs/baselines/issue-28/`.

| Command | Result / log |
| --- | --- |
| `cargo check --locked -p pike --lib` | Passed; `development-check.log`. |
| `cargo check --locked -p pike --tests` | Passed; `development-tests-check.log`. |
| `cargo test --locked -p pike --lib app::close::tests:: -- --test-threads=1` | Eight initial policy tests passed; `development-policy-tests.log`. |
| `cargo test --locked -p pike --lib app::tests::closing:: -- --nocapture --test-threads=1` | Three native shell tests passed, one graphics test explicitly ignored; `development-native-tests.log`. |
| `cargo test --locked -p pike --lib app::tests::closing::actual_modal -- --ignored --nocapture --test-threads=1` | Sandboxed graphics unavailable (`development-controls.log`); native renderer exposed a fixture assertion error on hidden snapshots (`development-controls-native.log`); corrected assertion passed (`development-controls-corrected.log`). |
| `cargo test --locked -p pike --lib app::close::tests::force_exit_is_scoped -- --nocapture --test-threads=1` | Passed; `development-force.log`. Same-executable witness remained alive. |
| `cargo test --locked -p pike --lib -- app::tests::cancellation:: app::close::tests::force_exit_is_scoped --nocapture --test-threads=1` | Three expanded pane/window-prepare/Force tests passed; `development-prepare-force.log`. Force bypassed an owned blocking runtime job (19 ms subprocess sample). |
| `cargo test --locked -p pike --lib app::tests::closing::actual_modal -- --ignored --nocapture --test-threads=1` (native graphics) | Expanded mouse Cancel/Confirm/Wait/Force fixture passed; `development-all-controls.log`. Force effect is inspected, with actual process exit verified separately. |
| `cargo test --locked -p pike --lib app::tests::closing::modal_accepts -- --nocapture --test-threads=1` | Launch and natural completion during a decision passed; `development-modal-completion.log`. |
| `cargo test --locked -p pike --lib app::close::tests::real_keyboard_subscription -- --test-threads=1` | Initial fixture import error retained in `subscription-red.log`; real production-subscription instantiation reproduced Iced's captured-mapper E0080 in `subscription-capture-red.log`. Changed to `.with(token).map(non_capturing_mapper)`; real ignored-key stream, default Cancel and changed per-decision recipe identity passed in `subscription-green.log`. No dependency was added. |

Measured debug synthetic/macOS development samples: output close dispatch
217 µs, acknowledged cleanup 603 ms; held-reader close dispatch 115 µs,
actual three-second deadline 3,006 ms, Wait dispatch 9 µs, controlled completion
after FIFO release 3,008 ms total; application-only Force subprocess oracle
18 ms. These are individual fixture observations, not a latency percentile,
release budget, Windows measurement or physical keyboard/monitor check.

## Final integration, acceptance and local verification

One deliberate rebase integrated actual merged #19/#24, after preserving the
published `6a8d678f111489583bf79514f57f15eeccc31077` under
`checkpoint/b-r28-published-preintegration`; no old checkpoint was dropped.
The merged `Model::target`, in-place Restart, launch profile/focus maps and common
`App::select`/`focus_active` path are reused. The location-only public close
control was removed. Close buttons use stable SessionIds, including each pane's
header, so background and exited/error panes can be closed directly.
`App::interaction_blocked()` is the agreed boolean for future R25 capture;
queued workspace controls and geometry receipts are suppressed while a modal or
teardown owns interaction. No R25 or other unmerged feature was imported.

The actual merged IME event is `BackendCommand::ImeCommit`. The app guard calls
its public `cancel()` when rejecting modal/teardown input, invalidating every
held clone. Native tests obtain commits from the real public widget, forge stale
backend focus during a modal, restore focus after Cancel, reject old clones,
and accept a fresh commit exactly once afterward. Controlled lines are completed
before cleanup. Vendor IME, pointer and terminal input gates are unchanged.

| Acceptance | Evidence |
| --- | --- |
| Natural background exit is origin-bound and keeps focused siblings/tabs | Four routing cases, final sentinel/history-cap tests, and native application rapid-exit case (code7, drained final sentinel, registry0). |
| Cancel preserves targets; one counted confirmation; stable exact owner set; idempotence | Pure transaction cases include Starting/Running count, default Cancel, moved owners, stale decisions, repeats and background completion during a decision. |
| Exited/Failed close without prompt; last pane/tab becomes empty without respawn | Pure state cases plus native final-output close, with Closing budget retained until disposal. |
| Restart replaces in place with a fresh ID and rejects old events | Controlled live sibling test replays old close, retry, launch, disposal and terminal messages against the replacement. |
| Normal window exit waits for every owned resource | Real pane/window cancellation during native preparation, held cleanup receipt, output-close completion, and aggregate pool acknowledgement; premature completion message cannot exit. |
| Responsive three-second Wait/Force; application-only force | Real FIFO-held reader cleanup, real one-shot deadline and responsive Wait dispatch; force subprocess bypasses an owned blocked runtime job while a same-executable witness survives. Actual mouse Cancel/Confirm/Wait/Force controls pass. |
| Modal input and focus restoration | Native widget/IME test described above plus queued keyboard/mouse/local-history authority cases. |
| Windows attached descendants | Fixture authored and cross-compiled/linted; native root/child/grandchild execution remains UNPERFORMED. |

All verification below ran **locally**, macOS arm64, Rust/Cargo versions and exact
source-tree object IDs in `final-provenance.json`, `CARGO_BUILD_JOBS=4`, locked
dependencies, existing target cache, default wgpu only. No remote CI ran.

| Command | Actual result / retained log |
| --- | --- |
| `cargo test --locked -p pike --lib -- app::close:: app::tests::closing:: app::tests::cancellation:: app::tests::aggregate_shutdown app::tests::split::failed_split --nocapture --test-threads=1` | 19 passed; three explicitly ignored entries (two native graphics cases run below, one child fixture invoked only by its parent). `final-host-tests.log`. |
| `cargo test --locked -p pike --lib app::tests::closing:: -- --ignored --nocapture --test-threads=1` (native graphics access) | Both actual-control and held-IME cases passed. `final-native-controls-ime.log`. |
| `cargo test --locked -p pike --lib -- app::tests::interaction:: app::tests::all_live_entries --nocapture --test-threads=1` | Four passed, including background exit and normal input/history ownership. `final-routing-tests.log`. |
| `cargo clippy --locked -p pike --lib --bin pike --tests --example first_session_host -- -D warnings` | Passed. `final-clippy-native.log`. |
| Same Clippy command with `--target x86_64-pc-windows-msvc` | Passed, including Windows attached-tree fixture compilation. `final-clippy-windows.log`; not Windows execution or linking. |
| `cargo clippy --locked -p pike --lib -- -D warnings`, and the same with Windows target | Passed after the final compact pane-header close-button wiring; only the affected library lint was repeated. `final-view-clippy-native.log`, `final-view-clippy-windows.log`. |
| `cargo fmt --package pike -- --check`; `git diff --check` | Passed. `final-format.log`. |
| `cargo run --locked -p pike --example first_session_host -- success target/issue-28/host-success-actual19` | FAILED its ten-second synthetic input deadline; real cleanup still acknowledged registry0. Log and metadata retained as `final-native-application.log` and `native-application-failure-events.txt`. No oracle or focus gate weakened. |
| `cargo run --locked -p pike --example first_session_host -- rapid target/issue-28/host-rapid-actual19` | Passed actual Iced application natural exit: code7, final sentinel, output drained, registry0; `final-native-application-rapid.log` and `native-application-rapid-events.txt`. |

The additional GUI input probe records widget focus before sending typed bytes,
but does not wait for owning-window focus; the unchanged host requires both.
Its missing captured-key file and deadline are consistent with that readiness
mismatch, **not proof of the exact event ordering**. The focused native routing
reproducer verifies that unfocused queued input is rejected and focused input
works; the rapid application case isolates natural-exit/window cleanup without
that injection prerequisite. Full GUI synthetic-input readiness remains a
fixture limitation; no production focus policy was loosened to make it pass.

Final **debug, synthetic/macOS** samples (single observations, not percentiles):
output-close dispatch134µs/acknowledged cleanup604ms; held-reader close46µs,
real deadline3,002ms, Wait19µs, completion after release3,005ms total;
application-only Force subprocess20ms. These are not release, Windows, physical
keyboard/monitor or whole-application responsiveness budgets.

Changed production files are `src/app.rs` (integration), `src/app/close.rs` and
`src/app/close/view.rs` (policy/controls), `src/app/view.rs` (stable close controls),
and `src/launch.rs` (read-only native-ownership completion observation).
`Cargo.toml` adds only Windows process-observation dev features. Tests and the
existing native application fixture, plus this report/logs, provide evidence;
the existing IME test helper only gains sibling-test visibility.

## Remaining limitations and preserved holds

No native Windows/ConPTY execution, physical keyboard/monitor/DPI test, release
CPU/RAM budget or GUI-input-readiness success is claimed. The coordinator
confirmed no targetable Windows host. To execute the authored native Windows
probe later, use:

```powershell
cargo test --locked -p pike --lib app::tests::closing::windows::native_attached_tree_cleanup -- --ignored --nocapture --test-threads=1
```

That probe observes the attached root/child/grandchild with handles acquired
before close, awaits the native pool receipt, and checks each observation
handle is signaled. It still needs native execution on supported Windows cohorts.
Intentionally detached descendants are not promised to terminate. Force exits
before native cleanup acknowledgement and is separate from normal close.

The known Unix partial-line shutdown issue remains outside scope. Controlled
fixtures use completed lines or independent FIFO gates; no timeout or cleanup
oracle was relaxed. #13/#14 physical holds and #14 scheduling waiver are intact.
There is no vendor-source diff from actual integrated main, so no patched-vendor
suite was required or repeated for R28. The two existing `iced_winit::try_next`
deprecation warnings and `block0.1.6` future-compatibility notice are retained in
logs; Pike's requested Clippy `-D warnings` checks passed. No full-workspace,
alternate-renderer or remote CI checklist was used, and Actions remains OFF.
