# R33 native chrome — implementation checkpoint

Status: subtree checkpoint for R2; the full task remains in progress. Normal
workspace composition and shortcut-aware tooltip integration are intentionally
waiting for actual merged R32, as agreed with the coordinator. No PR is ready.

## Source and ownership

- Fetched `origin/main` and verified the requested exact base
  `a5fe9fc4d8f0df53db240155ecebe0088d9898f4` (merged R31).
- Created `issue-33-native-chrome` from that commit. The original
  `issue-27-pane-move-zoom` branch, reports, artifacts, and build caches remain.
- New `ui` modules own palette, spacing, type, quiet controls, bounded plain text,
  and width-aware cached ellipsis. Existing dark/light/system resolution remains
  in R31. Shared UI type is the system sans-serif; terminal fonts remain separate.
- Subtree edits: tab strip/overflow, pane headers/borders/divider paint, existing
  startup/error/exited presentation, and close decision presentation.
- No changes to normal root composition, settings, commands, capture, gestures,
  session ownership, resize ownership, vendor code, dependencies, or Actions.

## Local checks completed

All Rust commands use `CARGO_BUILD_JOBS=4`, locked dependencies, and this
worktree's reused writable `target` directory. Verification ran locally on
macOS arm64; no remote CI was used.

| Command | Result |
| --- | --- |
| `cargo test --locked --lib ui::tests` | 1 passed: bounded Unicode/control filtering. |
| `cargo test --locked --lib app::tests::resize` | 2 passed: modal/frozen geometry and native session identity. |
| `cargo test --locked --lib tab_strip::tests` | 1 passed: finite visible range containing selected tab. |
| `cargo clippy --locked --lib --example native_chrome -- -D warnings` | Passed; existing dependency deprecation/future-incompatibility notices remain. |
| `cargo build --locked --example native_chrome` | Passed. |
| `native_chrome … --components` | Passed actual widget layout, real overflow hit-testing, and no requested follow-up redraw on two external redraw events. |
| `git diff --check` | Passed. |

Focused native component screenshots were rendered and visually inspected at
100/125/150/200% for dark/light, 320/1200 DIP. The actual strip measures 34 DIP,
the overflow target measures 26×26 DIP, selected tab stays visible at narrow
width, long ASCII/CJK titles have measured ellipsis, and inactive close glyphs
reveal on hover without changing layout. Evidence:
`docs/baselines/issue-33/components-responsive/`, including `contact.png`.
The contact rows are dark then light, each at 100/125/150/200%; each row shows
320 DIP followed by 1200 DIP. This is the actual tab-strip component, not final
full-App screenshot acceptance.

Pane headers are 26 DIP, borders 1 DIP, and visual gutters 4 DIP. The previous
pane-grid resize leeway and capture remain untouched. Pinned Iced 0.14.2 adds
the existing 8-DIP leeway to spacing for input; it has not been reduced to the
4-DIP paint width. Cell-based resize/pannable geometry remains the same owner.

## Measured baseline

Archived the exact `a5fe9fc` source into a temporary source directory, added only
the fixture harness, and built release with the same worktree target cache.
The exact harness and executable SHA-256 are retained under
`docs/baselines/issue-33/native-before-*`.

Actual native macOS arm64/wgpu window, 1200×800 logical pixels, one real PTY
running a controlled idle helper, 3 seconds settle then 24.012 seconds idle:

- Native redraws: **1** (passive raw event observer returns `None`, causing no
  measurement messages or repeated redraw requests).
- Process CPU time: **0.18 → 0.20 seconds**, a 0.02-second increase.
- RSS: **99,008 → 99,056 KiB** (`ps -o time=,rss=`).
- Font measurements / PTY resizes / grid resizes / cache invalidations /
  snapshots: **all deltas 0**.

Only two finite measurement deadline tasks exist in this fixture. No production
timer, telemetry, animation, or monitor was added. An earlier finite offscreen
driver baseline is also retained, explicitly distinct from the native-window
measurement; it had a 10ms fixture queue-drain loop and is not a product idle
performance claim. A matched after-styling native measurement remains required.

## Preserved failures and corrections

- Sandbox native graphics returned `native graphics required`; rerunning the
  same fixture with native GPU/PTY access succeeded. Red evidence remains.
- Initial component compile mismatches (padding number type, fixture error
  conversion and rectangle API) were corrected locally; logs remain.
- Native debug screenshot reproduced a cosmic-text coordinate overflow; release
  rendering missed title text. Paragraph inspection found infinite measured
  widths for long CJK labels using the inherited terminal monospace/no-wrap
  setup. The UI now uses an explicit system sans-serif token and finite measured
  ellipsis; native debug screenshots pass without vendor changes. Diagnosis and
  corrected-run logs remain; synthetic-only instrumentation was removed.
- Native heartbeat and `ask` rejected the exact live preamble worker handle
  with an execution-context error and no effects. The coordinator was informed
  through successful native status messages; IDs/capabilities were not replaced.

## Remaining acceptance and limitations

After one deliberate actual-R32 integration: compact normal root composition,
plus/menu/settings affordances, effective shortcut tooltips, and bounded passive
rows outside terminal clipping. C will integrate its new error data/actions and
rows after actual R33; no unmerged C interfaces are imported or stubbed.

Still required: final actual-App one/nested-four/eight/overflow/error/empty and
small-window screenshots at the four scales, terminal prompt/content clipping
oracles, matched after idle counters, final relevant formatting/lint/tests,
R2 review/fixes, commit/push and focused issue-closing PR.

Native Windows/ConPTY, Windows snapping, physical keyboard/monitor input and
mixed-DPI monitor movement are unavailable on this macOS host. Offscreen native
Iced plus real Unix PTY and cross-compilation cannot establish those passes.
Existing physical R13/R14 holds and the owner14 dependency waiver are preserved.
