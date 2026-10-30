# R29 — live tabs

Branch `issue-29-live-tabs` started from exact merged R28/PR75 main
`b29a7756f06007f4b6f6872f93b2d2038b8519fd`. Original branch
`issue-28-close-policy` and `checkpoint/b-r29-from-r28-394d973` retain
`394d9730315de9d290cc6d00095bfee78caa0a57`. Prior commits, reports, native
failure receipts and caches remain intact; no reset, rebase or force-push.

Independent model checkpoint `2bf91fa` and prepared fixture checkpoint `17c9961`
were preserved before integrating actual merged R25/PR76 main
`52f5f90675c11ce15d19a11554c9105f803056a3` **once**, in merge `6a8b029`.
Shared host work began only after the coordinator released that gate.
Verified, committed and pushed implementation source:
**`b45c4cd726756444617922a07be63dcf33e0e105`**. Later publication adds only
reports and synthetic receipts; exact final head is in the PR/coordinator
handoff. [Provenance](../baselines/issue-29/provenance.json) records source,
binary and artifact identity.

## Behavior and changed files

The existing ordered stable TabIds and independent trees remain canonical.
`Control::NewTab` uses the existing single-use reservation, 32-retained limit
and bounded launch pool. It captures the source launch profile/cwd before
changing selection, including starting/error/exited sources. Existing typed
launch failures remain in redacted retryable panes. Limit refusal is atomic
and explains recovery; toolbar refusal restores typing focus. The last empty
window stays open and can create another terminal.

`Control::ActivateTab(TabId)` restores the remembered pane and acknowledges that
tab's unread/bell/exit flags. The tab row exposes creation, selection and these
indicators. Live hidden sessions keep their subscriptions, parsing and protocol
replies. Native completion releases resources and retains final output/status
without selecting the hidden tab. Delayed hidden attachment/split/restart
cannot steal focus; modal/shutdown ownership continues to reject tab controls.

Changed source groups:

- `src/tabs.rs`, its tests, `src/layout.rs`, `src/workspace.rs`, `src/lib.rs`:
  constant-size activity with current-reservation/retained-pane validation;
  removed, closing, unattached and stale owners cannot mark another tab.
- `src/app/tabs.rs`, `src/app.rs`, `src/app/view.rs`, `src/app/startup.rs`:
  complete host controls, inheritance/errors, visible tab bar and scoped focus.
- `src/terminal.rs`: record valid hidden output/bell events; reject delayed
  hidden widget geometry/input/selection/focus receipts.
- `src/app/resize.rs`: transfer already-built bounded presentation state, keep
  active viewport area until its keyed sensor reports layout, and preserve
  R25's frozen fallback. Only lightweight layout state is cached, bounded by
  retained tabs and pruned on removal; hidden trees are never fitted.
- `src/app/tests.rs`, `src/app/tests/tabs.rs`: seven host regressions, including
  explicit native deferred-IME and frozen-presentation cases.
- `examples/live_tabs.rs`, `examples/support/live_tabs_widget.rs`: finite actual
  App/native Iced fixture using a public New tab button, public widget encoder,
  native focus operations and four controlled child processes.

No production polling/background task, dependency, lockfile or vendor change.
The resize algorithms and shared prechild capture/mapper are unchanged; no R26
shortcut work was authored.

## Local verification

All verification ran **locally**, macOS arm64, stable Rust 1.97.1, default wgpu,
locked dependencies, `CARGO_BUILD_JOBS=4`, and this retained worktree's own
writable `CARGO_TARGET_DIR=target`. Native PTY/graphics commands explicitly ran
outside the Codex sandbox. Windows Clippy is cross-compilation, not execution.
Raw receipts and [exact command/result manifest](../baselines/issue-29/commands.json)
are under [issue-29](../baselines/issue-29/).

Current CODEX_HOME matched `/Users/daan.hoedjes/.codex-personal`; both current
and explicit `CODEX_HOME="$HOME/.codex-personal" codex login status` returned
`Logged in using ChatGPT`. No account fallback or global configuration change.
Git used the repository's existing author identity through command-local `-c`
options. Current AGENTS.md, relevant PLAN.md, live issue29 and exact pinned
Iced/iced_term APIs were inspected first.

