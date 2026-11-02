# R38 renderer comparison — in progress, selection pending

This work starts from merged main `f1737cfd3b5b85986a9a34acafd4f0c0cf1abfef`
(R33/PR83). Product renderer, Cargo features/dependencies/defaults, shared runtime,
and production App/session/focus/lifecycle code remain unchanged. R38 is **not
complete**. Native Windows 11 measurements, integration of relevant R34 changes,
and independent review are required before a renderer/profile decision.

## Isolated fixture checkpoint

`examples/renderer_comparison.rs` and its directory use the real App settings
loader, task/subscription/view, split/tab controls and existing terminal counters.
The intended matrix is 1/4/8 visible panes with 0/8 additional hidden live sessions,
empty/saturated 2,000-line histories, idle and bounded 2 MiB/s producers. Children
use the same Alacritty PTY; there is no second emulator or runtime. Histories,
command vocabulary, chunks, events, latency results and deadlines are bounded.

Setup selects each target through App controls before synthetic input; it does
not disguise control commands as terminal protocol replies. Flood producers are
armed before measurement at a common bounded epoch, then hidden tabs stay hidden
through the measurement interval. Cleanup requests cooperative child exit and
checks drained backend completion before closing the App. An external deadline
and owned-process identity checks belong to the collector.

Latency labels distinguish synthetic App input → visible snapshot from synthetic
App input → native screenshot callback. Neither is physical keyboard-to-display
latency. Redraw-request interval histograms are not presentation or GPU timings.
The pinned lean Iced build disables `sysinfo`; actual selected GPU adapter must
be reported unavailable through that API, rather than enabling a feature or
guessing it from installed GPU devices.

All verification to date ran locally on macOS/aarch64, Rust 1.97.1 with
`CARGO_BUILD_JOBS=4`, in this worker's reused `target` cache. No Actions run was
dispatched. Initial `cargo check --locked -p pike --example renderer_comparison`
reported missing production APIs despite their presence in source: cached vendor
metadata predated the branch switch. Updating only the mtime of
`vendor/iced_term/src/lib.rs` rebuilt its metadata, without changing tracked
contents or deleting the build cache. The next red exposed an attempted Iced
sysinfo API use; that was removed, and the third scoped check passed. The first
Clippy red (collapsible conditional) is retained and corrected. See
`docs/baselines/issue-38/checkpoint/` for raw logs.

The first sandboxed native launch could not connect to macOS GUI services; its
exact test-owned process was terminated before retrying with native access.
The native-access retry reached child readiness but did not acquire real macOS
window focus, then hit its finite deadline and cleaned up. An explicit
`--synthetic-focus true` option permits labeled non-Windows state-machine dry
runs; it is rejected on Windows and cannot qualify Windows native acceptance.

## Remaining work and boundaries

### Independent review corrections (local checkpoint)

R2 found that the immediate screenshot could contain the preceding READY scene,
that End retained a pending query, and that incomplete sample counts could still
write the fixture completion marker. Those findings are accepted. The correction
encodes the exact owned child PID and nonce in a terminal-emitted truecolor space
strip and validates its actual screenshot pixels. Only frame-triggered readbacks
are attempted, at most three per nonce. Blank, stale, wrong-child and late
callbacks cannot become latency rows; End retires pending work and completion
requires every requested validated sample. Callback receipt is timed before
pixel decoding; decoding overhead has its own raw column.

The interval overrun was independently traced to this fixture's unbuffered
per-pixel file writes on the UI thread. It was not evidence of a production
renderer performance defect. The writer now buffers rows. Only bounded first/last
proof strips are retained, and their artifact encoding happens after measure_end;
full screenshot buffers are released. Native fractional pixel edges are tested.
Parent monotonic/wall intervals and every producer's start/end epochs must overlap
the requested window; materially late End or missing producer coverage invalidates
the run. Writer-rate and parser-sentinel-observed throughput keep separate names.

