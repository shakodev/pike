# R32 / issue #32 — effective shortcuts and command reference

Status: complete feature and local verification in PR #82; exclusive R1
independent advisory re-review is green on corrected source 20cc70c. GitHub Actions remains disabled.

## Source and ownership

- Created `issue-32-remappable-shortcuts` from fetched actual merged main
  `a5fe9fc4d8f0df53db240155ecebe0088d9898f4` (PR #81 / R31).
- Deliberate final integration fetch again found actual main at that same
  SHA and reported Already up to date before final verification.
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
| `cargo fmt --all -- --check` | Passed after the final affected fixture adaptations. |
| `cargo test --locked -p pike --lib` | Pre-review checkpoint: 176 passed, 10 existing ignored; shared command/focus/settings integration justified the package library run. |
| `cargo test --locked -p pike --lib app::` | Final R1 fix: 96 passed, 9 existing ignored; command/menu/focus/capture and App-state integration affected by the correction. |
| `cargo test --locked -p pike --lib app::commands` | Final R1 fix: 19 passed, including native/App focus lease revocation, queued-map generation and reload-during-help regression. |
| `cargo test --locked -p iced_term --lib bindings` | 9 passed, including opt-in/default restoration/native fallback/custom override regressions. |
| `cargo test --locked -p iced_term --lib window_focus` | 1 passed, including redundant focus, native blur/focus and foreign-window receipt rejection. |
| `cargo clippy --locked -p pike --lib --bin pike --example remappable_shortcuts --example command_routing --example settings_reload --example pane_resizing -- -D warnings` | Passed for the affected library/bin and four fixture targets. |
| `cargo clippy --locked -p iced_term --lib -- -D warnings` | Passed explicitly for the patched dependency. |
| `cargo build --locked -p pike --example command_routing --example settings_reload --example pane_resizing` | Passed for the existing fixtures affected by additive command receipts. |
| `target/debug/examples/remappable_shortcuts docs/baselines/issue-32/native-development-3` | Passed before R1 help-focus correction; actual App/full Iced tree/tasks/subscriptions/local graphics/real PTY; cleanup acknowledged. |
| `target/debug/examples/remappable_shortcuts docs/baselines/issue-32/native-help-focus-fixed` | Final corrected source passed the complete suite plus four held-real-Enter native/App blur/refocus cases; fresh Execute/navigation/Escape/focus and exact bytes retained. |
| `target/debug/examples/command_routing docs/baselines/issue-32/native-command-defaults` | Passed with exact-one Controls for newly hosted clipboard/history and unchanged byte/viewport/selection/IME/owner oracles. |
| `target/debug/examples/settings_reload docs/baselines/issue-32/native-settings-compatible-2` | Passed with current receipt and toolbar-label expectations; all settings/session/resize/dialog/owner oracles retained. |

The finite byte fixture uses only synthetic fixture output, clipboard strings,
configuration and child byte files. It proves one default structural dispatch
with repeats consumed; menu Copy/Paste/Reload and focus; overrides/unbinds with
exact native close/copy/paste/resize/history fallback bytes; rejection of an
ambiguous surviving default and last-valid custom paste; 23 effective help
alternatives; measured End/PageUp/Home navigation, disabled command and Escape;
shifted symbol/non-US/AltGr/native IME priority; delayed paste modal/tab/native
blur round trips; custom structural mapping after successful/failed reloads; and actual asynchronous
reload while help is open, rejecting a held old row, refreshing 24 live bindings
and preserving Escape/focus. Final corrected acceptance also holds a real Enter
ClosePane receipt across native-only and App-delivered blur/refocus, proving
rejection before/after queued App focus delivery and fresh-menu usability.

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
  restored; no unrelated source/lifecycle changes remain. An exact `git archive`
  of merged main a5fe9fc independently reproduced the same class of failure with
  `cargo check --locked -p pike --example startup_lifecycle`: exit 101, 49 errors.
  The exact command/base identity and original output are retained in
  `exact-main-reproduction.txt` and `pike-r32-exact-main-startup.log`.
  Current AGENTS authorizes proportional checks on the changed package/targets;
  final lint/build verification targets the app and changed native fixture.
- The adapted settings fixture first looked for the deliberately removed
  hardcoded label `Reload settings (Ctrl+Shift+F5)`. That run and cleanup are
  retained under `native-settings-compatible`; changing only its expected fixed
  label to `Reload settings` allowed the next native run to pass. Neighboring
  fixtures accept the additive MappedCommand receipt alongside legacy types and
  require exactly one common-resolver action for copy/paste/history; their byte,
  mode, geometry, owner, and focus assertions were kept.
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

## Source and acceptance identity

The complete compiled review checkpoint is `f1273b08e8f6801795fd177b3ad37ff6255e3c84`
on PR #82. The current native target delta from it (effective-reference traversal
and the async-open-help regression) is retained as
`native-development-3/source.patch`, with the tested executable SHA-256 beside it.
`source-manifest.sha256` identifies the final affected Rust files and pinned
manifest/lock independently of report-only commits. Later neighbor fixture
adaptations and comment updates do not change the tested remap target behavior.

