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
for builds, tests, Clippy, and future CI so resolution cannot silently change.
The pinned compiler is the verified build baseline, not a claim that the
whole dependency graph supports Iced's minimum Rust version.

## Dependency baseline

Direct requirements are exactly `iced = "=0.14.0"` and
`iced_term = "=0.8.0"`. Transitive versions and checksums are fixed by
`Cargo.lock`; those dependencies' own manifests still use compatible ranges.
See the [dependency audit](docs/dependencies/README.md) for resolved versions,
compiler requirements, enabled features, and duplicate trees, and the
[Windows verification record](docs/verification/issue-1.md) for measured results.

This baseline keeps the published dependencies unchanged. iced_term enables
Iced's default renderers and Tokio's `full` feature. Feature pruning and the
shared executor belong to [issue #4](https://github.com/shakodev/pike/issues/4);
the current build is not the final lightweight configuration.
