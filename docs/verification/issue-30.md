# R30 — tab management

Implementation source: **`fff1e0bcfd176b53a9d242dea0b916967bf0c461`**.
Product source is byte-identical to `7625fa6cedcf0cd6f0f3d583206d2314b3400138`;
the complete passing native fixture is `401c8ecadc3753847e7248bc400cc2d20c5d5b64`.
The final child extracts the identical 1200×800 fixture viewport into one shared
constant; no product behavior or geometry value changed. Exact source, binary,
artifact hashes and commands are in [provenance](../baselines/issue-30/provenance.json)
and the [command manifest](../baselines/issue-30/commands.json).

## Behavior

- Rename through the tab strip or **Ctrl+Shift+R**. Names are plain text, capped
  at 256 Unicode scalars / 1,024 retained UTF-8 bytes; controls, line separators
  and bidi overrides are stripped. Editable spaces survive typing; save trims
  edges. Blank save or **Use automatic name** resets the manual override.
- Names resolve as manual override > sanitized OSC title of the remembered
  focused session > sanitized launch-program basename > `Terminal`. OSC changes
  and ResetTitle never clear a manual name. Per-session automatic titles and
  per-tab overrides follow stable identities through split/focus/exit/restart.
- Move left/right reorders stable TabIds relative to stable anchors without
  touching SessionId, reservation, PID, terminal, focused pane, layout or zoom.
  Existing direct/next/previous shortcuts and the strip share one current-order
  selection helper and the merged R29 activation path. **Ctrl+Shift+L** or
  **All tabs** opens a finite scrollable list; arrows/Home/End/Tab/Enter navigate
  it and measured row bounds keep the selected entry visible. Bare F2 continues
  to the terminal.
- Each strip/list close targets its captured TabId and reuses R28's exact
  session-set confirmation and asynchronous cleanup. Cancel preserves every
  owner. Confirmed background cleanup leaves the active terminal usable;
  active removal chooses the current right neighbor, then the left at the end.
- Dialogs replace the workspace and participate in the existing modal/focus
  boundary. Unique input IDs/tokens reject obsolete events and focus tasks.
  Native and App-only blur retire IME ownership while preserving the draft.
  The additive text mode of the existing capture wrapper suppresses submit,
  cancel and app shortcuts during preedit, rejects unopened/blurred/stale commits,
  and permits a fresh valid editor retry. Existing pane/help capture and the
  SessionId focus Authority remain unchanged.

## Integration and changed files

The branch starts at merged R29 main
`013690ef60db99438c51773e1e220e77b6382ce7`. Original R25 branch, reports, commits
and target cache remain at `dbd329eccf6995b8f207a17c643aa2e0f1741147`, also held
by `checkpoint/c-r25-before-r30`. Pure checkpoints `ca4b9aa`, `867f471` and
`e6bd4cd`, clipping red/green and helper preparation are retained. Actual merged
R26 `7d715b792c7120b363abc9500ffbfb0837f0a414` was integrated **once**, in
`d389d7093efbc97612b547e623cca97687d37f7b`, before host wiring. No unmerged R26
or R27 source was imported; no reset, deletion, rebase or force-push occurred.

- `tabs.rs`, `layout.rs`, `workspace.rs`, `tab_strip.rs` and `tab_strip/reveal.rs`:
  bounded metadata, stable ordering/navigation, clipped labels and measured
  overflow visibility, with focused model/component tests.
- `app/tabs.rs`, new `app/tab_dialog.rs`, `app/commands*`, small root/close/view
  glue and the additive `app/resize/capture.rs` mode: complete controls, errors,
  ownership and shortcuts. The existing PaneGrid/content/header block stayed
  unchanged under the agreed R27 extraction boundary; no focus Authority change.
- `terminal.rs`: validated origin/reservation-bound OSC title/reset storage.
- `examples/tab_management*` and support: finite actual-App/runtime/PTY driver,
  canonical viewport and controlled child identity/state/cleanup oracles. The
  existing command fixture host exposes only its widget operation hook and uses
  the same viewport constant. `PLAN.md` records the shortcut defaults.

No dependency, lockfile, vendor or workflow diff against actual merged R26.
Personal `CODEX_HOME` was verified and the explicitly scoped login check reported
`Logged in using ChatGPT`; no global/account settings were changed.

## Local verification

All checks ran **locally** on macOS 26.7.1 arm64, Rust 1.97.1, default wgpu,
`CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=target`, locked/offline dependencies and this
worktree's own reused target. Native graphics/PTY checks ran outside the sandbox.

