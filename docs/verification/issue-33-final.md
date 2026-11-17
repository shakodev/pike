# R33 native chrome: final local verification

The owner resumed the full issue after the historical stop. PR:
https://github.com/shakodev/pike/pull/83 (`Closes #33`). The earlier
`issue-33.md` and `issue-33-owner-stop.md` are preserved historical checkpoints.
This report supersedes their incomplete implementation status.

## Source and boundaries

- Original actual-main base: `a5fe9fc4d8f0df53db240155ecebe0088d9898f4`.
- Deliberately fetched and merged actual R32 main
  `8543f823c35003528413efbd6b547334532d94c8` once, before root integration and
  final checks. Merge: `c212665`. Original branches, reports, reds and build cache
  remain preserved; no reset/rewrite or self-merge occurred.
- Final production correction: `333d08f4697906903cf68d738713296f4ccf05c9`.
  Scene fixture correction: `dc77f7dd1b49359b406630b9b899d075d4a56142`.
  Later evidence/report commits do not change production code.
- Shared root seam was explicitly approved by the coordinator. No App fields
  were added. The exact close/help/tab-dialog priority, R32 effective resolver,
  menu/help behavior, capture closure and generation mapping remain intact.
  Only `command_menu()`'s final widget construction changed.
- No unmerged R34 imports or stubs. R34 owns its eventual typed error/footer
  insertion after actual R33; existing host/settings/terminal error presentation
  is implemented here.

Changed production files:

- `src/ui.rs`, `src/ui/{tokens,style,label}.rs`, `src/lib.rs`: shared dark/light
  palette, DIP/type tokens, quiet icon controls, bounded plain text and cached
  measured ellipsis. Existing R31 dark/light/system resolution is retained.
- `src/tab_strip.rs`, `src/tab_strip/reveal.rs`, `src/app/tabs.rs`: compact strip,
  active-tab visibility, overflow, hover/active close reveal, separate reserved
  unread/bell/exited cues, effective command-reference captions and full titles
  in tooltips. Overflow close controls reserve scrollbar space.
- `src/app/view.rs`: normal 34-DIP tabs/plus/menu row, compact empty state and
  bounded passive settings/error/size/notice rows before the Fill pane. Menu
  retains profiles, reload, copy, reset-name and tab moves. Passive rows stay
  outside terminal clipping. No permanent settings/action toolbar remains.
- `src/app/panes/view.rs`, `src/app/split.rs`, `src/app/close/view.rs`,
  `src/app/close.rs`: 26-DIP headers, focused split controls, stable zoom/close
  child positions across mouse-down focus, 1-DIP focus outline, 4-DIP gutter
  paint, start/error/exited/closing controls, and viewport-sized presentation
  while terminal font geometry is unavailable. The original title adapter keeps
  visible text draggable. Only unused toolbar close helper exports were removed.
- `src/app/commands/reference.rs`: 26-DIP quiet menu icon using its existing
  effective tooltip and exact existing admission predicate.

Native decorations/snapping configuration, session IDs, ownership, split/move/
resize algorithms, terminal emulation, vendor sources, dependencies and lockfile
were not changed relative to integrated main. No production animation, polling,
redraw timer, blur, telemetry, resource monitor, second PTY or `unsafe` was added.

## Local checks and native scenes

All Cargo work used `CARGO_BUILD_JOBS=4`, `--locked`, and this worktree's reused
writable target. Platform: macOS arm64, default wgpu renderer. Actions stayed off.
The exact installed Iced 0.14.0 / iced_widget 0.14.2 and patched iced_term 0.8.0
sources were inspected before API changes. Personal scoped Codex login was
verified with `CODEX_HOME="$HOME/.codex-personal" codex login status`.

Evidence root: `docs/baselines/issue-33/`.

