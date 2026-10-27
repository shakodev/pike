# Issue #11 — capped history and viewport snapshots

Implemented from upstream main `dc50610` after prerequisite #5 was closed.
This branch does not include the separate local resource-fix commit or the
unmerged issue #10 verification PR. No dependencies, lockfiles, renderer defaults,
Alacritty sources or workflows change. The existing ConPTY/emulator remains.

## Ownership and API

Alacritty's Term owns the authoritative primary/alternate buffers and their
selection/scroll state. Presentation keeps only an owned, row-major vector of
visible cells and cursor/selection/mode metadata. It reuses the allocation on
same-sized refreshes; absolute cell points and display offset are preserved
when scrolled back. No production full-history clone or transcript remains.
Cell extras use Alacritty's existing Arc/COW representation.

`BackendSettings::history` is a validated `settings::HistoryLimit`: default
2,000 lines, range 0–10,000. Invalid limits cannot reach Backend through that
type. `BackendCommand::SetHistoryLimit` changes it live using Alacritty's existing
Config/set_options API. That API updates the primary buffer when alternate
screen is active; alternate screen continues to have no history. Reductions
clamp display offset, and selection extraction uses the authoritative model.

Prepared backends start without a snapshot. UI `attach` makes them visible;
`attach_hidden` keeps the count at zero from construction until activation.
The application's PreparedSession offers the same hidden handoff through its
existing cancellation/admission gate. UI attachment seeds font cell metrics
without reflowing the initial grid or notifying the PTY.
`Terminal::set_visible(false)` releases presentation cells and clears the canvas
cache. Callers must keep the Terminal, its subscription, and session-ID event
routing alive: hidden sessions still parse output, write protocol replies,
handle failures and acknowledge shutdown. Ordinary hidden handling and explicit
`refresh()` do no viewport work. An explicit font change still updates geometry.
`set_visible(true)` refreshes once; repeated true calls do not refresh again.

Exited models retain their bounded Alacritty buffers and can be reactivated,
scrolled and selected after native resources have been released. Valid layouts
reflow only the retained model after backend completion, so a child that exits
before its first visible layout still exposes its newest text in the pane.
Undersized layouts remain ignored, no disposed PTY receives resize messages,
and parsing geometry stays fixed during an in-flight EOF drain. `final_content`
returns None while hidden; activate first. `renderable_content` exposes the
current visible snapshot without refreshing. `snapshot_stats` observes only
counts, dimensions and active-screen history size; it never captures cells.
Dropping Terminal still requests backend shutdown.

This changes the patched adapter API: `RenderableContent::grid` becomes
`viewport`, and the cursor position is `cursor_point`. `viewport.display_iter()`
provides absolute points; `viewport.cells()` exposes the visible row-major slice.
The view, lifecycle tests and shutdown example have been updated. Existing
BackendSettings literals using `..Default::default()` keep compiling.

```rust
use iced_term::{BackendCommand, Command};
use iced_term::settings::HistoryLimit;

terminal.handle(Command::ProxyToBackend(BackendCommand::SetHistoryLimit(
    HistoryLimit::try_from(2_000)?,
)));
terminal.set_visible(false);
// Continue routing this session's events while it is hidden.
terminal.set_visible(true); // One current viewport, including after exit.
```

Copy/URL extraction read the authoritative model on demand, rather than storing
history in a presentation grid. Multi-page selection therefore survives the
snapshot change. This does not add clipboard, keyboard or hyperlink features.

## Verification

Native Windows checks cover 59 adapter tests per renderer plus one deliberately
ignored profiling test. Eleven new regressions cover validated limits; saturated
history and reused viewport allocation; output while scrolled back with absolute
cursor/selection coordinates; zero history; primary/alternate transitions and
history reduction while alternate is active; wide/combining cell metadata;
multi-page copy; hidden flood with no snapshots; hidden font geometry changes;
and reactivation of final output after native ConPTY exit.
An application regression checks initially-hidden attachment through LaunchPool
and acknowledged cleanup through the normal handoff ownership path.

### PR #60 review follow-up

Review of `94869fe` reproduced stale geometry when a hidden session received a
layout during shutdown's EOF drain, then activated after backend completion.
The layout pixels were remembered but never replayed. A deferred-resize flag now
applies the latest valid layout before the next visible snapshot, after disposal
is acknowledged. Hidden refreshes still take zero snapshots; activation takes one.
Resizes are also accepted after event-bridge failure, using the same deferred or
completed-model-only path. Input remains closed and the disposed PTY is never
notified. Ordinary snapshots do not issue additional resizes.

