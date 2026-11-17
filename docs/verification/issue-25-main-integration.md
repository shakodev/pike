# R25 integration with merged R28

Same branch `issue-25-pane-resizing` and [PR76](https://github.com/shakodev/pike/pull/76).
Preintegration head: `a112f9a6cc9bb0aa727dc6e01e4416343907bbd1`, preserved as
`checkpoint/c-r25-before-r28-a112f9a`. Actual fetched main / integration base:
`b29a7756f06007f4b6f6872f93b2d2038b8519fd` (merged PR75).
Tested merge source: `1dd11bb7021094d974104e643b2e478d2e4cad60`, with exactly those
two parents. No rebase, force-push, cache removal, Actions run or PR merge occurred.
The final publication commit adds only reports/receipts; its exact SHA is sent
to the coordinator and Reviewer1 and recorded in the final report under
`target/issue-25-main-integration/`. The committed
[provenance](../baselines/issue-25/main-integration/provenance.json) records
base/source/checkpoint and source/binary SHA-256 hashes.

This supplements the [approved R25 report](issue-25.md), preserving all original
immutable red/green evidence and the merged [R28 cleanup report](issue-28-native-close-fix.md).

## Behavior and changed files

Both `src/app.rs` conflicts combine their invariants: keep `close_policy` and
`resize_layout`; reject backend resize proposals during frozen presentation;
reject user input during modal/shutdown while R28 protocol/lifecycle/rejection
delivery continues; refresh constraints when footer height changes.

R28 hides terminal snapshots while modal. `refresh_resize_layout` now retains
the last bounded presentation during that interval, then reconciles current
pane/session membership and footer metrics after Cancel/Confirm. Discarding it
while cell snapshots are hidden would lose the capped fallback after a decision.
Resize/font admission respects `interaction_blocked()`, and queued resize focus
also checks current composition, window focus and stable session origin.

The view retains one outer prechild keyboard capture with the unchanged mapper
interface, now receiving `interaction_blocked()`. Existing divider/content
pointer guards are unchanged. `close_view()` still returns before constructing
the terminal view, preserving modal Escape/Enter/Tab/Cancel priority and held
IME token cancellation. No R26 mappings or R29 tabs were added.

Changed production files: `src/app.rs`, `src/app/resize.rs`, `src/app/view.rs`.
`src/app/tests/resize.rs` adds one joint regression. The sole release-fixture
integration edit in `examples/pane_resizing.rs` explicitly confirms R28 cleanup.
New native probe source and its preparation script are under
`docs/baselines/issue-25/main-integration/`; the script generates a local
manifest under this worktree's `target`, reusing the original probe lockfile.
Original probe sources/receipts remain unchanged.

## Local checks

All verification ran locally on macOS arm64, stable Rust 1.97.1, locked
dependencies, default wgpu, `CARGO_BUILD_JOBS=4` and this worktree's writable
`CARGO_TARGET_DIR="$PWD/target"`. Native graphics and output-close commands
explicitly ran outside the Codex sandbox. Windows Clippy is cross-compilation,
not Windows execution. No full-workspace or alternate-renderer suite ran.

Environment `CODEX_HOME` matched `/Users/daan.hoedjes/.codex-personal` and
`CODEX_HOME="$HOME/.codex-personal" codex login status` returned
`Logged in using ChatGPT`. Native `gh auth status` confirmed personal account
`shakodev` after the sandbox could not access its keyring. No account/config
changed; Git used the existing branch identity through per-command `-c` flags.
Current AGENTS.md, relevant PLAN.md routing/geometry rules and pinned Iced
0.14.0 / iced_widget 0.14.2 / iced_term 0.8.0 APIs were inspected first.

Receipts below are in
[`main-integration`](../baselines/issue-25/main-integration/).

| Command | Result / receipt |
| --- | --- |
| `cargo fmt -p pike -- --check` | Passed; `format.log`. |
| `rustfmt --edition 2024 --check docs/baselines/issue-25/main-integration/pointer-integration.rs` | Passed; `probe-format.log` and final direct check. |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example pane_resizing -- -D warnings` | Passed; `clippy.log`. |
| Same Clippy command with `--target x86_64-pc-windows-msvc` | Passed; `windows-cross-clippy.log`. |
| `cargo test --offline --locked -p pike --lib app::tests::resize::modal_and_frozen_layout_jointly_gate_resize_but_keep_background_delivery -- --exact --test-threads=1` | **1 passed**; `joint-admission.log`. |
| `cargo test --offline --locked -p pike --lib -- app::close::tests::keyboard_selection_is_explicit_escape_cancels_and_repeats_cannot_confirm app::close::tests::real_keyboard_subscription_carries_the_decision_token_and_renews_identity app::tests::closing::modal_accepts_launch_and_natural_completion_without_closing_or_focusing_a_target app::tests::closing::close_during_output_is_nonblocking_idempotent_and_waits_for_native_receipt --nocapture --test-threads=1` | **4 passed**, sandbox; `close-routing.log`. |
| `cargo test --offline --locked -p pike --lib app::tests::closing:: -- --ignored --nocapture --test-threads=1` | **2 passed natively**, actual modal buttons and held IME clones; `native-modal-ime.log`. |
| `cargo test --offline --locked -p pike --lib app::tests::closing::close_during_output_is_nonblocking_idempotent_and_waits_for_native_receipt -- --exact --nocapture --test-threads=1` | **1 passed natively**; `native-output-close.log`. Single debug synthetic sample: dispatch **271 µs**, native cleanup **1 ms**, not a release percentile/budget. |
| `python3 docs/baselines/issue-25/main-integration/prepare-probe.py`, then `cargo build --offline --locked --manifest-path target/issue-25-integration-probe/Cargo.toml --bin pointer-integration` | Passed; `probe-build-final.log`. |
| `cargo clippy --offline --locked --manifest-path target/issue-25-integration-probe/Cargo.toml --bin pointer-integration -- -D warnings` | Passed; `probe-clippy.log`. |
| `target/debug/pointer-integration` and `target/debug/pointer-integration --mouse-report` | **Both passed natively**, `cleanup sessions=0`; `native-pointer-normal.log`, `native-pointer-mouse.log`. |
| `git diff origin/main --check -- src examples docs/verification` | Passed; raw test receipts preserve original blank final lines. |

The joint regression checks modal queued divider/key/focus controls, direct
font/geometry admission, rejected PTY input, background footer notices, Cancel
recovery, frozen backend size and confirmed-close membership/native cleanup.
The native probe uses a persistent real App widget Tree, actual Task effects
and native PTYs, driven by synthetic events. Both modes assert zero divider
terminal selection/mouse commands or admitted PTY bytes, actual fresh-drag
movement, raw/App blur cancellation, owner transfer/restoration, ordinary
click/drag pass-through, stable sessions/widgets and tap/plain-key/resize-key
focus. It also asserts queued blur rejection and Opened/Preedit/resize-key
delivery through the same borrowed view, including queued resize-focus rejection
during live composition. Modal keys use the real keyboard subscription: Enter
defaults Cancel, Tab/Escape cancels, Tab/Enter closes the exact selected owner.
Modal pointer/resize events preserve ratios, and capped membership remains
current after both Cancel and Confirm.

Development fixture failures remain visible, rather than being labeled
application red/green evidence. `development-hidden-metrics.log` queried geometry
while R28 intentionally hides snapshots; the final assertion runs after Cancel.
`development-unprimed-ime.log` omitted IME disable/enable redraws after deliberate
blur; the fixture now sends those redraws before its same-view sequence.
An earlier counter assertion compared pre-modal cache invalidations across
legitimate modal focus changes; the final assertion snapshots after modal entry.
No production input/focus oracle, cleanup deadline or original receipt was relaxed.

## Reused evidence and limits

Source identity checks found zero differences from `a112f9a` for
`src/app/resize/capture.rs`, `src/layout.rs`, `src/layout/`, `src/workspace.rs`
and `examples/pane_resizing/widget.rs`. Prior 5 geometry / 16 workspace passes,
independent geometry audit, 14 native cases / 56 native size comparisons,
75 native resizes and zero extra calls for 1,120 repeated proposals are reused
for unchanged algorithms, not relabeled as fresh integration measurements.
The original resize regression passed during initial targeted development;
only the new joint test was repeated after its relevant changes.

`Cargo.toml`, `Cargo.lock`, all vendor sources, `src/session.rs`, `src/launch.rs`,
`src/terminal.rs` and `src/app/close*` are identical to actual merged main.
R28's Unix master-close/reap correction, held-owner shutdown semantics and native
vendor receipts are retained. No new vendor edit requires another vendor suite.
Inherited iced_winit `try_next` deprecations and the `block 0.1.6`
future-compatibility notice remain in logs.

Native Windows/ConPTY, physical keyboard/monitor, mixed-DPI, Linux and release
idle RAM/CPU/responsiveness measurements remain unperformed. Prior fixture
samples keep their original method/limits. Actions remains OFF, physical
#13/#14 holds and the #14 scheduling waiver are unchanged, and Reviewer1 owns
independent re-review. This author does not merge PR76.
