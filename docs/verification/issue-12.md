# Issue #12 — cached metrics and deduplicated resize work

Based on `origin/main` `fa8023d`, after prerequisite #11 closed. The separate
local main resource-fix commit and unmerged issue #10 verification are excluded.
No dependencies, lockfiles, renderer defaults, Alacritty sources or workflows
change. The existing emulator, event bridge and native ConPTY remain.

## Behavior

`TermFont` measures one advanced-shaped `m` on attachment and caches its logical
cell size. Only a changed font family, font size or relative line height measures
again. Identical settings do no shaping. `FontSettings.scale_factor` is relative
line height, **not DPI**. Non-finite/nonpositive settings retain valid defaults
or the previous value; finite extremes clamp font size to 1–256 logical pixels
and relative line height to 0.1–8. These are adapter safety limits, independent
of the future product zoom range.

The pinned Iced 0.14.1 window state constructs its logical viewport from physical
client size divided by monitor DPI times application scale. `ScaleFactorChanged`
updates that viewport; paragraphs and widget layouts already use logical units.
Consequently a pure monitor-scale change does not require reshaping logical cell
metrics. A changed logical pane area goes through the same resize calculation
as a window drag. No DPI multiplier is applied to line height or measured cells.

`geometry.rs` keeps the last usable layout independently of cached font metrics
and applied terminal geometry. It preserves fractional cells for division,
rendering and selection hit-testing; integer conversion occurs only for native
pixel metadata. Initial synthetic layout products use f64 to preserve the
backend's initial 80×50 grid without float-rounding loss. Layout messages carry
only their area, so a queued widget resize cannot overwrite a newer font.

Zero, negative, NaN, infinite or sub-cell pane areas preserve the last usable
layout and applied grid. A valid new font is remembered even when the current
pane cannot fit one cell; restoration uses those cached metrics. Finite huge
areas and tiny positive cell metrics clamp the viewport to **512 columns × 512
rows**, bounding allocation and remaining below ConPTY's signed coordinate
limit. No path emits a zero or wrapped native size. These are viewport limits;
the separate history line/byte limitations from #11 still apply.

Only changed rows/columns call `Term::resize`. On Windows only changed
rows/columns notify ConPTY, which consumes no pixel metrics; Unix also compares
rounded cell-pixel metadata. Sub-cell drags remember the newest usable area
without native messages, grid reflow or explicit terminal-cache invalidation.
EOF draining still defers reflow; the newest geometry applies after completion
without notifying a disposed PTY, including after bridge failure.

Backend mutation, viewport capture and repaint have separate dirty flags.
Output wakeups, selection, effective geometry, history changes and final
completion refresh visible content. Accepted input snapshots only when it
actually returns a scrolled viewport to the bottom; echoed output arrives via
the existing event bridge. Title, bell, protocol writes, bindings and no-ops do
no presentation work. Theme and hyperlink-hover changes repaint without copying
viewport cells. Hidden events retain dirty state without snapshots/cache clears;
activation still captures once. Explicit `refresh`/`final_content` refresh and
invalidate together, so their new snapshot cannot leave cached geometry stale.

`Terminal::work_stats()` exposes saturating per-session font-measurement, native
notifier-call, grid-resize and explicit-cache-clear counters. It retains no
terminal text and adds no sampler, timer, task or queue. Native counters measure
calls, not child acknowledgement; the size-reporting fixture supplies the latter.
Iced's cache may also regenerate when its layer size changes; that automatic
work is outside the explicit-clear counter. This change adds no frame-rate
throttling or polling.

## Verification

Both lean renderer profiles pass formatting, denied-warning all-target
application/adapter Clippy, 39 library tests, two application tests, two
compile-fail doctests, five example tests and **70 adapter tests**. The opt-in
startup and history profiling probes stay ignored in ordinary tests. App and
all examples build in release for both renderers. Builds, tests and profiling
run sequentially.
Renderer/runtime feature isolation, the feature-guard regressions and the
measurement-support/trace validation checks also pass locally.

Eleven new adapter regressions cover the full font cache key, repeated/invalid
settings, finite extremes, fractional boundaries/hit-testing, startup rounding,
font changes while too small, invalid/minimized layouts, 100/125/150/200% logical
scale conversions, no-op/title/input handling, visible wakeup floods, hidden
output and paint-only changes. Existing native output/drain/shutdown regressions
continue to pass. Development caught an initial fractional-product rounding
error before the final full suites; its dedicated regression now passes.

