# R38 renderer comparison — workflow/evidence, acceptance incomplete

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

The initial `40f9ee0` interval overrun was independently traced to this fixture's unbuffered
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

R2's two later controlled-comparison P2 findings are corrected: cohorts include
the immutable build/config/source/artifact identity, with invocation path options
normalized and executable/output paths excluded. Distinct binaries or profiles
cannot pool just because renderer/variant labels match. Canonical per-slot
terminal columns/rows/visibility must match within a controlled scenario across
profiles, alongside window DIP/scale. R2's unchanged matching control plus both
mutation cases pass through a retained-seed/output-path adapter; source hashes,
portable runner and synthetic data are under `comparison-axes/`. Full native
resource counter sequences also reject material gaps in one-second sampling.

The exact Windows procedure is in `docs/verification/issue-38-windows-run.md`.
Actual R34 has no merged main ref: its unmerged boot API has not been imported.
Current-main release workflow checks therefore retain the legacy constructor
scope; final comparison must be repeated on the reviewed actual34 product boot
path after integration. Complete raw machine/counter metadata, feature
trees, binary/source/build identities, profile tradeoffs, startup repetition,
portable archive costs, output/text-quality review, and native Windows runs
remain pending. macOS dry runs cannot select a Windows default or justify a
hardware fallback. Preserve the separate R13/R14 physical holds and owner14 waiver.

## Final available local verification and unresolved results

Draft [PR #84](https://github.com/shakodev/pike/pull/84) remains incomplete.
The final finite release experiment used immutable source
`7221910ccd8ae911c029bdd10765cf9f850a588b`, tiny-skia-only and wgpu-only builds,
and baseline plus thin-LTO/codegen-1/stripped invocations. Each build retains its
compiler/source/feature/binary/archive identity. The full native Windows seven-
profile experiment has **not** been run, and the combined local thin profile
does not isolate a causal LTO/codegen/stripping tradeoff.

| Short synthetic macOS run family | Cases | Valid workflow | Invalid |
|---|---:|---:|---:|
| Passive 1/4/8-visible, 0/8-hidden, empty/saturated, idle/flood | 96 | 66 | 30 |
| Separate four-visible, two-probe readback diagnostic | 4 | 2 | 2 |

These are three-second workflow runs with synthetic App focus authority, not
resource budgets, physical input/display measurements or a Windows renderer
comparison. All 816 announced child instances across these 100 cases were absent
after their app exited, and every bounded stderr reader finished. The one planned
matrix was not repeated to replace its reds. The four latency diagnostics contain
two validated samples each for wgpu; each tiny-skia run obtained only one of two
before its invalid window ended.

The 30 passive invalid results have distinct causes in the raw evidence:

- 21 tiny-skia flood cases observed 3,811–5,653 ms for a requested 3,000 ms. They
  fail duration/producer-window validity; minimum producer coverage of the actual
  parent interval was 53.14%. The extra `invalid_overlap` label also includes the
  invalid parent duration condition, so it must not be interpreted as an
  independently established parser failure.
- Eight cases hit the finite external readiness deadline before `measure_begin`:
  seven tiny-skia and one wgpu. Announced children were cleaned up. No interval,
  resource or latency result is claimed for those cases.
- One wgpu-thin, one-visible/eight-hidden saturated case was rejected before
  measurement: its visible terminal reported 1,567 history lines rather than
  2,000. Readiness/history coordination remains unresolved; no saturation pass is
  inferred from another case.

The passive source-722 trace could additionally emit `incomplete_latency,0,0`
after any invalid interval even though zero probes were requested. That label-only
defect is corrected by checking sample completion separately. The retained raw
trace is unchanged, and those cases remain invalid for their actual timing or
readiness/history reasons.

A single matched tiny-skia, one-visible/empty-history/no-readback flood diagnostic
added numeric timer arming/wake/handling observations, without changing the timer
behavior. It armed 0.132 ms after Begin and woke at 3,001.231 ms; the fixture
handled the message another 2,603.229 ms later, yielding a 5,604.507 ms window.
The child was active for 3,002 ms. App and child exited, no child remained and the
reader finished. This locates that delay after timer wake and before fixture
message handling; it does **not** distinguish production UI work, fixture work or
host scheduling. No profiler or matched product-only measurement was performed.
The earlier unbuffered-image-write diagnosis applies to the earlier development
red, not automatically to these later readback-free failures.

Raw CSV/JSON, every run's exact manifest row and source/binary/profile identity,
feature trees, all cleanup outcomes, and the single-case source patch are in
[`final-local/`](../baselines/issue-38/final-local/). The diagnostic ZIP is about
1 MiB and excludes binaries and all invented unit-test cost data; its exact hash
and size are recorded separately. Windows private bytes/working set/CPU/GPU/
thread/handle costs are unavailable here. Pure parser service time is unmeasured:
the throughput endpoints distinguish child writes from UI observation of the
parser sentinel.

Verification ran **locally**, with `CARGO_BUILD_JOBS=4`, locked dependencies and
this worker's preserved target cache:

| Command / check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked -p pike --example renderer_comparison -- -D warnings` | Passed |
| Same scoped Clippy with `--no-default-features --features renderer-tiny-skia` | Passed |
| `cargo test --locked -p pike --example renderer_comparison` on each isolated renderer | 14 passed per renderer |
| `cargo check --locked -p pike --example renderer_comparison --target x86_64-pc-windows-msvc` | Passed; compile only |
| Author Python evidence/feature/manifest/cap/identity tests | 14 passed |
| Unchanged R2 semantic cancellation cases | 4 passed, source restored with matching hash |
| Unchanged R2 aggregation control/rejection bodies with declared schema adapter | 7 passed |
| Unchanged R2 cohort/terminal-geometry bodies with retained-seed path adapter | 3 passed |
| Baseline and thin release builds on both isolated renderers | Passed; 4 immutable local artifacts |
| Finite native macOS dry-run matrix and single attribution case | Results above; reds remain invalid |

No production App/root/focus/capture/session/renderer/lifecycle code, Cargo
dependency/feature/default/runtime setting, vendor contents or workflow file was
changed. Patched-dependency tests were not required because no vendor contents
changed. CI/Actions stayed disabled; no remote verification was run. The old
`fc5db2b` branch, untracked R32 handoff, prior reports and build cache are preserved.

Remaining gates include the unresolved local readiness/history/timing results,
the unmerged actual34 boot/renderer/lifecycle integration, native Windows 11
ConPTY/process/GPU and profile measurements, text-quality/physical checks and an
independently reviewed decision. The owner confirmed no Windows host is
available. Product defaults remain unchanged; no fallback or waiver is invented,
and R38 is not closed. Full task acceptance is **not achieved**.
