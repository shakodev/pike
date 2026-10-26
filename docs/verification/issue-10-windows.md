# Issue #10 — native Windows completion

The implementation merged in PR #57. Issue #10 was reopened because its macOS
checks did not execute Windows ConPTY or measure the native UI. Those checks
were completed locally on 2026-10-26, without GitHub Actions.

The starting revision is `dc50610`. This follow-up adds an opt-in GUI probe and
collector, measurements, and documentation. Production startup behavior is
unchanged. `start_with` becomes crate-visible so the standalone probe can
compile the exact pool source with a deterministic preparer; it is not a public
library API. The test-module path is explicit for both compilation contexts.
No dependency, feature, lockfile, renderer-default or vendor changes are included.

## Acceptance evidence

- Native application tests pass for both renderers: 18 library tests and two
  application tests. They exercise queued cancellation, slow success/failure,
  worker panic, late success after target/window closure, canceled attachment,
  fresh IDs on retry, stale completion routing, and disposal before capacity
  reuse. The ordinary suite leaves the opt-in timing probe ignored.
- The GUI probe runs three 32-request bursts per scenario and renderer, with a
  two-job/two-session limit. Every burst admits exactly two and rejects 30
  before creating extra tasks. A worker assertion checks that preparation runs
  on a different thread from Iced update.
- Both admitted workers enter a gated preparer. The UI cancels the targets, or
  stops pool admission for window closure, after 250 ms. It releases the gates
  after 500 ms. Successful workers then create **real** ConPTY backends and run
  the probe executable as a synthetic stdin-waiting child. This guarantees
  successful native creation after cancellation, independent of OS timing.
- Both results must be Canceled and both backend handles must acknowledge
  Requested completion. Failure uses a synthetic error and creates no PTY.
  The collector separately verifies every recorded fixture PID has exited.
  Across both renderers, all 24 late native backends completed with no surviving
  fixtures; all 18 bursts retained the two-admitted/thirty-rejected bound.
- The actual Iced event loop receives a 5 ms heartbeat during each burst and
  redraws its changing counter. Starting/recovered screenshots were inspected.
  The timer subscription stops during recovery sampling. It exists only in this
  opt-in probe, not in the application or LaunchPool.
- Explicit native iced_term tests pass: 48 per renderer, including preparation
  on a worker, UI attachment, disposal without attachment, real ConPTY flood,
  final output and cancellation-safe shutdown. The original dispatch surrogate
  also runs natively in release mode; its timer is not described as a GUI test.

## Measurements

Windows 11 Pro build 26200, x64 MSVC, Rust/Cargo 1.97.1, Ryzen 5 2600X
(12 logical cores), 16 GiB class RAM and RTX 2070. The pinned
Microsoft.Windows.Console.ConPTY 1.25.260930003 runtime is staged beside the
executables. [Provenance](../baselines/issue-10/windows/provenance.json) records
machine/compiler/runtime information and source hashes. Raw events, timing,
process samples, summaries, synthetic PIDs and lossless PNG screenshots are in
[the Windows evidence directory](../baselines/issue-10/windows/).

Each row summarizes three bursts in one process; the values are maxima across
those bursts, not percentiles. UI gap is the time between heartbeat handling
on Iced's update thread, including timer delivery, scheduling and screenshot
overhead. It is not keyboard-to-render latency or a frame-time measurement.
Recovery runs from releasing the delayed workers through acknowledged disposal
and consumption of both results; the injected pre-release wait is excluded.

| Renderer | Scenario | Max admission µs | Max UI gap ms | Max recovery ms | Sampled max private MiB | Process CPU seconds |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| tiny-skia | Target close | 23.700 | 17.221 | 64.293 | 17.17 | 0.218750 |
| tiny-skia | Window close | 23.100 | 17.081 | 52.491 | 16.77 | 0.265625 |
| tiny-skia | Failure | 60.600 | 16.714 | 0.951 | 4.53 | 0.171875 |
| wgpu | Target close | 20.700 | 8.090 | 62.406 | 237.34 | 1.687500 |
| wgpu | Window close | 15.100 | 6.545 | 55.831 | 238.31 | 1.546875 |
| wgpu | Failure | 10.300 | 7.766 | 1.117 | 230.32 | 1.531250 |

