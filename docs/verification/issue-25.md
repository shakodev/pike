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
R26 should extend the mapper at this single boundary; its return type is
`app::resize::capture::KeyAction` (visible to sibling app modules).

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

## Prior-stage local results (before review corrections)

The actual-R19 release run is preserved under
[`native-macos-r19`](../baselines/issue-25/native-macos-r19/summary.json), with
[source/binary hashes and host provenance](../baselines/issue-25/native-macos-r19/provenance.json),
[raw native reports/geometry](../baselines/issue-25/native-macos-r19/geometry.csv),
[actual dispatch samples](../baselines/issue-25/native-macos-r19/dispatch.csv),
[redraw request/delivery samples](../baselines/issue-25/native-macos-r19/frames.csv),
[resize count transitions](../baselines/issue-25/native-macos-r19/resize.csv),
[scenario/focus receipts](../baselines/issue-25/native-macos-r19/events.txt), and
[the constrained-window screenshot](../baselines/issue-25/native-macos-r19/constrained.png).
The finite postprocessor [`summarize.py`](../baselines/issue-25/summarize.py)
retains originals and computes median and nearest-rank-style 95th percentile
using sorted sample index `floor(n * .95)`, capped at `n - 1`.

| Local observation | Result |
|---|---|
| Nested drag/key/font/scale/tiny/minimize/restore/maximize cases | 14 passed; 56 exact independent native dimensions |
| Real live widget focus after synthetic divider drags | Both drag cases passed |
| One parent view across Opened → preedit → empty preedit → commit → Closed | Composition bypassed resize mapping until Closed; no shell write |
| Four independent sessions/processes and widgets | Preserved through every case; all four PIDs absent after cleanup |
| Native resize counts | 20 / 19 / 18 / 18, total 75, including startup, font and window transitions |
| 1,120 repeated identical valid proposals | Zero additional native resize calls |
| 2,998 App update dispatch samples | Median 8 µs; p95 54 µs; max 7,417 µs |
| 761 redraw-event delivery samples | Median 270 µs; p95 1,392 µs; max 59,289 µs |
| 760 redraw-request intervals | Median 9,584 µs; p95 42,649 µs; max 1,707,669 µs |

Dispatch time covers `App::update`, excluding trace writes. Redraw-event
delivery measures request timestamp to observer delivery; intervals include
output cadence, deliberate waits and minimize. These are measured fixture
events, not physical input-to-display latency or a responsiveness budget pass.
The observer emits no app messages or redraw requests.

[`/usr/bin/time -l`](../baselines/issue-25/final-local/fixture-time.log) reported
16.85 seconds wall time, 2.77 seconds user time, 4.92 seconds system time,
161,693,696 bytes maximum RSS and 338,707,392 bytes peak memory footprint for
the instrumented release command. The fixture uses a separate headless public
widget driver alongside its GUI, with the same selected wgpu backend; these
measurements do **not** establish ordinary application idle memory/CPU or a
Windows resource budget. No second renderer configuration was built or tested.

Relevant final commands, all local with `CARGO_BUILD_JOBS=4`:

```sh
cargo fmt -p pike -- --check
cargo clippy --locked -p pike --lib --bin pike --tests --example pane_resizing -- -D warnings
cargo clippy --locked -p pike --lib --bin pike --tests --example pane_resizing --target x86_64-pc-windows-msvc -- -D warnings
cargo test --locked -p pike --lib layout::resize
cargo test --locked -p pike --lib workspace::tests
cargo test --locked -p pike --lib app:: -- --test-threads=1
cargo build --release --locked -p pike --example pane_resizing
/usr/bin/time -l target/release/examples/pane_resizing docs/baselines/issue-25/native-macos-r19
python3 docs/baselines/issue-25/summarize.py docs/baselines/issue-25/native-macos-r19
git diff --check
```

Formatting, both selected-target Clippy invocations and release build passed.
Geometry: **5 passed**; workspace: **16 passed**; app: **28 passed, 4 ignored**.
The ignored pre-existing device fixtures were not counted as passes; the new
finite native fixture explicitly exercises resize input/composition/focus.
The new sibling-visible capture interface was also checked with library Clippy
after its visibility-only change. Existing patched `iced_winit` reports two
deprecation warnings, and `block 0.1.6` has a future-compatibility notice; neither
was introduced by this change. Logs are under
[`final-local`](../baselines/issue-25/final-local/clippy.log).

R25 changes layout/workspace geometry APIs, `app/resize.rs`, its single capture
wrapper, minimal view/update wiring, tests, the finite example and evidence/docs.
It changes no vendor code, dependencies, launch pool, terminal routing or CI.
The PR remains based on actual merged R19+R24; R28's separately agreed early
modal view and common queued-control gate consume the suspended contract during
their actual integration, without an unmerged API import here.

Synthetic macOS and cross-Windows compilation do not establish native Windows
runtime behavior, physical keyboard operation, or mixed-monitor behavior.
Physical #13/#14 holds and the #14 scheduling waiver remain unchanged. No
unsupported resource, latency or Windows acceptance claim is made.

## Review corrections and sensitive regressions

Source review found three gaps in the prior-stage implementation: a no-motion
divider tap unfocused the terminal without publishing a Resize event, a queued
resize could apply after blur, and a capped layout retained removed pane
membership. Original independent red source/logs at `22d74e8` are preserved
unchanged in [`reviewer-red`](../baselines/issue-25/reviewer-red/initial-fallback-probe-native.log).

The resize wrapper now preserves the original widget owner immediately after
divider press/release, including release outside the grid, using the pinned
divider hit geometry. Resize keyboard controls carry their originating stable
terminal ID, recheck current owner/window/composition at consumption, and return
the actual widget focus task. Raw blur is observed in the same retained view
before mapping another key. Capped fallback reuses only bounded dimensions and
always rebuilds current pane/session membership; native Resize is frozen until
valid geometry returns. Canonical ratios remain unchanged by these focus and
fallback protections.

The finite fixture now primes its driver Tree once and never refocuses between
event groups. Tap/outside-release/drag paths assert that persistent widget focus
survives, and a controlled child must acknowledge a query encoded by that widget
after a tap. Same-view blur then resize key, live composition, queued resize after
blur and changed-owner rejection are covered. The standalone
[`focus-probe`](../baselines/issue-25/focus-probe/src/main.rs) reuses the exact
reviewer red source, executes real App task/focus effects against one persistent
Tree and checks actual rendered membership after a capped close; its separate
fixture manifest adds no production dependency. Corrected final receipts follow.