The opt-in `resize_work` example attaches on its calling thread and drives the
existing real ConPTY with `pty_fixture`. Numeric nonces on size/flood commands
avoid replaceable-title coalescing; ordinary fixture commands stay compatible.
The probe checks child-reported dimensions, counter deltas, natural exit 41,
EOF drain and acknowledged cleanup. It retries only synthetic size queries if
the worker has not yet applied a queued resize; production adds no retry loop.

| Native probe stage | Child grid | Extra measurements / native resizes |
| --- | --- | --- |
| Initial 8.5×18.25 cell layout | 120×40 | 1 initial measurement / 1 resize |
| 1,000 sub-cell drags | 120×40 | 0 / 0; also no grid reflow, snapshot or cache clear |
| 100% logical scale, 100 repeats | 120×40 | 0 / 0 |
| 125% logical scale, 100 repeats | 96×32 | 0 / 1 |
| 150% logical scale, 100 repeats | 80×26 | 0 / 1 |
| 200% logical scale, 100 repeats | 60×20 | 0 / 1 |
| Invalid/minimized area then restore | 60×20 | 0 / 0 |
| Font 20, 100 repeats | 43×14 | 1 / 1 |
| Relative line height 1.6, 100 repeats | 43×11 | 1 / 1 |
| 4,096-line visible output flood | 43×11 | 0 / 0 |
| 4,096-line hidden output flood | 43×11 | 0 / 0; zero hidden snapshots/cache clears |

The final tiny-skia probe uses 0.250 s process CPU over 0.629 s observed wall
time and peaks at 9.39 MiB sampled private bytes. Its two floods use the real
parser/PTY; it creates no GUI window or render surface, and child resources are
excluded. Cumulative elapsed columns are trace timestamps, **not** input/render
latency percentiles. Raw counts, sampled process data and executable/source
hashes are under [issue-12 baselines](../baselines/issue-12/).

Native cmd and PowerShell two-session GUI smoke checks pass layout resize,
size markers, independent child exits 41/42, final output and cleanup. The
tiny-skia cmd idle run records 61 responsive samples over 61.39 seconds with no
process CPU-counter advance, 12.39–12.52 MiB private bytes, 23.51–23.57 MiB
working set and 9–13 threads. Fixture cleanup succeeds. These are whole-GUI
process measurements; the shell/native host resources are recorded separately.
The screenshots still show the equal-size pane cache-position defect tracked
separately by #13. It is already reproduced against pre-R11 and R11 sources
and remains outside this patch. Unicode drawing
limitations also remain assigned to #18. This PR does not claim either fixed.

Physical monitor DPI transitions and moving between differently scaled monitors
were not automated: they need an interactive Windows display-configuration
run. The four-scale matrix verifies Iced-equivalent logical bounds against real
child console sizes, plus actual font/line-height changes; it does not change
Windows display settings or claim OS notification coverage. GUI minimize/restore
notifications likewise need an interactive check; zero/tiny restoration is
verified directly through the same public resize command on native ConPTY.

## Reproduction

Prepare the pinned runtime in the actual Cargo target directory first. Run
renderer profiles sequentially because Cargo overwrites example output paths.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo build --release --locked --bins --examples

cargo clippy --locked --all-targets --no-default-features --features renderer-tiny-skia -- -D warnings
cargo test --locked --no-default-features --features renderer-tiny-skia
cargo test --locked --examples --no-default-features --features renderer-tiny-skia
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/tiny-skia,iced/tokio -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
cargo build --release --locked --bins --examples --no-default-features --features renderer-tiny-skia

powershell -NoProfile -File tools/measure-resize-work.ps1 -OutputDirectory target/resize-work-final
powershell -NoProfile -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 2 -Mode smoke -Renderer tiny-skia -OutputDirectory target/resize-cmd-smoke
powershell -NoProfile -File tools/measure-compatibility.ps1 -Shell powershell -Sessions 2 -Mode smoke -Renderer tiny-skia -OutputDirectory target/resize-powershell-smoke
powershell -NoProfile -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 1 -Mode idle -Renderer tiny-skia -OutputDirectory target/resize-idle
```

Use fresh output directories and `-Executable` for custom target paths. The
GUI collector needs process-tree access outside the workspace sandbox. Commit
uses `[skip ci]` to retain the no-Actions constraint; no workflows are changed
or dispatched.