| Command / scope | Result / receipt |
| --- | --- |
| `cargo test --locked -p pike --lib tabs::tests -- --nocapture --test-threads=1` | **4 passed** on source-identical model checkpoint `2bf91fa`; stable order/focus/nested ratios, coalescing, stale owners and background/fallback exit. |
| `cargo test --locked -p pike --lib workspace::tests -- --nocapture --test-threads=1` | **16 passed** at `2bf91fa`; generated transactions, retained limit, close/move/restart/zoom. These successful checks are reused, not repeated for another receipt. |
| `cargo build --offline --locked -p pike --example live_tabs` | Passed; `fixture-build.log`. |
| `cargo test --offline --locked -p pike --lib app::tests::tabs:: -- --nocapture --test-threads=1` | **5 passed**, before adding the two joint tests; `native-tab-tests.log`. |
| Selected `switching_frozen` / `tab_switch_invalidates` tests with `--include-ignored` | **2 passed natively**, including graphics/IME; `native-joint-tab-tests.log`. |
| `target/debug/examples/live_tabs docs/baselines/issue-29/native-development-1` | **Passed first run**; `native-development-1.log`, `completed.txt`, `events.txt`, bounded synthetic child files. |
| Affected split/resize/queued-focus/modal-attachment tests | **12 passed, 1 explicitly ignored** graphics sibling-paste neighbor; `native-host-neighbors.log`. Its unchanged prior evidence is reused; the new tab IME graphics test ran explicitly above. |
| Explicit iced_term same-size widget identity / hidden native output-activation tests | **2 passed natively**; `native-adapter-neighbors.log`. No vendor patch. |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example live_tabs -- -D warnings` | Passed; `clippy.log`. |
| Same Clippy command with `--target x86_64-pc-windows-msvc` | Passed; `windows-cross-clippy.log`. |
| `cargo fmt --package pike -- --check` | Passed; `format.log`. |
| Source/report `git diff 52f5f906 --check -- src examples docs/verification` | Passed; `diff-check.log`. Byte-preserved raw receipts remain outside this source/report whitespace check. |

The native fixture observed distinct child PIDs 44617/44620/44628/44631,
inherited profile/cwd, matching **128×32** initial terminals and nested **63×15**
panes. Switching away before attachment preserved focus with **0 snapshots /
0 viewport cells** in the hidden attachment. Hidden output produced 64 updates
and two cursor queries; exact received bytes were `ESC[2;3R ESC[4;5R` without
the separator. Unread/bell coalesced; hidden snapshot, grid-resize and cache-
invalidation deltas stayed **0**. Actual App widget traversal contained only the
visible terminal, so there was no hidden widget building canvas geometry.
Delayed hidden Write/Resize/focus receipts were rejected.

Seven activations each produced **one immediate current snapshot**, with
synchronous dispatch **223–250 µs**. Native widget operations confirmed remembered
focus. Pane/split IDs, ratios, widgets, processes, history, selection,
application-cursor/bracketed-paste modes and same-size dimensions survived.
Public widget input was exactly `active29\r` in the selected nested child,
with zero added input in the inactive child. Hidden exit retained code **37**,
acknowledged native cleanup, set its exit flag, built no snapshot, preserved
active focus and displayed its final sentinel on activation. Confirmed aggregate
shutdown used the normal App cleanup acknowledgement.

These are individual debug synthetic/macOS samples, not release percentiles,
CPU/RAM budgets or physical input verification. The harness has bounded input,
a 20-second scenario deadline and 25-second failed-cleanup escape. No oracle or
deadline was relaxed, and all executed checks passed first run; later source
changes added only the two joint tests. No development failure was discarded.

## Reused evidence and limits

Existing [R23](issue-23.md), [R24](issue-24.md), [R28](issue-28.md),
[native R28 fix](issue-28-native-close-fix.md) and
[merged R25 integration](issue-25-main-integration.md) evidence is retained for
unchanged algorithms. No full-workspace, both-renderer or unrelated performance
suite ran. Existing `iced_winit::try_next` deprecations and `block 0.1.6`
future-compatibility notice remain in logs.

Actions is **OFF**: no enable/dispatch/rerun or replacement CI. Native
Windows/ConPTY, Linux, physical keyboard/monitor and release RAM/CPU verification
remain unavailable. The #13/#14 physical holds and explicit #14 scheduling waiver
remain unchanged. Rename/reorder/overflow polish (R30), command remapping
(R26/R32), layout persistence and a second PTY remain outside R29. Independent
review and coordinator-owned merge remain separate gates; this author has not
merged its own PR.