The corrected native macOS debug 4-visible/8-hidden saturated flood dry run
completed all 12 drained backends, validated both requested child-specific pixel
nonces, and observed a 3,025 ms window for a requested 3,000 ms. All eight hidden
sessions had zero snapshot/font/resize/cache deltas. This is a short synthetic
focus-authority workflow check, not a performance budget, release comparison or
Windows result. Raw counters, epochs, proof strips, sample rows and bounded frame
histogram are in `docs/baselines/issue-38/correction/native-pid-nonce/`.

Local correction checks: denied-warning fixture Clippy and 13 fixture tests pass;
scoped Windows MSVC compilation passes on the recorded workflow source. The two
renderer release baselines built earlier from `40f9ee0` plus the empty-history
reset retain invalid pre-correction native measurements, source/binary hashes and
feature trees; those are not corrected timing evidence. The next coherent workflow
checkpoint will receive both-profile checks and release dry runs after relevant
R34 integration. No vendor source contents, dependencies or production defaults
changed. The owner has confirmed that no Windows host is available; this creates
no new waiver and leaves native selection blocked.

The isolated workflow is implemented in `tools/renderer-comparison.py`, its
bounded support/dry-run modules, the Windows process collector and finite GPU
watcher. It builds per-invocation release profiles with isolated native/dev
feature trees, source/compiler/binary/package identities, finite run manifests,
20-launch startup manifests, validated per-run summaries and comparative JSON/CSV.
Comparison reports expose missing/failed manifest coverage, geometry/source/machine
mismatches, unavailable counters and pending native/review gates; they never
automatically change defaults. Existing Windows feature and ConPTY package tools
are reused without modification. Private bytes are summed only across owned
processes; shared working sets remain separate.

Fourteen Python evidence/feature/manifest/BOM/cost-grouping/coverage regressions
pass locally. The finite Unix/native watcher independently completed idle and
flood dry-run scenarios and established app exit, both ready children absent and
bounded stderr reader completion. Those checks explicitly contain no native
Windows private-byte/working-set/CPU/GPU values. PowerShell is unavailable on this
host; execution of the Windows collector, GPU watcher and portable ConPTY package
steps is unperformed, with no synthetic substitution presented as native data.

R2's later delayed-Settle P2 is corrected: Settle requires its expected state,
End/close are absorbing, and real CloseWindow intent retires pending work. R2's
unchanged semantic four-regression adapter passes against the correction, with
source hashes proving restoration after the test-only append. An additional
actual CloseWindow intent regression covers that path directly.

R2's aggregation P1/P2 findings are accepted and corrected with a shared strict
raw validator: summary and comparison bind complete planned build/run identities,
verified app/child/ConPTY PID/creation roles, finite monotonic counters, complete
sample coverage and successful cleanup. Comparisons recalculate raw statistics
and reject stale source hashes. A 16 MiB/32,768-row process-table bound accepts
the supported full scenario; ordinary metadata stays capped at 2 MiB. Creation
ticks preserve integer precision. Nonfinite geometry is rejected. R2's unchanged
positive control and six rejection bodies pass with a declared adapter adding
only required new collector/owned-tree schema fields; invented unit-test rows
are stored separately under `strict-validation/independent-synthetic-only`.

Resource plans now use zero screenshot probes, including sustained output.
A distinct `--latency` plan runs four producers for 30 seconds with 200 validated
probes; its readback/validation costs stay in separate groups. Idle resource
windows are 62 seconds to capture at least 60 seconds of external one-second
CPU observations. A finite one-second post-End hold allows a forced end counter
sample before owned-child teardown. Windows stderr completion and GPU observer
termination are explicit outcomes; these native branches remain unexecuted.

The exact Windows procedure is in `docs/verification/issue-38-windows-run.md`.
Actual R34 has no merged main ref: its unmerged boot API has not been imported.
Current-main release workflow checks therefore retain the legacy constructor
scope; final comparison must be repeated on the reviewed actual34 product boot
path after integration. Complete raw machine/counter metadata, feature
trees, binary/source/build identities, profile tradeoffs, startup repetition,
portable archive costs, output/text-quality review, and native Windows runs
remain pending. macOS dry runs cannot select a Windows default or justify a
hardware fallback. Preserve the separate R13/R14 physical holds and owner14 waiver.
