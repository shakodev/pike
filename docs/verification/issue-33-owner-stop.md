# R33 owner-stop handoff

**Partial work; full issue #33 is incomplete.** The owner requested finishing the
current bounded increment and stopping on 2026-11-02. No root integration,
additional feature scope, final broad fixtures, CI, merge, or issue-closing PR
was started during wind-down.

Branch: `issue-33-native-chrome`. Coherent source/evidence checkpoint:
`bd080fa13e993da46bb7e31228ae2dcdbaebc451`. This handoff is a subsequent
documentation-only commit; its final SHA is recorded in the dispatch completion
and remote branch. The original `issue-27-pane-move-zoom` ref and original
reports/artifacts/build cache remain preserved.

## Exact source history

| Commit | Scope |
| --- | --- |
| `a5fe9fc4d8f0df53db240155ecebe0088d9898f4` | Fetched/verified actual merged R31 main; starting base. |
| `7e1ffb67f7bc776822ecbc31dd8cef28682724d1` | Initial compiled subtree checkpoint sent to exclusive reviewer R2. |
| `5fcc029` | Retained native baseline and prepared actual-App frame/scenario driver. |
| `9ff04b1` | Observed-prompt readiness regression and actual zoom-control ID lookup. |
| `855e535d86c15447826304396989277e6f16f30d` | Reserved tab activity cues outside title ellipsis, including overflow. |
| `bd080fa13e993da46bb7e31228ae2dcdbaebc451` | Final source-identified activity result, formatting and cleanup evidence. |

## Changed files and behavior

- `src/ui.rs`, `src/ui/{tokens,style,label}.rs`: shared static palette, spacing,
  type, quiet controls, bounded plain text, and width-aware cached ellipsis.
  The label cache retains only current text/width/font/display/paragraph.
- `src/tab_strip.rs`: 34-DIP responsive strip, active-tab visibility at narrow
  widths, 26×26-DIP controls, hover close reveal, reachable overflow, measured
  title ellipsis, separate unread/bell/exited cues and full tooltips.
- `src/app/panes/view.rs`, `src/app/split.rs`, `src/app/close/view.rs`,
  `src/lib.rs`: 26-DIP headers, 1-DIP focus outline, 4-DIP gutter paint,
  token aliases/exports, existing start/error/exited/close-state styling, stable
  header/control IDs. Existing 8-DIP resize leeway is preserved (pinned Iced
  adds it to spacing for input); it was not reduced to the 4-DIP paint width.
- `examples/native_chrome.rs`, `examples/native_chrome_idle.rs`, and
  `examples/native_chrome/{child,components,frame,scenes,window}.rs`: finite
  native renderer / real controlled PTY fixtures and native-window idle driver.
  `pack_screenshots.py` losslessly packages PPM frames as PNG with hashes.
- `examples/pane_moving/scenarios.rs`: zoom pointer lookup now measures the
  stable actual control ID through scroll translation and clipping instead of
  using an obsolete pixel offset.
- `docs/baselines/issue-33/` and the two verification reports: source receipts,
  local commands/results, original reds, native images and measurements.

Normal root `App::view`, `App` fields, command/help/capture behavior, settings,
pane gesture/session/resize ownership algorithms, vendor sources and dependency
lockfiles were not edited. No production polling, animation, blur, telemetry,
resource monitor, second PTY, or new dependency was added.

## Completed local verification

All Rust commands used `CARGO_BUILD_JOBS=4`, `--locked`, and this worktree's
reused target. Platform: macOS arm64, default wgpu renderer. No GitHub Actions
were enabled, dispatched or rerun.

| Check | Result / evidence |
| --- | --- |
| `cargo test --locked --lib ui::tests` | 1 passed; bounded Unicode/control filtering. |
| `cargo test --locked --lib app::tests::resize` | 2 passed; existing modal/frozen/resize/session identity checks. |
| `cargo test --locked --lib tab_strip::tests` | 1 passed; selected tab remains in range for capacities 1–5 and 1–32 tabs. |
| Scoped `cargo clippy --locked --lib --example native_chrome --example native_chrome_idle --example pane_moving -- -D warnings` | Passed after corrections; later activity scope also passed. Existing dependency deprecation/future-incompatibility notices remain. |
| `cargo build --locked --example native_chrome` | Passed. |
| Native `--readiness` | Passed: 12,900 copied blank cells rejected, actual marker observed after release, reservation unchanged, native cleanup acknowledged; R2 independently reproduced this pass. |
| Native `--activity` on exact `855e535` | Exit 0, **208 actual native rasters**: individual/combined activity, long ASCII/CJK, dark/light, 200/320/1200-DIP strip and 200/320-DIP overflow, 100/125/150/200% scales, real overflow hit targets, unchanged external redraws request `Wait`. |
| Changed-file `rustfmt --edition 2024 --check …` | Passed after module-order formatting correction. |
| `git diff --check` | Passed. |
| Native process audit | **0** task-owned fixture processes remain. Sandbox `ps` denial is retained; native read-only audit succeeded. |

