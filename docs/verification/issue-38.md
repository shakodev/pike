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

Collector/aggregator/build-profile scripts are being implemented and are not
accepted at this checkpoint. Complete raw machine/counter metadata, feature
trees, binary/source/build identities, profile tradeoffs, startup repetition,
portable archive costs, output/text-quality review, and native Windows runs
remain pending. macOS dry runs cannot select a Windows default or justify a
hardware fallback. Preserve the separate R13/R14 physical holds and owner14 waiver.
