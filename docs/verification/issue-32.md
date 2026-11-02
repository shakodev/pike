# R32 / issue #32 — effective shortcuts and command reference

Status: complete feature checkpoint for exclusive R1 review; final publication
and review fixes remain in this same task. GitHub Actions remains disabled.

## Source and ownership

- Created `issue-32-remappable-shortcuts` from fetched actual merged main
  `a5fe9fc4d8f0df53db240155ecebe0088d9898f4` (PR #81 / R31).
- Preserved the previous `issue-31-settings` branch, reports, refs and reused
  worktree-local `target`. Scoped personal Codex login passed.
- No dependency/version/lock changes, second PTY, background settings watcher,
  pane/grid/gesture/lifecycle rewrite or UI token/style imports. The existing
  typed resolver/common consumer and focus Authority remain the routing owners.
- Coordinator approved the exact settings/capture/root-menu and focused vendor
  seams before implementation. C34 retains `command_refused(Error)` integration;
  B33 retains chrome/style ownership. This task does not import their unmerged
  changes. The early native Orca sandbox connection returned
  `runtime_access_denied`; the same exact binary/IDs succeeded with escalation.

## Behavior

A bounded effective catalog drives validation, logical-key resolution and the
command reference. Configured commands replace their complete default binding
set, distinct alternatives are supported and `unbind=true` removes direct
bindings while retaining the menu action. Effective duplicates, unsafe modifier
families, malformed unbinds and unsupported keys produce indexed, redacted
settings errors. Failed reloads retain the complete last-valid configuration.

Every PLAN MVP action has a discoverable keyboard/mouse menu entry and a typed
explicit-binding name, including resize, retry/default shell, pane movement,
tab reorder/reset name, clipboard/history, close window and configured profile
entries. Context-disabled rows remain visible. The menu shows current bindings,
uses measured row reveal for Arrow/Tab/Home/End/Page navigation and restores
terminal focus through the existing Authority on dismissal or action.

Terminal host shortcut mode defaults off for existing iced_term consumers. Pike
opts in once at attachment. It releases legacy clipboard/history shortcuts to the
host and reuses the existing native encoder fallback actions after unbinding.
Custom terminal AddBindings survive toggles. Ctrl+C, printable AltGr and native
IME composition retain their established priority; actual shifted symbols and
non-US logical letters are mapped without physical positions.

Clipboard read receipts retain the requesting Reservation, the existing immutable
Authority lease and a native WindowFocus transition lease, and are consumed once.
Replies cannot follow a modal/tab/blur roundtrip back to the same session. The
single native clipboard read and existing paste admission path remain in use.
Mapped actions also carry settings generation; help reload refreshes its token
and generation while rejecting held old-view actions and keeping current help
navigation/commands usable.

## Local evidence at checkpoint

Commands use locked dependencies, `CARGO_BUILD_JOBS=4`, this worktree's target and
the existing default wgpu renderer. No remote CI was enabled, dispatched or run.

| Local command / fixture | Result |
| --- | --- |
| `cargo check --locked -p pike --lib` | Passed. |
| `cargo test --locked -p pike --lib app::commands` | 18 passed, including queued-map generation and reload-during-help regression. |
| `cargo test --locked -p iced_term --lib bindings` | 9 passed, including opt-in/default restoration/native fallback/custom override regressions. |
| `cargo test --locked -p iced_term --lib window_focus` | 1 passed, including redundant focus, native blur/focus and foreign-window receipt rejection. |
| `cargo clippy --locked -p pike --lib --bin pike --example remappable_shortcuts -- -D warnings` | Passed after the final generation receipt addition. |
| `target/debug/examples/remappable_shortcuts docs/baselines/issue-32/native-development-2` | Passed; actual App, full Iced tree/tasks/subscriptions, local graphics and real PTY child; cleanup acknowledged. |

The finite byte fixture uses only synthetic fixture output, clipboard strings,
configuration and child byte files. It proves one default structural dispatch
with repeats consumed; menu Copy/Paste/Reload and focus; overrides/unbinds with
exact native close/copy/paste/resize/history fallback bytes; rejection of an
ambiguous surviving default and last-valid custom paste; 23 effective help
alternatives; measured End/PageUp/Home navigation, disabled command and Escape;
shifted symbol/non-US/AltGr/native IME priority; delayed paste modal/tab/native
blur round trips; and custom structural mapping after successful/failed reloads.

## Preserved red evidence and limits

- First library integration run: 163 passed, 4 failed, 10 ignored. The four old
  assertions expected clipboard/resize to bypass the common resolver or expected
  generic `Invalid(Shortcuts)` errors. New common ownership/indexed errors change
  those expectations deliberately; independent new mapping/byte tests cover the
  behavior. Original log remains in `docs/baselines/issue-32`.
- First native run reached the custom IME stage without driving the second
  redraw already required by the unchanged R26 fixture's native IME handoff.
  It failed before the intended composition ownership existed. The corrected
  fixture drives both redraws and explicitly asserts `is_composing` before
  exercising the mapping. Original bytes/failure/cleanup remain under
  `native-development-1`; green bytes/cleanup are under `native-development-2`.
- `cargo check --locked -p pike --all-targets` exposed the unchanged legacy
  `startup_lifecycle` example's nested whole-library source inclusion: crate-root
  imports and restricted `pub(in crate::app)` ancestors cannot resolve there.
  Alias-only and isolated source-module experiments were minimized and then
  restored; no unrelated source/lifecycle changes remain. Red logs are retained.
  Current AGENTS authorizes proportional checks on the changed package/targets;
  final lint/build verification targets the app and changed native fixture.
- Two pre-existing `iced_winit` deprecated `try_next` dependency warnings and the
  pre-existing `block` future-compatibility notice are reported by Cargo. New
  app lint findings (operator precedence, collapsible if, closure type alias)
  were fixed locally; initial lint red is retained.
- This machine is macOS. Real PTYs/native Iced plus synthetic Windows modifier,
  layout, AltGr and IME events are not physical Windows/ConPTY, keyboard, IME or
  monitor validation. No Windows cross-compile or physical Windows claim is made
  at this checkpoint. Prior R26/R30/R31 evidence is reused unchanged for their
  unchanged behavior; new regressions establish remapping-specific behavior.
- No resource/latency improvement claim is made, so no synthetic performance
  estimate is substituted for a measurement. Physical13/14 holds and the
  preserved owner14 dependency waiver remain in effect.
