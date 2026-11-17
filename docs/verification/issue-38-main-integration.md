# PR #84 integration review — 2026-11-17

Reviewed PR source `4d409313bdb31fffd5d27b4e09c450e946c69d5a` and integrated
main `b78e92232c8aad3b92510405d7f9eaf56853a388` (R14 and R34).
Compiled source is `998da23de21ed897375edceba8cb4dbcd63d5cc8`; subsequent
review artifacts/documentation do not change that executable source.

The owner's current review-and-merge request authorizes merging the comparison
tooling. This does not complete issue #38, approve a renderer/profile choice or
establish a Windows runtime result. No additional confirmed implementation
finding remains in this bounded review. Workflow validity still has the concrete
failed cases below; keep R38 open and the product default unchanged.

## Source review and integration fixes

Reviewed the actual-App fixture, bounded child protocol, sample retirement,
child-specific pixel nonce decoder, interval/overlap validity, source/build
identities, native process/cost validation, cohort/cell-geometry comparisons,
collector cleanup and finite bounds. Existing corrected cancellation, nonce,
raw cost, immutable identity and geometry regressions remain intact.

- The fixture now calls App::boot_with_config_path, using R34's real renderer
  readiness/failure path. It records readiness.txt with that constructor identity;
  successful scenario setup therefore follows the product readiness gate.
- The comparison feature validator previously rejected current main's actual
  wgpu-bare graph. It now requires the native bare profile, DX12/Vulkan/WGSL and
  rejects the old iced/wgpu defaults, GLES/WebGL and glow/khronos-egl leakage.
  Both production and fixture Windows graphs pass; regression mutations fail.
- Direct fixture validation previously returned a NaN interval for a NaN event
  timestamp (later strict JSON serialization also rejects nonfinite output).
  Event timestamps now fail validation immediately if nonfinite or negative,
  including readiness, setup and measurement events. Added regression cases.
- The Windows procedure now records the completed R34 integration. Earlier
  reports and raw failures remain unchanged as historical evidence.

`git diff --quiet origin/main -- Cargo.toml Cargo.lock src vendor .github`
passes: the tooling merge changes no product code, dependency/features/defaults,
shared runtime, PTY implementation or Actions workflow. No patched dependency
suite was rerun because this PR changes no vendor code.

## Verification ran locally

All verification ran on aarch64 macOS using Rust 1.97.1, locked dependencies
and CARGO_BUILD_JOBS=4. Both renderer profiles are relevant here because this is
renderer-comparison tooling; checks were scoped to its example and scripts.
No full-workspace build/test checklist, seven-profile experiment or full native
matrix was run. Actions was not enabled, dispatched, rerun or replaced.

| Command/check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `git diff --check origin/main -- . ':!docs/baselines'` | Passed. Historical raw CSVs retain their CRLF bytes and logs retain terminal blank lines. |
| `python3 -m unittest discover -s tools -p test_renderer_comparison.py -v` | 16 passed, including native feature-profile and finite-timestamp regressions. Original 14-case suite also passed before the additions. |
| `python3 docs/baselines/issue-38/strict-validation/run-independent-validation.py --output <fresh-dir>` | 7 unchanged independent aggregation control/rejection cases passed through their retained schema-only adapter. |
| `python3 docs/baselines/issue-38/comparison-axes/run-independent-axes.py --output <fresh-dir>` | 3 unchanged comparison-axis cases passed through the retained seed/output-path adapter. |
| `cargo test --locked --no-default-features --features renderer-tiny-skia --example renderer_comparison` | 14 passed after integration. |
| `cargo test --locked --example renderer_comparison` | 14 passed with the default wgpu profile. |
| Matching per-renderer `cargo clippy --locked [profile flags] --example renderer_comparison -- -D warnings` | Both passed. Existing unchanged winit try_next dependency warnings remain. |
| `cargo check --locked --target x86_64-pc-windows-msvc --example renderer_comparison` | Passed; cross-compilation only. |
| `cargo tree --locked --target x86_64-pc-windows-msvc --no-default-features --features renderer-<profile> -e <normal,build or normal,build,dev> --prefix none --format '{p}|{f}'` through feature_guard | Both production/fixture graphs passed for both renderers. |
| `python3 tools/renderer-comparison.py build --output <fresh-dir> --target-directory <reused-target> --variants baseline` | Both isolated release app/fixture builds, feature checks and executable-only ZIPs passed; source.json records tracked_dirty=false. |
| `python3 tools/renderer-comparison.py dry-run --manifest <eight-case-manifest> --output <fresh-dir> --keep-going` with required macOS window access | Completed once: six valid cases and two invalid cases. The command correctly exits nonzero for the invalid matrix. All raw cases retained; no failed case was rerun or relaxed. |

