# Locked dependency audit

This is the historical issue #1 audit. For the current manifest-only vendor
patch, renderer isolation and shared runtime, see the [patch ledger](../../vendor/iced_term/PATCHES.md),
[issue #4 graphs](issue-4/) and [verification](../verification/issue-4.md).

Baseline for issue #1, resolved on 2026-10-25 with Cargo/Rust 1.97.1 for
`x86_64-pc-windows-msvc`. No vendored sources, patches, or additional direct
dependencies are used.

## Requirements and resolution

| Package | Declared requirement | Resolved in Cargo.lock | Declared Rust minimum |
| --- | --- | --- | --- |
| iced | Pike: `=0.14.0`; iced_term: `0.14.0` | 0.14.0 | 1.88 |
| iced_term | Pike: `=0.8.0` | 0.8.0 | Not declared |
| alacritty_terminal | iced_term: `0.25.1` | 0.25.1 | 1.85.0 |
| tokio | iced_term: `1.50.0` with `full` | 1.54.1 | 1.85 |
| winit | Transitive compatible range | 0.30.13 | 1.70.0 |
| wgpu | Transitive compatible range | 27.0.1 | 1.88 |
| ordered-float | Transitive compatible range | 5.5.0 | 1.90 |

A version without `=` is a Cargo compatible range, not an exact pin.
`Cargo.lock` is Pike's authoritative resolved graph, including registry
checksums. A dependency's packaged lockfile does not constrain Pike.

The highest declared `rust_version` in Windows-filtered Cargo metadata is
1.90 (`ordered-float` 5.5.0). Missing declarations do not establish a lower
bound, and metadata alone does not verify an MSRV. Only pinned stable 1.97.1
is tested here; the full graph has not been tested on 1.88 or 1.90. Pike
therefore does not declare a project-wide minimum compiler version yet.

## Iced compatibility and duplicates

There is one version of each Iced package, with **no unexpected duplicate
Iced versions**. Compatible patch releases resolve as follows:

| Packages | Resolved version |
| --- | --- |
| iced, iced_core, iced_debug, iced_futures, iced_graphics, iced_program, iced_renderer, iced_runtime, iced_wgpu | 0.14.0 |
| iced_tiny_skia, iced_winit | 0.14.1 |
| iced_widget | 0.14.2 |
| iced_term | 0.8.0 |

The [complete duplicate tree](windows-duplicates.txt) records parallel
versions of foldhash, font-types, hashbrown, read-fonts, rustc-hash, skrifa,
syn, thiserror/thiserror-impl, and windows-sys. These originate from the
published text/graphics, proc-macro, and Windows dependency chains. They are
retained rather than forcing unsupported transitive upgrades. They do not
introduce a second Iced or terminal implementation.

## Features and resource implications

The [complete feature tree](windows-features.txt) records the actual unified
Windows graph, not only Pike's manifest:

- iced_term requests Iced defaults plus `advanced`, `canvas`, and `lazy`.
- Iced defaults enable `wgpu`, `tiny-skia`, `crisp`, `web-colors`,
  `thread-pool`, `linux-theme-detection`, `x11`, and `wayland`. Target-specific
  dependencies are still filtered by Cargo; feature names do not imply that
  Linux backends run on Windows.
- wgpu defaults include DX12, Vulkan, GLES, Metal, and WebGPU feature names;
  the Windows target determines which implementation code is built.
- iced_term requests Tokio `full`, resolving to 1.54.1. This does **not** enable
  Iced's `tokio` executor feature. Iced currently selects its default thread
  pool (`iced_futures` has `thread-pool`, without `tokio` or `smol`). No app
  subscriptions, timers, terminal sessions, or custom workers are created.

Setting `default-features = false` only in Pike cannot disable defaults
requested by iced_term. The small manifest/executor patch route is scoped
to issue #4, after the terminal compatibility harness in issue #3. Both
renderers remain enabled here; this graph is not evidence of low runtime
memory or a measured renderer choice.

## Reproduce the audit

```powershell
cargo tree --locked --target x86_64-pc-windows-msvc -e features --charset ascii
cargo tree --locked --target x86_64-pc-windows-msvc -d --charset ascii
cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
```

The two text attachments contain the first two commands' output, with the
repository's absolute path replaced by `<repository>`. Re-run the audit when
intentionally changing Cargo.lock; do not infer duplicate versions from
different patch numbers belonging to different Iced package names.

APIs and features were checked against installed registry sources:
`iced-0.14.0/src/application.rs`, `iced-0.14.0/Cargo.toml`,
`iced_futures-0.14.0/src/backend/default.rs`, `iced_term-0.8.0/Cargo.toml`,
and `alacritty_terminal-0.25.1/Cargo.toml`. The entry point follows the
versioned [Iced application API](https://docs.rs/iced/0.14.0/iced/fn.application.html).
