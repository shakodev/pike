# R38 tiny-skia End timing correction

The repeated second-long End-message delays are fixed in the tested local
scenarios. The separate 30-second/200-readback gate still fails on this Mac.
Issue #38 remains open for that gate and matched native Windows selection data.

## Cause and implementation

A two-second native release profile of the original eight-pane flood recorded
340 main-thread samples. Of those, 314 were in `adjust_clip_mask` beneath
tiny-skia text drawing, including 213 in its full-mask clear. The pinned
tiny-skia canvas implementation assigns infinite local clip bounds to every
text item; each ASCII cell therefore rebuilds a full-window mask. The timer
itself woke on time while the UI thread remained occupied rendering.

ASCII now uses the existing terminal shaped-buffer path and shares the pane
mask. Repeated characters share a buffer within the visible viewport. Logical
buffer extents exceed the pane to preserve ink clipping, including italic
overhang, without allocating a pane-sized bitmap per glyph. Unicode, wide and
combining spans retain their existing cell clips. Bounds join the glyph cache
key so a resize refreshes the clip guards. No dependency, background task,
renderer backend, PTY or terminal input behavior was added.

Two fixture corrections keep these diagnostics explicit and reproducible:

- The offscreen Unicode probe now supplies its synthetic WindowFocus authority.
  Its initial pixel checks passed, but its copy check previously had no native
  window from which to receive Focused.
- Non-Windows `--synthetic-focus true` retains its explicitly simulated authority
  through subsequent native blur observations, which remain logged. Windows
  still rejects that option. A concurrent early headless check stole focus in
  two prototype runs; those runs remain invalid. Final experiments ran
  sequentially. Readbacks now reuse the known window ID and pace inputs from
  send time instead of adding an unconditional 100 ms after every callback.
  Frame gating, exact child/nonce pixel validation, attempt limits, producer
  rate, requested sample counts, interval tolerance and deadlines are unchanged.

## Controlled release evidence

Before source: `982763098a0cf391ea9fbc865426decc611daba8`.
First corrected source: `9114d2a761a0dcb8bd4278961ad8adde9ac154c6`.
Both source manifests report clean tracked trees. Their Rust source hashes
differ only in the three `vendor/iced_term/src/view*` implementation/test files.
The fixture/focus corrections are identical in these builds. Both use the same
baseline release profile, isolated tiny-skia features, machine, geometry, fonts,
children and 2 MiB/s producers. Build order reverses on the second repeat.

| Scenario; two runs each | Before | Corrected |
| --- | --- | --- |
| 8 visible, empty history, flood, no readbacks | Both invalid; 4103–4127 ms | Both valid; 3090–3113 ms |
| End handling lag in that scenario | 1101–1124 ms | 86–112 ms |
| 4 visible, saturated history, flood, two readbacks | Both invalid; 3971–4179 ms; 1/2 readbacks each | Both valid; 3062–3063 ms; 2/2 each |
| End handling lag in that scenario | 968–1175 ms | 61 ms |
| 1 visible, empty history, idle | Both valid | Both valid |

All twelve cases retain their actual results. All 52 announced children were
observed absent afterward; every bounded stderr reader finished. The four
invalid old-renderer cases exited 1 after drained completion. These are native
macOS/App/Unix-PTY observations with synthetic focus, not Windows or physical
keyboard-to-display measurements.

The separate finite external observer samples app cumulative CPU and resident
memory at approximately 250 ms. These are app-only RSS bytes and one-core CPU,
not Windows private bytes, working sets or summed child memory. Measured ranges
across the two corrected runs were:

| Scenario | App CPU, one core | Run-median RSS |
| --- | ---: | ---: |
| 8-pane flood | 126.24–137.06% | 219.16–242.33 MiB |
| 4-pane flood with readbacks | 110.13–112.15% | 252.94–253.23 MiB |
| 1-pane short idle | 2.65–5.65% | 198.70–222.69 MiB |

The three-second, zero-settle idle observations are not a settled idle budget.
The old flood intervals overran, so their resource observations are not an
equal-window cost comparison. No lower-CPU or release-memory claim is made.

## Final source and remaining latency gate

Final executable source: `3cc11b0b97b311c05d449299d293e19f3e212127`.
The renderer code is unchanged from the first correction; this source also
contains the readback dispatch/pacing correction and its regression test.

