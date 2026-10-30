# iced_winit patch ledger

## Provenance

- Published crate: **iced_winit 0.14.1**, all archive files retained verbatim
  except the source delta below. Root Cargo.lock selects the local path at the
  same version; packaged Cargo.lock/Cargo.toml.orig are provenance only.
- crates.io archive SHA-256:
  `7589888c8e951899cc688247a69933bb7a0511f0b4b2e122ac3fcd5dacb37f72`.
- Upstream revision: `38237dd294da3256c2ac4d3f8288fc73956c91e0`, archive path
  `winit`, retained in .cargo_vcs_info.json. Upstream:
  <https://github.com/iced-rs/iced/tree/38237dd294da3256c2ac4d3f8288fc73956c91e0/winit>.
- MIT license is retained in LICENSE from that exact upstream revision; the
  published subcrate archive omits the repository-level license file.
- Root workspace excludes this dependency so optional unused upstream features
  are not resolved as workspace roots. Explicit tests still select it with
  `cargo test --locked -p iced_winit --lib window::ime::tests`.

## R19 / issue #19 — refresh native candidate area after DPI changes

- Upstream enable_ime caches only the logical cursor rectangle and purpose.
  Pinned winit 0.30.13 converts logical coordinates inside its Windows setter;
  WM_DPICHANGED updates scale without reapplying that candidate area. A stationary
  logical cursor could therefore retain its old physical exclusion rectangle.
- Include current native window scale in the cache identity. The same existing
  area/purpose setters run once on scale change; repeated identical requests do
  nothing. Preserve allowed state and the caller's preedit overlay; no disable /
  re-enable, composition cancellation, timer, polling or unsafe API change.
- Move only this existing enable/disable cache branch into window/ime.rs, with a
  private statically dispatched native-setter seam for deterministic tests.
  Production uses winit's original logical setter, retaining one DPI conversion.
  Tests drive the production branch and record setter calls/physical arguments
  through a fake native sink, including 100/125/150/200%, unchanged scale,
  movement, purpose changes and disable/enable. This is not native Windows proof.
- No dependency versions/features or public APIs change. Cargo.lock delta is
  registry-to-path registration only. Root .gitattributes preserves vendor bytes.
- Remove once the pinned upstream runtime refreshes the native candidate area
  when native scale changes without interrupting active composition. See
  [R19 verification](../../docs/verification/issue-19.md) for evidence and limits.