| Exact command / affected scope | Result |
| --- | --- |
| `cargo test --offline --locked -p pike --lib tabs::tests` | Initial 7 passed; existing activity evidence retained. |
| `cargo test --offline --locked -p pike --lib workspace::tab_tests` | 4 passed: twelve identities, reorder, navigation, stale owners and deterministic removal. |
| `cargo test --offline --locked -p pike --lib tab_strip::tests` plus exact `tab_strip::reveal::tests::overflow_reveal_uses_measured_rows_and_current_scroll_and_rejects_stale_modal_ids` filter | 1 + 1 passed; bounded range and measured/stale scroll behavior. |
| `cargo test --offline --locked -p pike --lib title_` | 4 affected title/draft tests passed after draft handling was added. |
| New `app::tests::tab_management` filters, including exact rename/stale-intent and `app_blur_without_widget_events_retires_editor_receipts_and_preserves_the_draft` | 3 host cases passed; no PTY/process creation for these pure cases. |
| `cargo test --offline --locked -p pike --lib app::commands::` | 9 passed, including R/L one-action/repeat rules and bare-F2 pass-through. |
| `cargo test --offline --locked -p pike --lib app::tests::tab_management::tab_dialogs_cancel_held_terminal_ime_clones_before_and_after_dismissal -- --ignored --exact --test-threads=1` | 1 native graphics/PTY case passed for both dialogs; held clones cannot write before/after dismissal. |
| `cargo build --offline --locked -p pike --example tab_management` | Passed. |
| `target/debug/examples/tab_management docs/baselines/issue-30/native-development-4` | **Passed**: complete actual-App/runtime synthetic keyboard/pointer/IME with twelve tabs and thirteen native children. |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example tab_management -- -D warnings` | Passed. |
| Same Clippy command with `--target x86_64-pc-windows-msvc` | Passed, cross-compilation only. |
| Fixture-only native Clippy (`--example tab_management --example command_routing`) and Windows cross-Clippy (`--example tab_management`) | Passed after the identical viewport constant extraction; production checks were reused. |
| `cargo fmt --package pike -- --check`; `git diff --check` | Passed. |

The [native events](../baselines/issue-30/native-development-4/events.txt) and
[cleanup receipts](../baselines/issue-30/native-development-4/cleanup.txt) record
thirteen distinct PIDs, unchanged registry/widget mapping, retained modes/history
and restored split ratios/zoom/focus, all twelve direct/cyclic/overflow targets,
manual/OSC/reset precedence, harmless multi-session Cancel, exact targeted
background cleanup with active-only `x`, and deterministic active-tab fallback.
Exact PTY bytes are only bare F2 (`1b4f51`) for its controlled child and `x` for
the active survivor; no application command or editor text reaches a PTY.
Ten children exited naturally with code 0 and `output_drained=true`. Three
intentional closes have acknowledged `Requested` cleanup and
`output_drained=false`; those prove disposal, not natural-exit draining.

## Preserved failures and independent evidence

Reviewer2's original long-title rendering probe changed **4,580 pixels** outside
the tab on `ca4b9aa`; the same oracle on `e6bd4cd` changed **0 pixels**. Original
probe/red, green renders and hashes remain under
[clipping](../baselines/issue-30/clipping). The historical runner is preserved
byte-for-byte; the current runner only adds the shared viewport to its helper
compile harness. Clipping/model/reveal checks were not rerun after unchanged
code or documentation-only children.

The three failed native development trees/logs are retained. Early fixture setup
sampled before split attachment/geometry settling and required hidden
`renderable_content`, which deliberately returns None. The corrected oracle
checks hidden identity directly and compares every mode during actual visits,
restoring the split's zoom/focus/ratios afterward. The third failure isolated the
1100×720 pointer viewport versus the real 1200×800 host; hit-testing/clipping was
preserved and all users now consume one canonical viewport. Each failed run
completed through the App shutdown path; explicit empty-model cleanup receipts
are retained for later runs. No deadline, input or native cleanup oracle was
weakened. An initial pure draft-length expectation was corrected from 256 to
254 after trimming its two leading editable spaces; the 256-scalar cap remains.
A mechanical viewport extraction's unused import/duplicate-module lint failure
is also retained beside the corrected affected-fixture Clippy green.

Reviewer2 independently reproduced the pointer failure, passed the corrected
full native fixture and stronger one-child editor/ownership probes, and retained
its own model/clipping/reveal evidence. Final independent verdict/publication
and merge are coordinator-owned; the author has not merged this PR.

## Limits

No native Windows/ConPTY, physical keyboard/monitor, Windows IME candidate UI,
Linux, release RAM/CPU or latency measurements were performed or claimed.
Windows Clippy is compilation evidence only. The #13/#14 physical holds and #14
scheduling waiver remain. Existing unchanged profile/query/cleanup, bounded
session registry and corrected R29 cache/attachment-intent evidence are reused.
No vendor tests were needed because vendor source is unchanged. No full-workspace
or both-renderer checklist, Actions enable/dispatch/rerun or replacement CI ran.