| Command / check | Result and evidence |
| --- | --- |
| `cargo fmt --all -- --check` | Passed; `accepted-fmt.log`. |
| `cargo clippy --locked --lib --example native_chrome --example native_chrome_idle --example pane_moving -- -D warnings` | Passed on final production correction; `corrected-source-clippy.log`. |
| `cargo clippy --locked --example native_chrome -- -D warnings` | Passed after fixture-only containment correction; `accepted-fixture-clippy.log`. |
| `cargo test --locked --lib` | 178 passed, 10 existing ignored; `accepted-lib-tests.log`. |
| `cargo build --locked --example native_chrome --example pane_moving` | Passed; `final-title-error-oracles-build.log`. |
| `target/debug/examples/native_chrome …/accepted-scenes` | Exit 0; **128 rasters**, `completed.txt`, cleanup acknowledged. |
| `target/debug/examples/pane_moving …/corrected-title-edge-gestures` | Exit 0; first visible title letter pick, full move/zoom, stable owners, selection/copy, mouse/modal cancellation, clipping, hidden output/query/exit and native cleanup. |
| `cargo clippy --locked --example settings_reload --example tab_management --example remappable_shortcuts -- -D warnings` | Passed after current-UI selector migration; `current-chrome-scroll-clippy.log`. |
| `target/debug/examples/settings_reload …/current-settings_reload` | Exit 0; real menu/profile/reload/plus/header path, retained owners/settings/PTY bytes and cleanup. |
| `target/debug/examples/remappable_shortcuts …/current-remappable_shortcuts` | Exit 0; current menu ID, effective map, native fallback bytes, focus/modal lease regressions and cleanup. |
| `target/debug/examples/tab_management …/current-tab_management-scroll` | Exit 0; twelve-tab actual pointer/keyboard rename, reorder, overflow, targeted close and cleanup. |
| `git diff --check` | Passed. |
| Native process audit | 0 task-owned fixture processes remain; `accepted-process-audit.txt`. |

`accepted-scenes-provenance.json` records exact source, binary SHA-256 and source
file hashes. Binary: `bb9fcaf8e3c321532085673032a5b4c06ae12d2a529b0cc040ebc89f4544900e`.
`accepted-scenes/screenshots.json` records PNG, original PPM and RGB hashes.
Lossless packaging preserves the original PPM files in the task's temporary
artifact directory. PNGs and hashes are committed.

The matrix covers one, one-small/restored, nested-four, eight, eight-small/
restored, overflow strip/list/small, passive settings error, failed launch/
small, empty/small, and exited. Each has dark/light system modes at
100/125/150/200% raster scales. The normal viewport is 1200×800 DIP and the
small viewport 320×240 DIP. All sixteen contact sheets and representative
full-resolution originals were visually inspected; accepted sheets are under
`accepted-contacts/` (dark/light rows; four scale columns).

Actual-widget assertions check the 34-DIP root/strip, 26×26-DIP plus/menu/
overflow targets, 26-DIP header controls, positive pane bodies, reachable failed
heading/recovery controls, visible failed-heading ink and controlled prompt
pixels inside actual clipped terminal viewports. Pane minimum remains 20×5
cells; smaller windows preserve sessions and use bounded notices and pannable
content. A fresh controlled child prompt is requested after resize reflows an
old wrapped prompt into history; blank/stale snapshots cannot satisfy readiness.

The existing `on_resize(8)` leeway is preserved. Pinned Iced documents the input
width as `spacing + leeway`: 4-DIP paint/spacing plus 8-DIP leeway yields a
12-DIP centered drag region. It was not narrowed to the painted gutter.

Existing dependency warnings remain: `iced_winit` deprecated `try_next` and the
`block` future-incompatibility notice. No vendor files changed in this issue,
so no patched-dependency tests were required.

## Native idle observations

Exact actual-R32 source and corrected styled production used identical
`native_chrome_idle`, child and window harness hashes, locked dependencies,
release build, one actual native 1200×800-DIP window and one real controlled idle
PTY. Settle: 3 seconds; idle interval: about 24 seconds. The raw observer returns
`None`; it records redraw/focus/unfocus/resize counts without generating App
messages. Only two finite fixture deadlines exist. CPU/RSS come from local
`ps -o time=,rss=`. No polling or monitor was added to the product.

| Observation | Actual R32 | Styled production |
| --- | --- | --- |
| Source | `8543f823` | `333d08f` |
| Idle seconds | 24.003 | 24.007 |
| Native redraws | 1 | 1 |
| Focus / unfocus / resize deltas | 0 / 0 / 0 | 0 / 0 / 0 |
| Process CPU seconds, start → end | 0.17 → 0.18 | 0.17 → 0.19 |
| RSS KiB, start → end | 98,640 → 98,768 | 99,552 → 99,680 |
| Font / PTY resize / grid resize / cache / snapshot deltas | all 0 | all 0 |