Process CPU includes startup, the three active GUI bursts, recovery and exit,
over 5.64–6.33 seconds. It is not an idle percentage. The external sampler
requests 20 ms intervals; Windows scheduling and process-property reads lengthen
them, so transient peaks may be missed. All external Responding samples are true.

Native success initially retains one additional thread and eight handles with
tiny-skia, or one thread and ten handles with wgpu. This is reported rather than
treated as exact baseline recovery. After the first burst, subsequent recovered
handle/thread counts stay constant or decrease:

| Renderer / scenario | Recovered handles, cycles 0/1/2 | Recovered threads, cycles 0/1/2 |
| --- | --- | --- |
| tiny-skia / target close | 219 / 219 / 219 | 12 / 12 / 12 |
| tiny-skia / window close | 220 / 220 / 220 | 12 / 12 / 12 |
| tiny-skia / failure | 212 / 212 / 212 | 11 / 11 / 11 |
| wgpu / target close | 626 / 626 / 605 | 37 / 37 / 35 |
| wgpu / window close | 627 / 627 / 627 | 37 / 37 / 37 |
| wgpu / failure | 617 / 617 / 617 | 36 / 36 / 36 |

Private memory fluctuates after cleanup; full before/after values are retained
per cycle in each summary. These short runs establish acknowledged shutdown,
bounded admission and observed recovery, not an unlimited-soak leak guarantee.

The existing compatibility harness also ran one synthetic cmd session with each
renderer, sequentially outside a debugger. After ten seconds of settling it
collected 61 external samples over more than 60 seconds, then sent a native
window-close request and verified final output, child exit 41, acknowledged
cleanup and no surviving fixture. All window-responsiveness samples were true.
Child CPU measured 0.00% in both runs and its memory is recorded separately.

| Renderer | Idle seconds | CPU % of one core | Private MiB min–max | Working set MiB min–max | Threads min–max |
| --- | ---: | ---: | ---: | ---: | ---: |
| tiny-skia | 61.30 | 0.00 | 12.43–12.54 | 23.13–23.18 | 9–13 |
| wgpu | 61.25 | 0.03 | 210.28–210.44 | 163.87–163.95 | 33–38 |

CPU is based on process CPU-counter deltas, without dividing by core count.
A zero means the counter did not advance during the sample interval. The
software profile meets the one-session 0.1%-CPU/80-MiB target in this observation;
the default wgpu profile exceeds the memory target. Renderer selection and that
memory difference are not changed by this startup-verification follow-up.

## Reproduction

Run from a Windows checkout. Use a fresh output directory for every measurement.
The collector refuses to overwrite prior evidence. The parent records resource
counters and bounded stderr; it does not collect terminal transcripts, shell
profiles, command lines or environment values. The only child is the synthetic
probe executable. PPM screenshots can be converted losslessly to PNG for review.

```powershell
powershell -NoProfile -File tools/prepare-conpty.ps1
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo build --release --locked --bins --examples
powershell -NoProfile -File tools/measure-startup-lifecycle.ps1 -OutputDirectory target/startup-wgpu
cargo test --release --locked --lib measure_32_requested_launches -- --ignored --nocapture
powershell -NoProfile -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 1 -Mode idle -Renderer wgpu -OutputDirectory target/startup-wgpu-idle

cargo clippy --locked --all-targets --no-default-features --features renderer-tiny-skia -- -D warnings
cargo test --locked --no-default-features --features renderer-tiny-skia
cargo test --locked --examples --no-default-features --features renderer-tiny-skia
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
cargo build --release --locked --bins --examples --no-default-features --features renderer-tiny-skia
powershell -NoProfile -File tools/measure-startup-lifecycle.ps1 -OutputDirectory target/startup-tiny-skia
powershell -NoProfile -File tools/measure-compatibility.ps1 -Shell cmd -Sessions 1 -Mode idle -Renderer tiny-skia -OutputDirectory target/startup-tiny-skia-idle
```

Build and measure profiles sequentially: Cargo overwrites executable paths.
The recorded builds used the root repository's shared target directory; the
collector's `-Executable` parameter selects that location. Process-tree queries
for the existing idle collector required execution outside the workspace sandbox.
No workflow was edited or dispatched. The completion commit uses `[skip ci]`.

These results cover one Windows machine, synthetic launch settings and a maximum
of two admitted sessions per burst. Other shells/profiles, DPI combinations,
typing/copy/paste latency and long-running saturation remain separate scenarios.
