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

## R34 / issue #34 — private actionable clipboard results

- Add `Clipboard::try_write` around the existing window_clipboard implementation,
  returning only pinned iced_runtime's WriteError codes. The legacy void writer
  delegates to it; raw platform errors are dropped instead of repeatedly logged.
- Route the additive WriteWithResult action through the same concrete writer and
  send the real result through its existing task channel. No clipboard readback,
  second backend, iced_core trait change, worker or periodic task.
- Extract only the existing clipboard action dispatch for deterministic tests.
  Explicit tests cover the concrete unconnected host's acknowledged failure and
  injected native failure mapping, including secret-bearing error strings.
- Remove with upstream equivalent safe acknowledged clipboard writes; see the
  [runtime patch](../iced_runtime/PATCHES.md) for exact source provenance.

## R34 / issue #34 — finite existing compositor recovery and safe cleanup

- Replace OOM panic and unbounded per-frame retry/logging with three transient
  retries per window. Successful presentation or an externally changed surface
  generation resets the counter. Lost/Outdated still use the original surface
  recreation/reconfiguration path; no renderer fallback is added.
- On exhaustion, pause presentation and send one typed failure through the
  registered runtime sink. Continue ordinary lifecycle delivery until the App's
  original pool shutdown completes; then its existing Exit returns a fixed
  graphics error. With no sink, return the safe existing Result immediately;
  Pike's production boot gate ensures no backend exists before registration.
- Map event-loop creation/run errors into that safe Result instead of expecting
  or discarding them. Clipboard and renderer metadata never include raw driver,
  platform, clipboard, terminal, argument, environment or title strings.
- Add off-by-default `recoverable-error-fixtures` with a maximum-four finite
  SurfaceError queue at the actual compositor result. Only the isolated local
  native fixture manifest enables it; normal Pike builds do not compile a fault
  injection API. No environment trigger, alternate compositor, PTY or timer.
- Explicit tests cover the budget, surface/success reset, OOM, ordered readiness
  plus one failure, clipboard conversion, and adjacent candidate-area behavior.
  Actual native-window controlled-child evidence is retained in issue-34.
- Partial checkpoint: the proposed stalled-renderer native timeout dialog and
  broader native resize error propagation were NOT implemented at owner stop.
- Remove when the pinned upstream host offers equivalent bounded recovery,
  metadata delivery and cleanup ordering.
