# Issue #1: Windows build baseline verification

Recorded 2026-10-25. This verifies the minimal window and reproducible build,
not terminal/ConPTY functionality or the later MVP performance gates.

## Build and dependency checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass, no warnings |
| `cargo test --locked` | Pass; 0 tests, no business logic or input messages yet |
| `cargo build --release --locked` | Pass on Windows x64 MSVC |
| Clean-checkout release build | Pass; fresh target directory, lockfile unchanged, checkout diff empty |
| `cargo fetch --locked` | Pass; Cargo.lock unchanged |
| Windows feature and duplicate trees | Captured in [dependency audit](../dependencies/README.md) |
| Resolved baseline/version uniqueness assertions | iced 0.14.0, iced_term 0.8.0, Alacritty 0.25.1; no duplicate Iced package names |
| Dependency patch tests | Not applicable; no vendor code or dependency patches |

The pinned toolchain is `1.97.1-x86_64-pc-windows-msvc`:
rustc `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`,
LLVM 22.1.6. The toolchain's rustfmt, Clippy, and MSVC target are installed.
MSVC linking succeeded without a special terminal on this machine.

The clean build used an isolated detached Git worktree containing only the
committed baseline files, with no pre-existing target directory. Running
`cargo build --release --locked` there compiled the complete graph in
2m 01s. Cargo.lock had the same SHA-256 before and after that build, and
`git diff --exit-code` passed. The registry download cache was shared; no
compiled artifacts were reused from the original checkout.

Cargo.lock SHA-256 before and after locked checks:
`a98d44d0af9e39e0f49fffd2c32632ad56d5f152bb004e38c0a4f0d06bbdbd23`.

## Windows release smoke and idle observation

Environment:

- Windows 11 Pro, version 10.0.26200, build 26200.
- AMD Ryzen 5 2600X, 6 physical / 12 logical cores; 17,095,331,840 bytes RAM.
- NVIDIA GeForce RTX 2070, driver 32.0.16.1742; reported active display
  1920 x 1080 at 280 Hz. Monitor scale was not measured.
- High performance Windows power plan; release build, outside a debugger.
- Default renderer selection with both wgpu and tiny-skia compiled; the
  selected graphics API was not instrumented. No renderer override was set
  by the measurement script.
- One static 960 x 640 logical-pixel window, synthetic labels only. No shell,
  child process, PTY, terminal history, application timers, or subscriptions.
  This is **not** a one-terminal memory measurement.

From the repository root, after the build and other Cargo checks finished:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File docs/verification/measure-issue-1.ps1
```

The [measurement script](measure-issue-1.ps1) launches only the built executable
with `Start-Process -WindowStyle Hidden`, waits in bounded 50 ms checks for a
window titled `Pike`, settles for 10 seconds, then samples process counters
at approximately one-second intervals for 60 seconds (61 samples). It sends
a normal window-close request and waits up to 10 seconds for exit. Test
polling runs externally; none is added to the application. The script
overwrites the two raw observation files when re-run.

To preserve those historical files during a review, pass
`-OutputDirectory target/pr-47-review`. The helper saves diagnostics and then
fails if the process exits with a nonzero code or any observation reports
an unresponsive window; those outcomes cannot silently pass the smoke check.

Raw data: [idle CSV](issue-1-idle.csv), [smoke summary](issue-1-smoke.json).

| Observation | Measured result |
| --- | --- |
| Window title detected after launch | 2,598 ms (one run; not first-render or prompt-ready latency) |
| Idle observation interval | 60.653 s |
| Process CPU consumed | 0.109375 s |
| Idle CPU normalized to one core | 0.1803% (`100 * CPU seconds / wall seconds`) |
| Private bytes, min-max | 224,444,416-224,616,448 bytes (214.05-214.21 MiB) |
| Working set, min-max | 207,437,824-207,511,552 bytes (197.83-197.90 MiB) |
| Process threads, min-max | 40-45 |
| Process handles, min-max | 592-597 |
| Windows `Responding` | True in all 61 observations |
| Normal window close to process exit | 229 ms, exit code 0 |
| Release executable size | 10,835,968 bytes |

Tested executable SHA-256:
`0dcd9d59b953935b9350c08211f20550deecb37c914cbb92277380924c71fbfb`.

The initial smoke attempt encountered a console window before the Iced
window. Release builds now select the Windows GUI subsystem, and all Rust
checks and the release smoke were repeated after that change.

## PR review verification

The review found that the original helper recorded bad exit/responsiveness
outcomes without failing. It now rejects both after saving the diagnostics.
Controlled PowerShell process doubles verified a successful observation,
rejection of exit code 7, and rejection of an unresponsive window, including
diagnostic retention and disposal of the process wrapper in all three cases.
The real release window then passed a fresh 61-sample observation and normal
close with exit code 0; review output was saved separately under
`target/pr-47-review`, preserving the original raw measurements above.

Formatting, Clippy with denied warnings, locked tests, and the locked release
build passed again. Both dependency-tree attachments were reproduced and
matched exactly after path normalization. Cargo.lock and the release
executable retain their recorded SHA-256 values. No Rust source or dependency
changes were needed during review.

## Limits and follow-up

The unpruned static-window measurements are above the later proposed
one-session 80 MiB / 0.1%-CPU budgets. This baseline does not satisfy or
redefine those performance gates. Terminal measurements belong to #3;
renderer feature pruning and executor configuration to #4. The process
exited normally; thread/handle counts include runtime, graphics, and OS
overhead and have not been attributed to individual libraries.

This single observation is not a median/p95 latency study, a redraw trace,
a graphics correctness test, or an older-Windows compatibility result.
Resize/DPI, keyboard, shell, PTY teardown, and renderer comparisons remain
unverified because this issue does not implement terminal functionality.
No real terminal data or credentials were collected.

CI is scoped to #2; no CI result is claimed in this PR. The pre-existing
AGENTS.md, PLAN.md, and issue catalogue/publication records were preserved
without edits or inclusion in this implementation commit.
