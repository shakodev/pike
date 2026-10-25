# iced_term patch ledger

## Provenance

- Published crate: iced_term **0.8.0**, extracted from the crates.io archive.
- Archive SHA-256: `e1e287970417db55e0b395a966f061f10c7d3b03fd146d0b8806678d8c063103`.
- Source revision: `e0e90be2c160bd68f1bf544a51bf772d75b98ffc`, retained in
  `.cargo_vcs_info.json`; matches Pike's planning provenance.
- Upstream: <https://github.com/kemokempo/iced_term/tree/0.8.0> (published
  manifest records the repository's earlier `Harzu/iced_term` name).
- License: MIT, retained verbatim in [LICENSE](LICENSE).
- All archive files are retained, including `Cargo.toml.orig` and the upstream
  `Cargo.lock`. Only the normalized `Cargo.toml` differs, plus this ledger.
  Pike's root `.gitattributes` disables line-ending conversion for this directory
  so Windows Git checkouts preserve archive bytes.
  The archive lockfile is provenance only: workspace builds/tests use Pike's
  root `Cargo.lock`. Do not resolve vendor tests as a separate workspace.

## R04 / issue #4 — configurable dependency features

Only `Cargo.toml` changes. Terminal, event, input, rendering, PTY and lifecycle
source code is byte-for-byte the published version.

- Iced: disable dependency defaults; preserve required `canvas`, `lazy`,
  `advanced`. Feature `iced-defaults` forwards `iced/default`.
- Tokio: disable dependency defaults and replace `full` with the only feature
  used by this crate, `sync`. Feature `tokio-full` forwards `tokio/full`.
- Both compatibility features are enabled by default, preserving upstream
  dependency behavior for consumers that use defaults. Pike opts out and
  selects the renderer/runtime centrally. Alacritty remains unchanged.

Explicit vendor regression commands (25 published unit tests per profile):

```powershell
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
powershell -NoProfile -File tools/test-feature-profiles.ps1
```

No upstream fixes are included in this patch. Later patches must record their
source diff, reason, regression tests and any changed ownership/resource limits.
