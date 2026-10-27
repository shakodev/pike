# Issue #13 — widget identity and moved-pane geometry

Implementation starts at `fb9e5fc`. Read AGENTS.md and issue #13 with comments
(none). The referenced uncommitted PLAN.md was not present in the checkout or
Documents/Downloads planning folders; the issue declares its body self-contained.
Dependencies #11 and #12 are closed and merged in starting main (`fa8023d` and
`0a1bf7f`). No application workspace/session changes are needed for this patch.
The initial measurements below belong to head `0dd6ec1`; review corrections and
their separate evidence are recorded at the end of this document.

## Implementation and API review

[Upstream PR 70](https://github.com/kemokempo/iced_term/pull/70), head
`d9b343fdd5362979c61be307cdd0fa4ab301ef80`, adds a terminal ID to widget state
and resets it in `diff` on replacement. Backport that change to the patched
0.8.0 adapter. Iced otherwise reconciles state by widget type and tree position,
so replacing a terminal with another of the same dimensions can inherit focus,
a selection drag, click history, modifiers, partial wheel scrolling and the
already-seen size. Resetting the first-layout marker ensures the new terminal
receives its own resize command. The backend still deduplicates actual PTY work.

The installed Iced 0.14 sources were checked before implementation:
`iced_core::widget::Tree`, renderer layers/translations and window Rescaled;
`iced_graphics::geometry::Cache`; and `iced_widget` 0.14.2 canvas/container.
The cache keys its stored geometry by drawing bounds, not widget identity or
screen origin. The old view cached absolute screen coordinates using the entire
viewport size, so a same-size move reused glyphs at the old position.

Geometry now contains pane-local coordinates. Its cache uses pane size, while
each draw applies the current layout translation and a layer clipped to the
intersection of pane bounds and ancestor viewport. This covers glyphs, cursor,
selection backgrounds, underline strokes and batched backgrounds, including
the interval before a resize command reaches the backend. An origin-only move
can reuse geometry and produces no resize/font/model work. Size changes use
Iced's existing cache key; font/theme changes keep their R12 invalidation paths.
A DPI event clears geometry once without changing logical cell metrics. The
existing explicit-clear counter now includes this widget-triggered invalidation
through a Cell, without new synchronization or a public API.

Current bounds also invalidate pointer gestures at the old origin. Clicking or
operating focus away, losing window focus, and releasing the button outside a
pane cancel its drag. Same-session reconciliation keeps keyboard focus; changing
IDs resets it for the application to assign deliberately. Scrollback and
selection remain in the session, rather than the transient widget state.
Alacritty's existing selection invalidation on a column-count reflow is unchanged;
font-size changes can legitimately cause that reflow.

No dependencies, lockfile, renderer defaults, PTY implementation, unsafe code,
production timers, background tasks or polling were added.

## Verification

- Formatting passed: `cargo fmt --all -- --check`.
- Denied-warning all-target Clippy passed for both renderers on aarch64 macOS
  and with `--target x86_64-pc-windows-msvc`, including explicit adapter targets.
- Application/all-example tests passed for both profiles: 39 library, two
  application, five compatibility and 39 library-source probe tests. The
  existing timing tests remain ignored. Two compile-fail doctests passed.
- Explicit adapter suites passed for both renderers: 74 tests, one existing
  profiling test ignored. Four new regressions cover all transient state across
  32 equal-size replacements, initial size delivery, origin-only movement with
  stable work counters, retained session selection/history and focus cleanup.
- Release `widget_geometry` builds and runs passed for both profiles. The
  public widget event checks use a real renderer and persistent tree to verify
  32 alternating IDs, initial resize routing, deliberate click focus, canceled
  drags after moves/outside releases/window focus loss, and DPI invalidation
  without extra font measurements or PTY/grid resizes.

The opt-in example launches three copies of itself as synthetic PTY children.
They emit known colored cells, history, IDs and readiness titles, then wait for
input. No user shell/profile or terminal transcript is collected. The probe
checks requested cleanup even after preparation, readiness or pixel failures;
`completed.txt` is written only after successful checks and acknowledged cleanup.
All six fixture PIDs in the recorded runs were absent after completion.
`output_drained=false` in cleanup records is expected for explicitly terminated,
stdin-waiting fixtures; this is not a natural-exit/final-output test.

Pixel checks at 125%, 150% and 200% compare the terminal at two fractional
origins, alternate equal-size views, and reject painting outside pane or ancestor
viewport bounds. They deliberately keep the larger initial backend grid while
drawing smaller panes, testing clipping before layout messages are consumed.
Nested PaneGrid screenshots cover swap, maximize, restore and an opaque overlay;
another screenshot exercises font/theme changes. Selected snapshots were
converted from PPM to PNG and inspected in the
[evidence directory](../baselines/issue-13/).

Translated interiors match exactly for wgpu at all scales, and tiny-skia at
125%/200%. At 150%, tiny-skia's fractional transform changes some antialiased
edge values: maximum channel delta 29, mean RGBA delta 0.042373. The comparison
allows mean delta below 0.25 and at most 0.1% of pixels above channel delta 32,
so substantial glyph/cursor displacement still fails. Pane/viewport exterior
checks remain strict, with a one-pixel boundary allowance for raster rounding.

## Release resource observations

macOS 26.7.1 arm64, pinned Rust 1.97.1. Each row is one complete release probe,
including renderer initialization, three synthetic fixtures, screenshot encoding
to PPM and shutdown. `/usr/bin/time -l` ran outside the workspace sandbox to
permit Metal and resource-counter access. PNG conversion and compilation are
excluded. Raw counters, event timings, renderer names, cleanup acknowledgements
and source hashes are retained with the screenshots.

| Renderer | Wall s | User CPU s | System CPU s | Reported maximum RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| wgpu | 0.85 | 0.08 | 0.11 | 78.17 |
| tiny-skia | 1.22 | 0.75 | 0.08 | 66.25 |

These are short offscreen test-program observations, not application idle/private
memory budgets, Windows performance, input-to-render percentiles or a long soak.
The geometry-only move check observes unchanged per-session work counters; no
timer or resource sampler is installed in production.

## Reproduction and remaining native checks

On either host, run the root formatting, Clippy and test commands with locked
dependencies. Repeat app Clippy/tests with
`--no-default-features --features renderer-tiny-skia`, then explicitly test the
patched adapter (the ordinary app suite does not run dependency tests):

```text
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
```

Build and run one renderer profile at a time; Cargo overwrites example paths.
Each probe run requires a fresh output directory. On Windows, stage the pinned
ConPTY runtime before building/running, including after `cargo clean`:

```powershell
powershell -NoProfile -File tools/prepare-conpty.ps1
cargo build --release --locked --example widget_geometry
& ./target/release/examples/widget_geometry.exe target/issue-13-wgpu

cargo build --release --locked --example widget_geometry --no-default-features --features renderer-tiny-skia
& ./target/release/examples/widget_geometry.exe target/issue-13-tiny-skia
```

On macOS, invoke `target/release/examples/widget_geometry` without `.exe` after
the same build commands. Wgpu needs access to the graphics device. A successful
run has a `passed` event, three Requested entries in `cleanup.csv`, and
`completed.txt` containing `passed`.

Native Windows ConPTY execution and interactive monitor-DPI changes could not
run on this macOS host. Windows cross-target Clippy provides type checking only;
the captured fractional scales are offscreen raster tests, not WM_DPICHANGED
observations. Before marking Windows verification complete, run both native
probe profiles and inspect the screenshots, then exercise the compatibility
harness on a fractional-DPI desktop. No GitHub Actions were dispatched or
modified; the implementation commit uses `[skip ci]`.

## PR #62 review — 2026-10-27

The review reproduced a missing case in the original widget-state reset:
a relayout can precede a button press without an intervening CursorMoved event.
The widget correctly forgot its old mouse position, but mouse reports still
used that reset point. A focused regression failed on `0dd6ec1`, reporting
grid (0,0) for a click at (2,3). Presses and releases now derive their grid
position from the current Iced cursor and layout origin. This adds no state,
task, allocation or PTY operation; it computes coordinates on button events.

The public event probe now also enables mouse reporting in synthetic fixture 2
and checks the actual first-press MouseReport after each equal-size replacement.
Readiness explicitly refreshes the retained model and verifies each fixture's
known text prefix and reporting mode. A title is a control event and can be
delivered ahead of a coalesced Wakeup, so merely observing the readiness title
was insufficient to guarantee the screenshot snapshot was current.

Formatting, application and explicit adapter Clippy passed for both renderer
profiles on macOS and the Windows MSVC target. Root application/all-example
tests passed for both profiles, as did the two compile-fail doctests. Explicit
adapter suites now pass 75 tests per renderer, with the existing profile test
ignored. Both release probes passed their pixel, pointer/focus and DPI-event
checks. All six review fixture PIDs were absent after acknowledged shutdown.

The original evidence is preserved. [Review evidence](../baselines/issue-13/review/)
contains source hashes, event/cleanup records, selected screenshots and another
single-run resource observation under the same offscreen methodology:

| Renderer | Wall s | User CPU s | System CPU s | Reported maximum RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| wgpu | 0.73 | 0.08 | 0.11 | 78.02 |
| tiny-skia | 1.16 | 0.75 | 0.09 | 66.75 |

These observations retain the original limits: they do not measure Windows,
application idle memory or interactive input/render latency. Native Windows
ConPTY and monitor-DPI checks remain unavailable on the macOS review host.
Issue #13 explicitly requires affected Windows behavior to be validated with
synthetic fixtures. This is still a merge blocker: PR #62 remains draft and
unmerged until that evidence is supplied. No Actions were dispatched or edited.

## Native Windows follow-up — 2026-10-27

The current authorization permits Windows GitHub Actions; the earlier `[skip ci]`
notes above describe historical runs. Repository Actions are enabled and the
existing Windows workflow supplies native MSVC/ConPTY checks. The issue-specific
[widget geometry workflow](../../.github/workflows/widget-geometry.yml) now runs
the exact requested PR head on `windows-2025` for both renderer profiles without
changing the shared Windows workflow. It stages the pinned ConPTY runtime into
an isolated target directory, runs the adapter suite explicitly, builds the
release probe and executes its synthetic pixel and input checks. A bounded
90-second process wait terminates the synthetic process tree on timeout.

Each artifact records the source SHA/hashes, native OS/compiler, runtime hash,
executable hash, renderer name, event log, screenshots and cleanup results.
Success requires three requested backend completions, the probe's passed marker
and absence of all three fixture PIDs. Outputs live in a fresh runner-temporary
directory outside the build cache. Official actions are pinned, repository
permissions are read-only, and builds use four jobs and locked dependencies.
No production dependency, feature or runtime behavior changes accompany this
verification workflow. `actionlint` validates its syntax before dispatch.

Workflow execution results will be recorded after the first pushed follow-up
head. Hosted offscreen screenshots at fractional scales exercise native ConPTY,
Windows fonts and the chosen renderer; they do not simulate a physical monitor
transition or establish application idle memory/CPU or interactive latency.
Interactive monitor-DPI compatibility checks still require a Windows desktop.
