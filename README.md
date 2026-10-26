# Pike

A Windows-first terminal multiplexer built with Rust, Iced, and iced_term.
This initial build baseline opens a native window; terminal sessions and
multiplexer controls are not implemented yet.

## Build on Windows

The initial target is `x86_64-pc-windows-msvc`.

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/)
   with **Desktop development with C++**, the MSVC x64/x86 compiler/linker, and
   a Windows 10 or Windows 11 SDK.
2. Install [Rust using rustup](https://www.rust-lang.org/tools/install).
   `rust-toolchain.toml` selects stable **1.97.1**, rustfmt, Clippy, and the
   Windows MSVC target. The first Cargo command may download these components.
3. In PowerShell, from this repository, run:

   ```powershell
   cargo fetch --locked
   cargo fmt --all -- --check
   cargo clippy --locked --all-targets -- -D warnings
   cargo test --locked
   cargo build --release --locked
   cargo run --release --locked
   ```

   The executable is `target/release/pike.exe`. Close its window to exit.
   If linking fails because the MSVC tools or SDK cannot be found, check the
   selected Build Tools workloads and try an x64 Native Tools terminal.

Commit `Cargo.lock` when intentionally updating dependencies. Use `--locked`
for builds, tests, Clippy, and CI so resolution cannot silently change.
The pinned compiler is the verified build baseline, not a claim that the
whole dependency graph supports Iced's minimum Rust version.

[Windows CI](docs/ci.md) runs the same checks on pull requests and `main`,
using the pinned toolchain and a cache keyed by toolchain and lockfile.
That guide also explains how to add explicit regression steps for future
dependency patches.

The [compatibility harness](docs/compatibility.md) opens one or two synthetic
ConPTY sessions for developer checks and release measurements. Its original
unpatched measurements remain in `docs/baselines/issue-3`.

## Dependency baseline

Direct requirements are exactly Iced 0.14.0, iced_term 0.8.0 and Tokio 1.54.1.
Transitive versions and checksums are fixed by
`Cargo.lock`; those dependencies' own manifests still use compatible ranges.
See the [dependency audit](docs/dependencies/README.md) for resolved versions,
compiler requirements, enabled features, and duplicate trees, and the
[Windows verification record](docs/verification/issue-1.md) for measured results.

iced_term is vendored through `[patch.crates-io]`; its [patch ledger](vendor/iced_term/PATCHES.md)
records the exact archive checksum, revision, MIT license, and local patches.
Alacritty 0.25.1 is also vendored for bounded input through its actual Windows
writer, retaining its Apache-2.0 license and [patch ledger](vendor/alacritty_terminal/PATCHES.md).
See [input verification](docs/verification/issue-6.md) for limits, tests and
Windows resource observations. [Shutdown verification](docs/verification/issue-7.md)
documents explicit backend completion and retained final output. Pike disables iced_term's compatibility defaults. Both the app and harness
use one shared Tokio executor with one async worker, at most two blocking
workers, and timers. No Tokio I/O driver or per-pane runtime is needed.
Blocking task admission and session teardown must be bounded by their callers;
the executor's worker limit does not bound queued tasks.

The default build uses wgpu only for comparison with issue #3. Build the
software renderer separately (the final renderer choice belongs to issue #38):

```powershell
cargo build --release --locked --no-default-features --features renderer-tiny-skia
cargo build --release --locked --examples --no-default-features --features renderer-tiny-skia
powershell -NoProfile -File tools/test-feature-profiles.ps1
```

Use exactly one renderer feature per comparison. `--all-features` intentionally
combines both renderers, and vendor defaults opt back into upstream features;
neither represents a lean profile. CI verifies each renderer and explicitly
runs the vendored crate's tests. See [issue #4 verification](docs/verification/issue-4.md)
for feature graphs and Windows resource measurements.
