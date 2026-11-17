# R38 resize-readiness correction — acceptance remains incomplete

The later [tiny-skia timing correction](issue-38-skia-timing.md) addresses the
End delays retained below. This record describes the earlier source and keeps
its failed evidence unchanged.

This follow-up starts from main `748675fa32b91644593ab498ecdf53967a138ac7`.
The corrected executable source is `95bad4eb6637346a2edf70d62de540bf18d51247`.
Issue #38 remains open: this fixes a comparison setup race, without selecting
a renderer, release profile or hardware fallback.

## Finding and correction

The fixture previously treated child readiness as layout readiness. A diagnostic
captured the eighth terminal at `prepare_begin` with its initial 80×50 grid,
zero PTY resizes and zero grid resizes. The other seven panes were already
46×24. Preparation could therefore emit its scene and fill history before the
final native resize and reflow. Child readiness alone cannot qualify this
transition.

`LayoutBarrier` records session-scoped native resize acknowledgements. Build
advances only when every attached session has a successful acknowledgement for
its latest resize request and its current grid dimensions. This also prevents
new tabs from being hidden before their first resize. Missing, stale, failed,
wrong-session and wrong-size acknowledgements cannot release the gate. Setup
still uses the existing finite deadline; no polling or production task is added.
The fixture records preparation milestones and pre-command geometry counters.

The retained pre-fix eight-pane empty/idle reproduction reached all eight
`prepared` titles but timed out before `scenario_ready`. Adding further setup
instrumentation produced a passing run and exposed the placeholder-grid race.
That observation establishes the unsafe transition; it does not conclusively
attribute every historical readiness failure to it. Both diagnostics, source
patches and binary identities are retained separately from corrected evidence.

## Verification ran locally

All checks used macOS/aarch64, pinned Rust 1.97.1, locked dependencies,
`CARGO_BUILD_JOBS=4` and the existing local target cache. Both isolated renderers
are relevant because the changed fixture compares them. Checks were scoped to
that fixture; product/vendor code and dependency manifests did not change.

| Command/check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `cargo test --locked --example renderer_comparison` | 15 passed. |
| Same test with `--no-default-features --features renderer-tiny-skia` | 15 passed. |
| `cargo clippy --locked --example renderer_comparison -- -D warnings` | Passed. |
| Same scoped Clippy with tiny-skia feature flags | Passed. |
| `cargo check --locked --target x86_64-pc-windows-msvc --example renderer_comparison` | Passed; compilation only. |
| `python3 -m unittest discover -s tools -p test_renderer_comparison.py -v` | 16 passed. |
| `python3 tools/renderer-comparison.py build --output <fresh-dir> --target-directory <cache> --variants baseline` | Both isolated release app/fixture builds and feature guards passed. |
| `python3 tools/renderer-comparison.py dry-run --manifest <eight-case-manifest> --output <fresh-dir> --keep-going` with native window access | Six valid cases, two invalid; command exits nonzero and preserves failures. |

Existing deprecated `try_next` warnings in unchanged iced_winit and the macOS
`block` future-compatibility warning remain. No vendor test suite was required.
Actions remained disabled; no remote CI or replacement infrastructure was used.

## Native macOS diagnostics

Each case requested three seconds, zero settling plus the existing two-second
arming guard. These use synthetic App focus, native windows and Unix PTYs.
They are workflow checks, not native Windows performance measurements.

| Renderer | Visible / hidden | History / workload / probes | Result; trace interval |
| --- | --- | --- | --- |
| tiny-skia | 8 / 0 | empty / idle / 0 | Valid; 3003 ms. |
| wgpu | 8 / 0 | empty / idle / 0 | Valid; 3006 ms. |
| tiny-skia | 1 / 8 | saturated / idle / 0 | Valid; 3002 ms. |
| wgpu | 1 / 8 | saturated / idle / 0 | Valid; 3012 ms. |
| tiny-skia | 8 / 0 | empty / flood / 0 | Invalid; 4158 ms. |
| wgpu | 8 / 0 | empty / flood / 0 | Valid; 3021 ms. |
| tiny-skia | 4 / 0 | saturated / flood / 2 | Invalid; 4143 ms and only 1/2 readbacks. |
| wgpu | 4 / 0 | saturated / flood / 2 | Valid; 3032 ms, 2/2 validated readbacks. |

All eight cases passed setup; pre-command counters show native resizes for every
session. All 58 announced children were independently observed absent after App
exit; every stderr reader finished. The two invalid cases exited 1 after drained
completion. Tiny-skia's End timer handling delays remain unresolved; the resize
gate is not a timing fix or proof of a production renderer defect. Tolerances,
producer rates, probe counts and interval validity checks remain unchanged.

Raw diagnostics, corrected build/source/compiler/binary identities, feature
trees, manifests, counters, proof strips, cleanup results and logs are under
[`resize-readiness/`](../baselines/issue-38/resize-readiness/). Binaries are excluded.

## Remaining decision

The current wgpu default remains provisional and unchanged. No matched native
Windows private-byte/working-set/CPU/GPU/thread/handle costs, seven-profile
tradeoffs, 20-launch startup comparison or completed text-quality/physical gates
are available. macOS evidence cannot supply those results. Full R38 acceptance
requires an interactive Windows 11 host and the documented
[native procedure](issue-38-windows-run.md), resolution or attribution of timing
failures, and an evidence-backed renderer/profile decision. This PR must not
close #38 or claim that renderer selection is complete.
