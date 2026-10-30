# R28 — scoped close, retained exit, restart and window shutdown

Implementation and final integration are in progress. This report preserves
development provenance; the final commands/head and acceptance accounting will
be filled after actual #24 and #19 merges are integrated once.

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

Measured debug synthetic/macOS development samples: output close dispatch
217 µs, acknowledged cleanup 603 ms; held-reader close dispatch 115 µs,
actual three-second deadline 3,006 ms, Wait dispatch 9 µs, controlled completion
after FIFO release 3,008 ms total; application-only Force subprocess oracle
18 ms. These are individual fixture observations, not a latency percentile,
release budget, Windows measurement or physical keyboard/monitor check.

## Final verification and limitations

Pending final integrated source checks. No Windows/ConPTY execution, physical
keyboard/monitor/DPI test, release CPU/RAM budget, or additional-shell performance
claim is made. The explicitly ignored Windows app-level fixture observes its
attached root/child/grandchild with handles acquired before close, awaits the
native pool receipt, and checks each process handle is signaled; it still needs
native execution on supported Windows cohorts. Intentionally detached
descendants are not promised to terminate. Force deliberately exits before a
native cleanup acknowledgement and is separate from normal close.

The known Unix partial-line shutdown issue remains outside scope. Controlled
fixtures use completed lines or independent FIFO gates; no timeout or cleanup
oracle has been relaxed. Vendor sources have not changed, so no patched-vendor
test suite is required by this implementation. No full-workspace or alternate
renderer checklist is being used as a routine gate.