The new native ConPTY regression deterministically holds completion observation
pending while the real backend closes and joins. It failed on the original head
for normal shutdown, then on the partial fix for bridge failure; it passes with
both fixes. Formatting, all-target Clippy, application/example/adapter tests and
release builds were repeated sequentially for both renderers. The original
history/smoke/idle measurements below retain their original provenance; review
validation and a fresh release idle observation are recorded separately in
`docs/baselines/issue-11/review/`.
The reviewed release's 61-sample idle run spans 61.48 seconds, with no measured
CPU-counter advance, 12.28–12.40 MiB private bytes and 9–13 threads. Every sampled
window responds; the synthetic child exits with 41 and cleanup completes.

Main subsequently advanced to `2b54452` (PR #59, independent workspace/session
state). Integrating it conflicted only where two tests had been inserted at the
same location in `src/launch/tests.rs`; both the reserved-launch module and hidden
attachment regression are retained. The automatic production merge preserves
`attach_hidden` on the existing shared handoff gate and the new identity types.
Formatting, all-target application/adapter Clippy, tests and release builds pass
again for both renderers: 39 library tests, two application tests, two compile-fail
doctests, five example tests and 59 adapter tests. Opt-in startup/history probes
remain ignored in ordinary suites. Final integration evidence, source hashes and
an additional release idle observation are under `review/integration/`.
That integrated release records 61 responsive samples over 61.49 seconds, no
CPU-counter advance, 12.27–12.39 MiB private bytes and successful child cleanup.

The native hidden-flood test consumes all backend events through source/backend
completion while snapshot counts stay unchanged. Activation adds exactly one
snapshot and exposes the final marker. Tests also assert that repeated hide/show
and explicit hidden refresh cannot create extra presentation copies. Application
tests and example tests continue to cover startup cancellation and shutdown.

The opt-in release probe is an emulator/snapshot allocation-footprint benchmark,
not an Iced render/input benchmark. It executes the real Alacritty parser and
grid at 120 columns × 30 rows, filling to the cap twice before taking 100 timed
copies. The `full` mode repeats the exact `grid().clone()` presentation algorithm
from the R03 source, once with its 10,000-line default and once at 2,000 lines.
This is a controlled replay of that algorithm on the current pinned sources,
not a new run of the old R03 GUI executable.

`tools/measure-history.ps1` samples process private bytes, working set, CPU,
threads and handles externally. Each retained phase lasts 250 ms. Transition
markers precede allocation/free work, and the collector discards samples whose
phase changed during the process-property reads. Phase summaries report medians
and min/max, so filling/copying/releasing work is not mislabeled as steady state.
Normal tests do not run these holds or the profiler. Production adds no timer,
poller, task, unsafe code or terminal-content logging.

## Windows release results

Recorded on Windows 11 Pro build 26200, Ryzen 5 2600X (12 logical cores),
16 GiB class RAM, RTX 2070 and Rust/Cargo 1.97.1. The native tests and GUI use
the repository's pinned ConPTY DLL/host. [Provenance and source hashes](../baselines/issue-11/provenance.json)
and [raw CSV/JSON/screenshots](../baselines/issue-11/) identify the builds.
Measurements are outside a debugger; the profile uses fresh processes per case.

| Presentation / history | Logical cells copied per refresh | Copy p50 / p95 µs | Retained process private MiB |
| --- | ---: | ---: | ---: |
| R03 full clone / 10,000 | 1,203,600 | 9,959.0 / 15,607.3 | 93.17 |
| Full clone / 2,000 | 243,600 | 1,979.3 / 3,267.0 | 19.30 |
| Viewport / 2,000 | 3,600 | 35.8 / 40.5 | 7.09 |
| Viewport / 10,000 | 3,600 | 35.8 / 36.1 | 31.50 |
| Hidden / 2,000 | 0 | no copy | 7.02 |
| Viewport / zero | 3,600 | 35.7 / 35.8 | 1.07 |
| Viewport / 2,000, 512 combining marks per first cell | 3,600 | 35.6 / 35.7 | 11.38 |
| Viewport / 2,000, 4,096 combining marks per first cell | 3,600 | 35.6 / 36.4 | 41.10 |

