# Issue #4: dependency patch and shared runtime

Recorded 2026-10-25 on Windows 11 Pro build 26200, x64 MSVC. Prerequisite #3
is merged. Existing untracked planning files are preserved separately.

## Changes and provenance

`vendor/iced_term` contains the exact published 0.8.0 archive, including MIT
license, source revision and original manifests/lockfile. Its archive SHA-256
is `e1e287970417db55e0b395a966f061f10c7d3b03fd146d0b8806678d8c063103`,
matching the planning provenance and original root lockfile checksum. Hash
comparison against the installed archive contents found only `Cargo.toml`
changed, plus the added [patch ledger](../../vendor/iced_term/PATCHES.md).
All terminal/PTY source files are unchanged. Git attributes preserve vendor
bytes on Windows; the upstream workflow's existing trailing blanks are retained.

The manifest makes Iced defaults and Tokio `full` optional compatibility
features. Pike disables them through `[patch.crates-io]`, selects one renderer,
and directly pins the already-locked Tokio 1.54.1. No package versions changed.
The root lockfile now resolves iced_term locally and records Iced/Pike's Tokio
dependency edges. The original vendor lockfile is provenance only; explicit
vendor tests use the root workspace lockfile.

Both application entry points select one `SharedExecutor`, implementing the
installed Iced 0.14 Executor contract. It delegates enter/spawn/block_on to
one Tokio runtime: one async worker, at most two blocking workers, timer driver,
no Tokio I/O driver. Harness delays use one-shot runtime timers in place of
temporary sleep threads. Idle mode still has no periodic timer.

The cap limits workers, not the pending Tokio task queue. No production
blocking jobs are introduced. Future spawn/cleanup code must bound admission
before submitting work. Runtime drop cancels yielding async futures and joins
finite blocking work; it cannot interrupt a running blocking operation.
PTY shutdown/draining/acknowledgement remains #7, and bounded asynchronous
session startup remains #10.

## Feature and automated checks

[Saved Cargo trees](../dependencies/issue-4/) include the actual unified
Windows features and resolved package features for each profile. The feature
guard runs in both CI matrix entries:

- Iced: `advanced,canvas,crisp,lazy,tokio` plus only the chosen renderer
  (`wgpu` also uses `wgpu-bare`). No Iced default/thread-pool, Linux platform
  features or web-colors are re-enabled.
- iced_futures: `tokio`, without `thread-pool`.
- Tokio: `default,rt,rt-multi-thread,sync,time`; its default feature is empty.
  No `full`, filesystem, I/O, networking, process, signal or macro features.
- iced_term: no compatibility defaults. Tiny-skia has no wgpu package;
  wgpu has no iced_tiny_skia package. wgpu's own default backend feature
  selection remains unchanged; further GPU backend pruning belongs to #38.