The selected Rust, Python and unchanged independent suites total 54 passing
unit/regression cases. They do not turn the two native workflow reds into passes.
Commands, source/compiler/build identities, logs and raw smoke results are in
[merge-review](../baselines/issue-38/merge-review/).

## Eight finite native workflow checks

These are short synthetic-focus App/native-window/Unix-PTY integration checks,
not Windows costs, physical input/display observations or a renderer comparison
sufficient for selection. Each requested a three-second measurement interval,
zero settle delay plus the existing two-second arming guard. Builds were run
sequentially; the baseline compile work completed before native measurements.

| Case | Renderer | Visible / hidden | History / workload / probes | Outcome |
| --- | --- | --- | --- | --- |
| 0 | tiny-skia | 1 / 0 | empty / idle / 0 | Valid, 3001.938 ms. |
| 1 | wgpu | 1 / 0 | empty / idle / 0 | Valid, 3001.449 ms. |
| 2 | wgpu | 4 / 0 | saturated / flood / 2 | Valid, 3010.424 ms; both child-specific pixel nonces validated. |
| 3 | tiny-skia | 4 / 0 | saturated / flood / 2 | Invalid: late interval/producer coverage and only 1/2 completed readbacks. |
| 4 | tiny-skia | 4 / 8 | saturated / idle / 0 | Valid, 3003.969 ms; hidden rendering counters stay bounded/unchanged. |
| 5 | wgpu | 4 / 8 | saturated / idle / 0 | Valid, 3002.574 ms; hidden rendering counters stay bounded/unchanged. |
| 6 | wgpu | 8 / 0 | empty / flood / 0 | Valid, 3029.848 ms. |
| 7 | tiny-skia | 8 / 0 | empty / flood / 0 | Invalid: eight children became ready, but no scene-ready/measurement milestone arrived before the external deadline. |

Case 3's requested 3000 ms interval lasted 3922.320 ms. Its End timer armed at
0.038 ms, woke at 3001.352 ms and was handled 920.941 ms later. One callback
contained the valid nonce; the second sample did not complete before End.
Case 7 was externally terminated after 43.068 seconds, before measurement.
These observations do not attribute cause to production, fixture, host or
renderer. Tolerances, byte rates, sample counts, pixel checks and deadlines were
not changed. No reliable full comparison or tiny-skia performance conclusion
is claimed.

All 50 announced child instances were independently checked absent afterward;
every bounded stderr reader finished. Six runs exited successfully, case 3
exited 1 after cooperative child completion, and the deadline case exited on
the collector's SIGTERM (-15). Cleanup does not make invalid timing evidence
valid. The original 96-case matrix's 30 invalid cases and its two invalid
instrumented cases remain retained and unresolved; these greens do not explain
or erase those historical failures.

## Artifacts and remaining gates

Measured executable artifact sizes for the baseline-only local builds:

| Renderer | App bytes | Executable-only ZIP bytes |
| --- | ---: | ---: |
| tiny-skia | 9807240 | 3261508 |
| wgpu | 15543896 | 4975387 |

The native dry-run collector explicitly records Windows private bytes, working
set, one-core CPU and GPU counters as unavailable. No release idle memory/CPU
budget or final profile tradeoff is inferred from artifact size or these short
workflow timings. Only one release profile per renderer was built in this
review; the seven-profile/20-startup/full-cost experiment remains pending.

Native Windows/ConPTY/PowerShell collector and GPU watcher execution remain
unperformed. Physical keyboard/monitor/Narrator, terminal text-quality and
presentation-latency gates remain open. R34's Windows acceptance remains tracked
separately in issue #34. Renderer/profile selection, any hardware fallback and
full R38 acceptance require matched valid native evidence and decision review.
The merge leaves issue #38 and the existing physical #13/#14 follow-up open.