Exact final activity executable SHA-256:
`643838c9d431097b46f9f0ba9de8fcac3237a95cc704deb51bc11cb4acd0a2fa`.
Receipt: `docs/baselines/issue-33/activity-855e535/provenance.json`.
Representative corrected narrow/background/overflow pixels were visually
inspected; the earlier component contact covers all four scales. These are
component acceptance, not final whole-App acceptance.

Final `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, integrated move/zoom gestures and broad actual-App scenes were not
run: the actual-R32/root integration gate had not been reached before owner
stop. No vendor code changed, so dependency test suites were not required.

## Measurements retained; no after-styling resource claim

Exact `a5fe9fc` source plus controlled harness, release/native macOS window,
1200×800 logical pixels, one real controlled PTY, 3-second settle and
24.012-second idle: **1** observed native redraw; process CPU time
**0.18 → 0.20 seconds**; RSS **99,008 → 99,056 KiB**; font/PTY resize/grid
resize/cache invalidation/snapshot deltas **all 0**. The passive raw redraw
observer returns `None`; only two finite measurement deadlines were added to
the fixture. Original source and executable receipts are retained.

A matched actual-R32 baseline and after-styling measurement were not performed.
Therefore no styling resource-regression or Windows performance claim is made.
An earlier offscreen queue-drain measurement is explicitly separate in the main
report and is not a product-native-window performance claim.

## Review findings and current state

- **E1 readiness evidence gap:** initial snapshots could be blank despite copied
  cells. Fixed with actual current controlled-marker observation; deterministic
  native regression and independent R2 rerun pass. Integrated multi-pane frame
  acceptance remains pending.
- **E2 existing pointer fixture regression:** old Max coordinate hit Split down
  after chrome changed. Source now queries actual visible control bounds/ID and
  compiles; the native move/zoom/gesture rerun remains pending integration.
- **P1 production activity visibility:** long title ellipsis hid unread/bell/exit
  suffixes. Separate reserved indicators now pass the source-identified 208-frame
  local run. Independent final P1/full-R33 verdict remains pending at handoff.
- The original paragraph infinity/debug overflow, missing release labels,
  fixture compile errors, gated-child setup errors, and subpixel assertion error
  are retained. No red evidence was discarded or represented as a pass.
- R2 evidence lives in the separate Pike reviewer worktree under
  `docs/baselines/issue-33-reviewer2/`; no reviewer files were modified.

## Pending exact integration proposal

The coordinator explicitly held normal root composition until actual R32 merges;
owner stop superseded that future gate. R32 candidate `f1273b0` was read for
planning only and was **not imported or merged**. Its committed API is
`command_reference() -> Vec<CommandReference>` with
`{command,label,bindings,enabled}`, and `command_menu() -> Element<Control>`.

Proposed but not implemented/approved during wind-down:

1. Preserve exact close/help/tab-dialog early returns and R32 capture closure /
   `MappedCommand` generation mapping.
2. Replace permanent toolbar/settings rows with a 34-DIP tabs/plus/compact menu
   row. R32's existing menu already exposes profiles, reload, copy, reset-name
   and tab moves, retaining those actions without duplicate permanent rows.
3. Change only the final widget construction of `command_menu()` to a 26-DIP
   quiet icon using its existing effective-help caption and exact existing
   `on_press_maybe` predicate; leave menu/help/resolver/capture behavior owned by A.
4. Add pure caption helper
   `command_hint(&[CommandReference], Command, label) -> String`; format owned
   captions from one reference vector for pane/tab controls. No App fields.
5. Keep bounded passive settings/errors/constraint/notices after strip and before
   the Fill pane, outside terminal clipping. C's new error/footer data/actions
   and passive row remain deferred until actual R33; no unmerged C API is stubbed.

Still required for full #33: actual-main integration, root composition/effective
tooltips, final one/nested-four/eight/overflow/error/empty and small-window
actual-App four-scale screenshots, prompt/content clipping, corrected native
move/zoom/gesture run, matched release idle before/after, relevant final checks,
R2 final review/fixes and an issue-closing PR. No such PR was opened at owner
stop; this branch is a partial checkpoint, not a completed issue.

Native Windows/ConPTY, Windows snapping, physical keyboard/monitor input and
mixed-DPI monitor movement were unavailable. Native macOS/offscreen Iced plus
Unix PTY does not establish those passes. Physical R13/R14 holds and the owner14
dependency waiver remain intact.

Native heartbeat and `ask` rejected the exact live worker context with no
effects; successful status/check messages carried coordination instead. Exact
IDs/capabilities were not replaced. Personal `CODEX_HOME` and explicitly scoped
login were verified; commit author identity was supplied per command from the
verified repository history, with no global account/configuration changes.
