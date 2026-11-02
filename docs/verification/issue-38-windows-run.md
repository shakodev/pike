# R38 native Windows run manifest

Status: **NOT RUN**. The connected host is macOS and the owner confirmed that no
Windows host is available. Do not provision infrastructure or enable Actions.
Run only after R34's relevant merged production startup/renderer/lifecycle changes
are integrated and independently reviewed. Keep the product defaults unchanged.

Use a native interactive Windows 11 24H2+ x64 checkout of the exact reviewed
`issue-38-renderer-selection` commit, the pinned stable compiler, Python 3.11+
and PowerShell. No debugger, shell profile, real terminal transcript, or user
environment dump is involved. Keep the machine, display, geometry, font, power
mode and synthetic children fixed across compared builds. Do not run other
benchmarks concurrently. Close test windows only through the collector.

From the checkout root, these are the intended commands (fresh output paths):

```powershell
$env:CARGO_BUILD_JOBS = '4'
python -m unittest discover -s tools -p test_renderer_comparison.py -v
cargo fmt --all -- --check
cargo clippy --locked -p pike --example renderer_comparison --no-default-features --features renderer-tiny-skia -- -D warnings
cargo test --locked -p pike --example renderer_comparison --no-default-features --features renderer-tiny-skia
cargo clippy --locked -p pike --example renderer_comparison --no-default-features --features renderer-wgpu -- -D warnings
cargo test --locked -p pike --example renderer_comparison --no-default-features --features renderer-wgpu
python tools/renderer-comparison.py build --target x86_64-pc-windows-msvc --target-directory target --output target/r38-windows-builds
python tools/renderer-comparison.py plan --builds target/r38-windows-builds/builds.json --output target/r38-windows-dry-run.json --repeats 1 --dry-run
.\tools\measure-renderer-comparison.ps1 -Manifest target/r38-windows-dry-run.json -OutputDirectory target/r38-windows-dry-run
python tools/renderer-comparison.py plan --builds target/r38-windows-builds/builds.json --output target/r38-windows-full.json --repeats 3
.\tools\measure-renderer-comparison.ps1 -Manifest target/r38-windows-full.json -OutputDirectory target/r38-windows-full
python tools/renderer-comparison.py plan --builds target/r38-windows-builds/builds.json --output target/r38-windows-startup.json --startup
.\tools\measure-renderer-comparison.ps1 -Manifest target/r38-windows-startup.json -OutputDirectory target/r38-windows-startup
python tools/renderer-comparison.py plan --builds target/r38-windows-builds/builds.json --output target/r38-windows-latency.json --latency --repeats 3
.\tools\measure-renderer-comparison.ps1 -Manifest target/r38-windows-latency.json -OutputDirectory target/r38-windows-latency
python tools/renderer-comparison.py compare target/r38-windows-full target/r38-windows-startup target/r38-windows-latency --output target/r38-windows-comparison.json
```

The full matrix is intentionally expensive and finite: 2 renderers × 7 release
profiles × 24 session/history/workload cases × 3 repeats = 1,008 runs, with
62-second idle and 600-second sustained-output intervals after ten seconds
settling plus a two-second arming guard. The idle interval leaves room for at
least 60 seconds of external one-second CPU samples. Resource runs use **zero
readbacks**. A separate 30-second four-producer/200-probe latency manifest includes
readback/validation observer costs and is grouped separately. The dry run uses
three-second intervals and is **not** budget evidence.
`-FirstRun` and `-RunCount` permit explicit chunks into separate fresh output
directories without overwriting earlier runs. Every repeat reverses build order
to expose order/warmup effects. If a staged screening/finalist comparison is used
to reduce this cost, retain the screening manifest/results and document the
independent decision narrowing candidates before measuring finalists; do not
silently omit profiles or cases. Startup adds 20 warm launches per build.

Profiles differ per Cargo invocation, never through product config/default edits:

| Profile | opt-level | LTO | codegen units | Strip |
|---|---|---|---|---|
| baseline | 3 | false | 16 | none |
| strip | 3 | false | 16 | symbols |
| codegen1 | 3 | false | 1 | symbols |
| thin | 3 | thin | 1 | symbols |
| fat | 3 | fat | 1 | symbols |
| size-s | s | thin | 1 | symbols |
| size-z | z | thin | 1 | symbols |

Every build retains the exact invocation, compiler, lock/source hashes, feature
trees, build log, app/fixture binary hashes and executable-only ZIP size/hash.
On Windows it also reuses the existing pinned ConPTY preparation/package scripts
and records the portable package plus runtime DLL hashes. The executable-only
ZIP is not mislabeled as the portable release. Ambient Rust/profile overrides
are rejected, not recorded as user environment data.

Every run retains its manifest row, owned PID/parent/creation identity, app versus
synthetic child/ConPTY-host counters, one-core CPU inputs, snapshots/font/resize/
history/input counters, native redraw histogram, exact throughput sentinels,
bounded latency samples, synthetic screenshots, graphics availability and
drained completion/cleanup results. GPU Engine counters run in a separate finite
observer; unavailable/localized counters are explicit, never zero. Shared working
sets are not summed. Installed GPUs are not proof of the adapter selected by the
lean Iced build. Metadata collection is separate from synthetic terminal data.

The fixture holds processes for one finite second after End so the external
observer can force an end counter sample before teardown. The hold and artifact
encoding occur outside the resource window. One strict validator is shared by
summary and comparison: it binds complete planned identities, verifies successful
cleanup and owned PID/creation/role coverage, rejects nonfinite/nonmonotonic
counters, requires sufficient sample-window coverage, and rebuilds cost statistics
from current raw rows. Exact creation ticks are parsed as integers without float
rounding. Native process tables are bounded to 32,768 rows and 16 MiB; the
supported 33-process ten-minute table can exceed 2 MiB. Stale summaries and
missing/failed planned cases remain explicit failures or blockers.

Before the first controlled run, attach operator metadata to the evidence root:
monitor model, physical resolution, refresh rate, desktop scale, topology, HDR,
power mode and whether the machine is on AC; exact Windows build and GPU driver
come from `machine.json`. Confirm the recorded client/physical geometry is stable
between profiles, installed Consolas font hashes match, and the window is visible
and focused. The synthetic child writes fixed 120-column history rows; retain
the actual per-pane terminal column/row counters rather than claiming every pane
is 120 columns. A separate 120-column-per-pane budget run requires a display and
geometry that actually support it; the fixed 1,600×1,000 DIP comparison alone
does not satisfy that part of the broader PLAN performance protocol.

Review synthetic Unicode/combining/wide/reverse/cursor pixels and the existing
relevant text-quality fixtures on both profiles at the same fonts/scales. Keep
R13/R14 physical holds and owner14 waiver separate. The fixture uses synthetic
App input, not physical keyboard events; snapshot/readback timing and native
redraw-request intervals must keep those names. Physical input-to-display and
actual GPU presentation timing remain separate evidence, never inferred from
these measurements. Retain any missing counter or failed budget as a limitation.

The final decision must compare correctness, latency distributions/sample counts,
delivered throughput, startup, app/private/child/ConPTY costs, CPU, GPU availability,
threads/handles and both archive scopes. Explain LTO/codegen/stripping/opt-level
tradeoffs using latency and cost as well as size. Select a default only with native
Windows evidence and independent review; retain a fallback only for demonstrated
hardware need. Until then, the decision is **PENDING**, R38 stays open and the
draft PR must not contain a completed `Closes #38` claim.