Copy quantiles are nearest-rank values from 100 iterations in one observation,
not end-to-end input/render percentiles. Legacy logical cell counts exclude
spare cached rows, which the old Grid clone also copies. Process private bytes
include the test harness, authoritative buffers, snapshot and allocator caches;
they are not an exact Rust live-allocation count. Full clone repeatedly allocates
rows, while the viewport pointer stays unchanged for all 100 refreshes.

At 2,000 plain-text lines, saturated and second-fill plateau private bytes are
both 7,344,128. With the viewport retained, the process is 6.12 MiB above its empty
phase, below the 25 MiB component allowance at 120 columns. Each combining case
also reaches exactly 2,000 accessible history lines and a stable second-fill
plateau, but increasing marks raises retained bytes. The 4,096-mark case exceeds
the 25 MiB allowance; it illustrates the residual per-cell byte limit below.
CPU time for the full 10,000-line test process is 1.171875 s, versus 0.046875 s for
the 2,000-line viewport case, including both fills, test/collector coordination
and the 100 copies. This is not a PTY-throughput or idle-CPU comparison.

Native cmd and PowerShell two-session smoke traces pass resize, background-first
child exit 42/41, final markers, backend completion and fixture cleanup. A native
one-session cmd idle run with tiny-skia, ten seconds settling and 61 samples over
61.54 seconds, records 0.00% CPU on one core, 12.36–12.48 MiB private bytes,
23.06–23.11 MiB working set and 9–13 threads. All sampled windows respond; the
fixture exits with 41 and no process survives. Zero CPU means the Windows process
CPU counter did not advance during this interval.

Visual smoke inspection finds the equal-size pane swap cache-position defect
tracked by #13: labels swap while cached terminal text stays at its old position.
The same screenshots reproduce it using the pre-R11 adapter, SharedExecutor and
compatibility sources (verified identical to `dc50610`), built from the separate
`8bc77f8` worktree. Both baseline and candidate evidence are retained. This PR
does not claim pane-movement correctness or include the separate cache/identity
fix; its viewport contents and native scenario traces are correct. The original
combining-glyph drawing limitation also remains outside this history change.

One overlapping build/test verification run timed out in the existing natural-exit
and flood/resize ConPTY checks. Both passed in isolated reruns and the full
non-overlapping suite; no timeouts were relaxed. Final verification runs execute
builds/tests and profiling sequentially.

## Residual byte bounds

History is a **line cap, not a byte cap**. Alacritty keeps up to 1,000 spare rows
for reuse, so shrinking a limit or setting zero need not return all row capacity
immediately. Viewport allocation can retain capacity from a larger geometry.
There is one viewport copy of visible cells, plus a cursor cell for metadata.

A cell's combining-character vector or hyperlink strings can grow independently
of line count. The pathological cases put 512 or 4,096 combining marks in the
first cell of each otherwise 120-column row. They demonstrate increased retained
memory at the same 2,000-line plateau; they do not claim a hard byte bound or
worst-case Unicode budget. A strict byte policy requires separately scoped
emulator/input decisions. Cell metadata is retained faithfully; this task does
not implement the Unicode drawing work assigned to issue #18.

The 25 MiB target for 120-column/2,000-line history can be assessed for the
measured plain-text emulator component. Whole-application GPU surfaces, child
processes, arbitrary cell extras, geometry and long-duration stress remain
separate costs. Do not treat component measurements as the final MVP memory gate.

## Reproduction

```powershell
powershell -NoProfile -File tools/prepare-conpty.ps1
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
cargo test --release --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio --no-run --message-format=json
```

Use the test executable reported by Cargo's final compiler-artifact JSON record:

```powershell
powershell -NoProfile -File tools/measure-history.ps1 -TestExecutable target/release/deps/iced_term-<hash>.exe -OutputDirectory target/history-measurements
powershell -NoProfile -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 2 -Mode smoke -Renderer tiny-skia -OutputDirectory target/history-smoke
powershell -NoProfile -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 1 -Mode idle -Renderer tiny-skia -OutputDirectory target/history-idle
```

Choose fresh output directories. Build/run renderer comparisons sequentially
because Cargo overwrites example paths. The recorded runs share the repository's
root target directory, selected through CARGO_TARGET_DIR and `-Executable`.
The existing native GUI collector requires process-tree access outside the
workspace sandbox. The completion commit skips CI to preserve the no-Actions
constraint; no workflows are changed or dispatched.