- [x] All 64 catalog actions accept an explicit binding through the same typed
  resolver, including prior stable host controls; all appear in the current
  keyboard/mouse reference, with context availability.
- [x] Defaults, replacement/alternatives, unbind/reuse, surviving-default conflicts,
  unsupported forms, atomic last-valid retention and remapping after reload.
- [x] Exact synthetic child bytes, one action/no duplicate writes, current help,
  disabled commands, measured long-list navigation, Escape and terminal focus.
- [x] Logical shifted/non-US symbols, AltGr/native IME priority, delayed clipboard
  modal/tab/native-blur receipts and live-help reload generation refresh.
- [x] Local affected-target formatting/lint/tests, explicit patched tests and
  durable red/green source-bound evidence; no GitHub Actions.
- [ ] Physical Windows/ConPTY/keyboard/IME/monitor validation: remains unperformed
  under the preserved physical13/14 holds and owner14 dependency waiver.

## R1 review cycle 1 correction

R1 independently found a P2 at the first compiled checkpoint: a queued real
HelpAction emitted by Enter could execute ClosePane after blur, and refocusing
the same window revived that old action. The original reviewer sources,
`native-held-help-f1273b0` and `native-help-roundtrip-f1273b0` failures/bytes/cleanup,
and prepared review bodies remain unchanged in its `target-reviewer/issue-32-review`
artifact family. The coordinator publishes independent review bodies centrally;
this author has posted no independent review or approval.

The correction stores the existing immutable WindowInputLease in help state and
checks it with the help token/settings generation before consuming any HelpAction.
The existing Capture widget has an additive `capture_with_window` constructor;
ordinary capture callers keep the same signature/default behavior. Help's native
Focused/Unfocused events synchronize the same WindowFocus immediately, including
when its full-screen modal has no Terminal child. App focus delivery refreshes
the live help token/lease only when necessary; redundant focused notifications
retain current actions. Dismissal/window close clears the receipt. No second
focus owner, mapper, PTY, input replay or unrelated gesture/lifecycle logic was
added.

The native regression withholds the actual Enter-emitted ClosePane action, then
covers native-only blur, native-only blur/refocus before App subscription delivery,
and the corresponding explicitly App-delivered transitions. All four reject the
held action without confirmation/close/bytes; each then executes a fresh current
menu action, cancels its conservative confirmation, and verifies navigation,
Escape, original session and terminal focus. Final corrected source/executable
identity is retained under `native-help-focus-fixed`; all source hashes are in
`source-manifest.sha256`. Explicit vendor binding/window tests remain valid
because the review correction changes only App capture/menu receipt handling.

## Final independent advisory re-review

R1 independently rechecked exact corrected source
`20cc70cb14033b353b5de07f0f9c78f170952f55` and reported no actionable source issue.
Its unchanged original held-help blur and blur/refocus probes, additional raw
native-before-App probe, fresh Home/Arrow/Enter/End/Escape recovery, live reload
while help is open, typed menu consumers and full corrected remap fixture passed,
as did 19 command tests, scoped Clippy and formatting. Original negative evidence
also includes `native-help-before-app-f1273b0`; all reviewer reds and prepared
review bodies remain untouched in its artifact family.

The final publication child changes this report only; its implementation is
identical to the reviewed source and source manifest. The coordinator publishes
R1's prepared independent body centrally under standing owner authorization.
This same-account advisory review is not formal separate-account approval.
Actions stays disabled, and author publication does not merge the PR or waive
physical13/14 holds or the existing owner14 dependency waiver.