Reports: `observed-r32-before/native-idle.txt` and
`observed-styled-after/native-idle.txt`; source/binary/harness identities:
`observed-baseline-provenance.json` and `observed-corrected-provenance.json`.
This is a finite local observation, not a Windows result, significance claim,
latency measurement or full release performance gate.

The earlier `final-styled-native-after` observation recorded 3 redraws, 0.04
seconds CPU and 2 cache invalidations while another task-owned native baseline
window was started. It is retained as confounded evidence; the exact event cause
was not recorded. The later isolated observation and sequential passive-event
diagnostic above are separately retained and do not erase that result.

## Retained reds, review, and limitations

- Initial root extraction and unsupported rectangle method compile errors were
  corrected locally; original and corrected logs remain.
- ANSI green was visible in the first raster but rejected by an overly narrow
  color detector. The corrected detector still verifies actual prompt pixels.
- Old eight-pane wrapped prompts reflowed into history on shrink. The controlled
  child repaint protocol now verifies a current prompt at the actual new size.
- The original missing configured executable was rejected by settings validation
  and therefore did not create a failed launch. The fixture now uses an existing
  non-executable regular file and verifies actual launch failure. No default
  shell output was captured or logged by the rejected fixture.
- Inactive Zoom/Close initially lost pressed widget state when split buttons
  were inserted on mouse-down focus. Stable parent child positions fix both;
  the full gesture run and R2's unchanged independent reproducer pass.
- The first measurement wrapper introduced pick width and regressed the first
  title letter's drag. Original direct title wrapping is restored; R2's unchanged
  title-edge probe and the full visible-letter gesture run pass.
- Visually blank failed panes exposed a missing-font/unbounded-scroll layout
  collapse. Sensor-viewport fallback fixes it; failed heading ink and recovery
  geometry now have explicit oracles. Blank images remain in `final-error-empty`.
- Failed-control rectangle intersection equality rejected valid fractional
  23.4-DIP heading bounds. Pinned `is_within` now checks containment directly;
  the full 128-raster run and R2's 40-raster error subset pass.
- The legacy pointer fixture's obsolete 8-DIP root-padding assumption was fixed;
  the clipboard fixture now awaits its actual asynchronous write. Reds remain.
- `cargo test --locked` was attempted and failed compiling the inherited
  `startup_lifecycle` nested whole-library source loader (97 missing root alias/
  ancestor errors). Log: `accepted-root-tests.log`. The same class was already
  independently reproduced on exact pre-R33 main; see R32's
  `exact-main-reproduction.txt`. The coordinator explicitly retained R34's narrow
  fixture-boundary repair ownership. No unmerged fix was imported. This also
  prevents claiming an all-target suite pass; scoped checks above are green.

The final affected-fixture audit also found legacy picker/text-toolbar selectors
in the existing settings, tab-management and remappable-shortcut native drivers.
Those directly affected R33 selectors were migrated in `3629c02` and `a9ec359`;
the unchanged `src/` production tree and accepted visual/idle evidence were not
rerun. Shared finite `examples/support/chrome_actions.rs` measures current
visible control IDs and drives the actual effective keyboard menu. Profile B is
one current menu action, reload uses the real menu, plus/header/overflow rows use
current controls, and the stale-owner test holds a real emitted reload shortcut.
No App state or manufactured action bypass was added. The initial offscreen
last-row pointer rejection is retained; the final tab adapter scrolls the actual
list before clicking its measured visible row. All three native runs pass with
explicit cleanup. R2 retains its independently reproduced original picker/tab
selector reds. `current-chrome-fixtures-provenance.json` and
`current-tab-scroll-provenance.json` identify these fixture sources and binaries.
`live_tabs` already locates actual emitted NewTab actions rather than text and
required no selector change. These are fixture-only changes; no production,
resolver, capture, session or settings algorithm was changed.

Exclusive R2 independently reproduced and then verified inactive controls,
title-edge dragging, full move/zoom, effective remapped tooltips, help modal input
ownership, and the corrected native error subset. Its final review body is
prepared independently for unchanged central publication by the coordinator.
Same-personal-account advisory review is not formal separate-account approval.

Native Windows/ConPTY, Windows snapping, mixed-DPI monitor movement and physical
keyboard/monitor verification were unavailable. Synthetic macOS/offscreen Iced
with real Unix PTYs does not establish those passes. Physical #13/#14 holds and
the #14 owner dependency waiver remain. No Actions were enabled or dispatched.