Passed locally for **each** renderer:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --no-default-features --features renderer-RENDERER -- -D warnings
cargo test --locked --no-default-features --features renderer-RENDERER
cargo test --locked --examples --no-default-features --features renderer-RENDERER
cargo test --locked -p iced_term --lib --no-default-features --features iced/RENDERER,iced/tokio
cargo build --release --locked --bins --examples --no-default-features --features renderer-RENDERER
```

Replace `RENDERER` with `wgpu` or `tiny-skia`. The requested default-profile
fmt/Clippy/test commands also passed. Each profile ran four executor tests,
four harness tests, and **25 vendor tests explicitly** (none ignored).
Executor tests cover shared runtime identity, enter context restoration,
timers/spawn/block_on, task cancellation, queuing beyond two blocking workers,
and async cancellation/finite blocking joins during runtime shutdown.
The feature guard, measurement-support tests and actionlint also passed.
The first standalone vendor test attempt omitted `iced/tokio`; Iced rejected
the missing executor feature. The corrected commands above and CI include it.

## Windows scenarios and measurement method

Both release renderer builds ran the two-session synthetic smoke sequence
under cmd and Windows PowerShell: same-sized pane swap, screenshots, changed
positive cell dimensions (58x29 to 67x32), session 2 exit 42 while session 1
is focused, then session 1 exit 41, application exit 0, and no surviving fixture
PIDs. The timers therefore execute through the shared runtime in a real Iced
event loop. These are scenario checks, not an input-to-render latency test.

[Machine metadata](../baselines/issue-4/machine.json) records hardware, power
plan, source and binary hashes/sizes. Hardware matches issue #3: Ryzen 5 2600X,
12 logical cores, 16 GiB class RAM, RTX 2070 driver 32.0.16.1742, 1920x1080 at
280 Hz, High performance plan. Screenshot scale is 1.0. Compiler/Cargo are
1.97.1. Each profile has a separate preserved executable and adjacent fixture.
Only small synthetic startup history is present; the unchanged 10,000-line
history is not saturated. No real terminal contents or credentials are logged.

Idle runs are sequential outside a debugger: ten seconds settling, 61 external
samples over at least 60 seconds, process CPU normalized to one core. Child
private bytes/CPU are separate in raw data; working sets are not summed.
Startup uses 20 cmd/one-session runs per profile, ending at fixture/window
readiness with 20 ms polling, not first presented pixel or interactive shell
prompt. Median averages the two middle samples; p95 is nearest rank 19/20.
Raw traces, counters, summaries and synthetic screenshots are in
[baselines/issue-4](../baselines/issue-4/). Reproduce with the commands in the
[harness guide](../compatibility.md), passing `-Renderer` and `-Executable`.

### Release observations

Each idle row is one independent 61-sample run. Private bytes and working set
are MiB min-max, and CPU is percent of one core over the entire interval.
The historical #3 rows are reproduced for comparison, not rerun controls.

| Profile | Shell / sessions | CPU | Private MiB | Working set MiB | Threads |
| --- | --- | ---: | ---: | ---: | ---: |
| #3 unpatched wgpu | cmd / 1 | 11.95% | 211.64-212.10 | 165.87-166.74 | 44-49 |
| #4 wgpu | cmd / 1 | 11.91% | 211.18-211.56 | 165.09-165.77 | 33-38 |
| #4 tiny-skia | cmd / 1 | 26.40% | 13.32-13.45 | 24.23-25.02 | 9-13 |
| #3 unpatched wgpu | cmd / 2 | 18.49% | 219.02-219.18 | 168.38-169.55 | 47-52 |
| #4 wgpu | cmd / 2 | 13.19% | 216.84-217.18 | 167.05-168.70 | 36-41 |
| #4 tiny-skia | cmd / 2 | 45.20% | 19.16-19.27 | 26.09-27.64 | 12-16 |
| #3 unpatched wgpu | PowerShell / 1 | 9.39% | 213.48-213.64 | 165.20-165.95 | 44-49 |
| #4 wgpu | PowerShell / 1 | 15.45% | 213.11-213.30 | 165.75-166.56 | 33-38 |
| #4 tiny-skia | PowerShell / 1 | 39.05% | 19.39-23.80 | 30.27-34.80 | 9-13 |
| #3 unpatched wgpu | PowerShell / 2 | 11.33% | 217.96-218.56 | 167.41-168.92 | 47-52 |
| #4 wgpu | PowerShell / 2 | 15.71% | 216.81-216.95 | 167.61-169.14 | 36-41 |
| #4 tiny-skia | PowerShell / 2 | 56.50% | 27.09-27.21 | 34.30-35.70 | 12-16 |

All 488 idle responsiveness observations passed. All runs exited normally
with the expected originating child codes and no fixture PID remaining.
The wgpu configuration used 11 fewer process threads at both ends of the
observed ranges, with little change in private memory. CPU varied upward
and downward by shell/scenario; these single runs cannot establish causality
or a general improvement. Tiny-skia used far less memory but substantially
more CPU with the unchanged terminal code. All scenarios miss the idle CPU
budget. Wakeup counts remain high; no continuous-redraw fix is claimed.

| cmd / one-session startup | Runs | Median ms | p95 ms |
| --- | ---: | ---: | ---: |
| #3 unpatched wgpu | 20 | 1016.35 | 1109.35 |
| #4 wgpu | 20 | 1045.52 | 1331.54 |
| #4 tiny-skia | 20 | 155.74 | 193.49 |

Release harness binaries are 12,692,992 bytes (wgpu) and 6,304,256 bytes
(tiny-skia), versus 13,013,504 bytes in #3. App binaries are 10,566,656 and
4,280,832 bytes respectively. These are executable sizes, not compressed
distribution measurements. Separate [app window-close checks](../baselines/issue-4/app-smoke.json)
exited 0 with observed close intervals of 215.42 ms (wgpu) and 12.63 ms
(tiny-skia); one run each, without terminal sessions, not a shutdown p95.

## Remaining constraints

### PR review follow-up

The review found a feature-guard gap: later Cargo tree entries could overwrite
features on an earlier version of the same package. A synthetic tree containing
an older Tokio with `full` followed by a clean newer version incorrectly passed
before the fix. The guard now unions features across every version/entry and
requests uncolored Cargo output. Four regression cases cover clean profiles,
earlier Tokio/Iced features, and renderer dependency leakage; CI runs them.
The collector also normalizes case-insensitive renderer arguments before
forwarding them to Iced and recording metadata, matching shell/mode handling.
These changes affect tooling only; application binaries and the resource
measurements above remain unchanged.

### Unpatched terminal behavior

Final title markers remain intermittent, as in #3: wgpu cmd smoke saw both;
the other three smoke runs saw neither. Terminal behavior is unpatched, and
these markers are not an oracle for exact final output. No output-drain fix
or joined PTY cleanup is claimed.

High idle CPU/wakeup rates, full-grid history copies, widget/input/Unicode
defects and other source findings from #3 remain for their focused issues.
This infrastructure does not establish the MVP resource budgets or choose the
final renderer. Older Windows/ConPTY versions, other DPI/GPU configurations,
PowerShell 7 (absent), flood/history/paste stress and input latency are untested
here. The current shared runtime has no caller that submits blocking session
startup/teardown jobs; future code must enforce admission/cancellation bounds.
