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
  `Cargo.lock`. Changes to the normalized manifest and source are listed below.
  Pike's root `.gitattributes` disables line-ending conversion for this directory
  so Windows Git checkouts preserve archive bytes.
  The archive lockfile is provenance only: workspace builds/tests use Pike's
  root `Cargo.lock`. Do not resolve vendor tests as a separate workspace.

## R04 / issue #4 — configurable dependency features

This patch changed only `Cargo.toml`. Subsequent source patches are listed below.

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

## R05 / issue #5 — bounded event delivery

- Add `src/event_bridge.rs`: 256 FIFO control slots, a 64 KiB aggregate
  retained String-capacity budget, a 4 KiB UTF-8 title slot, and coalesced
  redraw/mouse/cursor/bell state. Protocol requests retain FIFO order.
- Reserve separate sticky error, child-exit, Exit and source-completion state.
  On overload, reject subsequent ordinary events and report a redacted,
  session-scoped error. Processing that error requests only that PTY's shutdown
  and disables its writes/resizes. Lifecycle events remain deliverable.
- `backend.rs`: replace `blocking_send` with bounded state mutation and a
  Tokio Notify wake. The brief bridge mutex never covers terminal operations,
  I/O or awaits, and no producer waits for UI queue capacity. The event-loop
  proxy has a completion-on-drop marker; clones retained by Term do not.
- `terminal.rs`: poll the bridge directly with a fused `unfold` stream; remove
  both the 100-event input queue and the 1,000-event forwarding queue. EOF and
  subscription cancellation end without a panic, forwarding task or busy loop.
- Export `EventBridgeError` / `EventBridgeEvent` through `BackendCommand::EventBridge`
  and return `Action::EventBridgeError` when handled. Source completion is not
  joined/drained PTY shutdown acknowledgement (#7). Input admission is #6.
- Reviewed [upstream PR 69](https://github.com/kemokempo/iced_term/pull/69/files),
  commit `338e59f`: its `try_send` substitution still discards full-queue events.
  This implementation keeps lifecycle state outside the bounded control FIFO.
- No dependency or lockfile changes. Fifteen new regression tests (40 total)
  cover overload/coalescing/payload accounting, protocol order, lock safety,
  cancellation/EOF, session routing, and real Windows ConPTY flood/resize/close.

Run both explicit vendor commands above; see
[issue #5 verification](../../docs/verification/issue-5.md) for measurements
and API/overload policy details.

## R06 / issue #6 — fallible byte admission

- Every keyboard/paste/protocol/mouse/alternate-scroll write uses the patched
  Alacritty sender's fallible whole-input admission; no `Notifier::notify` path
  bypasses it. Return `Action::InputRejected(InputError)` with metadata only.
- The paste widget preflights before publishing a payload and reuses the
  clipboard String allocation; actual admission repeats the check atomically.
  Clipboard API allocation itself is outside the writer budget. Rejections
  return without font measurement, grid snapshot or terminal redraw.
- Alternate-screen scroll expansion preflights its full byte count before
  allocation and handles `i32::MIN` without overflow.
- Expose `Terminal::{preflight_input,input_status}`, `InputError`, `InputStatus`,
  and `INPUT_BYTE_LIMIT`. `BackendCommand::RequestShutdown` requests the existing
  shutdown and explicitly cancels pending input without waiting for cleanup.
  This is not a joined/drained shutdown acknowledgement (#7).
  Requested close is marked as expected in the bridge, avoiding a false
  BackendStopped error while retaining any pre-existing overload/failure.
- Alacritty's provenance and actual-writer accounting patch are documented in
  [its ledger](../alacritty_terminal/PATCHES.md). The 41 iced_term regressions
  run under both renderers, including expected-close/error retention; the new
  bounded-input and writer-failure tests live with the dependency implementation.