| Final native scenario | Outcome |
| --- | --- |
| tiny-skia, 8 visible, empty/flood, no probes | Valid; 3083 ms, End lag 83 ms. |
| tiny-skia, 4 visible, saturated/flood, two probes | Valid; 3072 ms, End lag 71 ms, 2/2 exact pixel nonces. |
| tiny-skia, 4 visible/8 hidden, saturated/flood, no probes | Valid; 3079 ms, End lag 76 ms; hidden render counters unchanged. |
| tiny-skia, 1 visible/8 hidden, saturated/idle | Valid; 3007 ms. |
| wgpu, 4 visible, saturated/flood, two probes | Valid; 3020 ms, 2/2 exact pixel nonces. |
| tiny-skia, 4 visible, saturated/flood, 200 probes over 30 seconds | Invalid; 102/200 probes. End interval 30081 ms and lag 78 ms. |

All 41 final announced children were observed absent afterward, with drained
completion and finished stderr readers. The final collector correctly returns
nonzero for the failed long case. Its completed observations have snapshot
median/p95 0.099/0.355 ms and readback median/p95 218.142/225.975 ms; incomplete
coverage prevents a latency-gate pass. These callback timings include software
readback overhead and do not measure physical display presentation.

The first renderer correction completed 69/200 long-run readbacks. A subsequent
dispatch prototype completed 103/200. Both invalid results are retained, with
their exact source/patch identities. The follow-up profile no longer has the
per-cell mask rebuild hot path; rasterization and macOS CoreGraphics/QuartzCore
presentation/color-conversion work now dominate. That observation does not
establish a Windows bottleneck or justify changing its renderer default.

## Verification ran locally

macOS 26.7.3/aarch64, pinned Rust 1.97.1, locked dependencies and the reused
target cache, with `CARGO_BUILD_JOBS=4`. Both renderers are relevant because the
shared terminal text path changed. No full workspace/backend/lifecycle suite
was needed; the patched dependency's affected tests were run explicitly.

| Command/check | Result |
| --- | --- |
| `cargo fmt --all -- --check` and scoped `git diff --check` | Passed. |
| `cargo test --locked -p iced_term --no-default-features --features iced/tiny-skia,iced/tokio --lib view::` | 31 passed. |
| `cargo test --locked -p pike -p iced_term --no-default-features --features pike/renderer-wgpu --lib view::` with native GPU access | 31 vendor tests passed; zero matching app tests. |
| `cargo clippy --locked -p iced_term --no-default-features --features iced/tiny-skia,iced/tokio --lib -- -D warnings` | Passed. |
| `cargo test --locked --no-default-features --features renderer-tiny-skia --example renderer_comparison` | 16 passed. |
| Scoped Clippy on `renderer_comparison` and `unicode_cells` with tiny-skia flags and `-D warnings` | Passed. |
| Windows MSVC `cargo check --locked --example renderer_comparison` on each isolated renderer | Both passed; compile only. |
| Both final baseline release builds and production/fixture feature guards | Passed. |
| Release `unicode_cells <fresh-dir>` on old/new tiny-skia and final wgpu | All passed: two fonts, 100/125/150/200% scales, model/copy/cursor/wide/combining/move/ancestor clips. |
| Byte comparison of old/new tiny-skia PPMs | All 18 images identical; lossless PNG copies and original hashes retained. |
| Sequential controlled/final native experiments | Results above; failed cases preserved. |

The first standalone bare-wgpu vendor command omitted the product's native
backend feature anchor and failed eight renderer-creation tests. Selecting
`pike/renderer-wgpu` supplied the actual graph; all 31 view tests then passed.
Both command logs remain. Existing iced_winit deprecation warnings and the macOS
`block` future-compatibility warning are unchanged. No Actions run was enabled,
dispatched or replaced.

Raw profiles, build/source/compiler/feature/binary identities, collector scripts,
manifests, every valid/invalid run, CPU/RSS observations, cleanup outcomes and
pixel artifacts are in [`skia-timing/`](../baselines/issue-38/skia-timing/).
Binaries are excluded. Native Windows/ConPTY resource costs and physical
keyboard/monitor checks remain unperformed. This fixes the demonstrated long
End stalls; it does not complete the remaining readback gate or select a release
renderer/profile for issue #38.
