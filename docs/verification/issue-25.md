# R25 — bounded divider and keyboard resizing

Issue: https://github.com/shakodev/pike/issues/25.

Drag any pane-grid divider, or press **Alt+Shift+Arrow** to move the nearest
ancestor divider on that axis by one focused terminal cell, rounded to Iced's
logical-pixel split lattice. Left/up decrease the split coordinate; right/down
increase it. Resize controls preserve focus, session IDs, processes and buffers.
Directional focus, pane movement, zoom, broad shortcuts and settings UI remain
separate issues.

The application uses the pinned Iced 0.14 pane-grid implementation, including its
integer rounding and real 4 DIP gutters. Each leaf minimum is its current
measured **20 columns × 5 rows**, plus the real 28 DIP header, 1 DIP border and
48 DIP status footer when present. Nested constraints combine both children on
the split axis and take the maximum on the other axis. Invalid/nonfinite/stale
resize requests are rejected atomically. The adapter remains responsible for
deduplicating effective native dimensions; subcell layout changes update its
cached area so a later font change uses the current area.

## Constrained windows and restore

Canonical ratios live exclusively in the workspace model. A lightweight
`pane_grid::State<PaneState>` presentation clone copies only pane/session IDs
and layout nodes, never an emulator, process or subscription. When a window
cannot honor a stored ratio, only presentation ratios clamp. Enlarging/restoring
the window recovers the stored ratios, subject to the same pixel rounding.

If even the intrinsic minimum tree cannot fit the viewport, the grid uses a
scrollable canvas and shows **Size constrained · 20×5 minimum · drag scrollbars
to pan**. Headers and terminal drawing are clipped to the viewport, and
scrollbar dragging pans safely even while a terminal owns wheel events. In this
case the child reports the full terminal canvas dimensions, including cells
temporarily outside the clipped viewport, rather than the clipped fragment.
No session is destroyed or automatically closed to make space.

Intrinsic sizing sums leaf minima/gutters and is linear in the retained pane
count, including a 32-deep tree at extreme ratios. Each canvas axis has an
explicit **16,384 logical-pixel cap**; an unrepresentable constraint keeps the
last good bounded presentation and native dimensions until a valid layout is
available. Nonpositive/nonfinite minimize measurements also retain the last
good layout. This cap is a geometry safety limit, not a measured memory budget.

Font metrics and layout are logical pixels. Iced applies monitor scaling once;
the application does not multiply terminal metrics by monitor DPI.
`FontSettings.scale_factor` is relative line height, not monitor scale. The
UI-thread `change_terminal_font` host path updates measured cell constraints.

## Input boundary

`app::resize::capture(content, suspended, active_terminal, on_key)` calls its
mapper before child widgets handle a keyboard event. The mapper returns
`None` to delegate, `KeyAction::Consume` for a captured release, or
`KeyAction::Dispatch(Control)` for an action. R25 supplies `resize_key`, matching
only exact Alt+Shift+Arrow presses/releases, including repeated presses.
R26 should extend the mapper at this single boundary.

The wrapper reads `Terminal::is_composing()` dynamically at dispatch, using the
API from actual merged R19 main `5d34cb74b99f5e7e6865789f645e5fe519ba2d94`.
Composition retains priority through empty preedit and commit until Closed.
All input-method events pass through untouched. The suspended gate covers
window/application input ownership; close-policy integration must include its
modal predicate and reject queued resize controls at its common control gate.
No post-dispatch keyboard listener or vendor input rewrite is introduced.

## Local verification and evidence

Verification runs locally on macOS arm64, with the inherited default wgpu
renderer/cache and `CARGO_BUILD_JOBS=4`. Actions remains disabled; no workflow is
dispatched or changed. No other renderer or full workspace sweep is required,
and R25 changes no vendored source.

Development checks passed: five geometry regressions (nested clamp,
idempotence, rounding, tiny/restore/font/synthetic DPI, invalid/minimize, nearest
ancestor and 32-pane/cap behavior), a native-session host regression, focused
Clippy with `-D warnings`, and x86_64-pc-windows-msvc application/fixture checking.
The first Windows fixture check exposed an initialization API difference;
using its documented integer record initialization fixed that local reproducer.

The finite `pane_resizing FRESH_DIRECTORY` example runs the real App and four
distinct output-producing native children. Expected geometry is collected
independently from actual terminal focusable widget bounds and current measured
cells. Each admitted query asks the child to report its native console size
(`/bin/stty size` on Unix, console buffer API on Windows), independently of the
adapter counters. It retains per-case native reports, expected bounds/cells,
dispatch/resize traces, a constrained-window screenshot and explicit cleanup.

Development evidence is preserved under
[`dev-native-1`](../baselines/issue-25/dev-native-1/events.txt): all 14 cases and
56 exact native geometry comparisons passed. It includes real divider drag,
pre-child resize keys, font changes, synthetic 100/125/150/200% application
scaling, tiny layout, minimize/restore and maximize. Repeated equivalent
proposals added no native resize calls. Its original frame subscription was
filtered by Iced, so that development run contains **no frame timing evidence**.
The corrected fixture observes raw redraw delivery without emitting app
messages or requesting new frames, and checks actual live widget focus after
divider drags.

Final current-source receipts, observed timing/count summaries and screenshot
will be added before marking the PR ready for review. Synthetic macOS and
cross-Windows compilation do not establish native Windows runtime behavior,
physical keyboard operation, or mixed-monitor behavior. Physical #13/#14 holds
and the #14 scheduling waiver remain unchanged. No unsupported resource or
latency budget claim is made.
