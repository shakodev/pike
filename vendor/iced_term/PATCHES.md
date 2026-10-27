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
  and disables its writes/resizes. R11 later permits retained-model resizing
  after failure, deferring reflow until EOF draining completes. Lifecycle events
  remain deliverable.
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

## R07 / issue #7 — explicit backend completion and final output

- `Terminal::{request_close,shutdown_handle,final_content}` expose idempotent,
  nonblocking close, cancellation-safe completion observation, and the retained
  read-only terminal model. Cloned handles share one request flag and one sticky
  `BackendCompletion` (reason, optional child code, EOF-drain flag).
- The existing PTY worker now disposes its parser/EventLoop/PTY itself; no
  completed JoinHandle retains backend state and no UI thread joins a worker.
  Source completion (`EventBridgeEvent::Completed`) precedes backend disposal;
  `BackendCompleted` and `Action::BackendCompleted` acknowledge disposal. The
  compatibility harness waits for the latter before removing a session.
- The bounded bridge reserves a separate sticky backend-completion slot and
  stays open until it is delivered. Completion observation also uses one Tokio
  watch value, independent of event-subscription lifetime, with no polling task.
  Canceling a subscription keeps its owned Terminal alive; dropping that owner
  requests close even when a subscription recipe/handle remains elsewhere.
- Natural exit enables the existing Alacritty parser's EOF drain before Exit;
  completed sessions reject input. This patch ignored geometry after exit;
  R11 later reflows retained models without notifying the disposed PTY. `final_content`
  can refresh the retained model without consuming subscription events.
- `Action::Shutdown` still describes terminal Exit, not released resources.
  Callers must use backend completion for resource/lifecycle decisions. A wait
  timeout never acknowledges cleanup. Async startup remains issue #10.
- No dependency, feature, lockfile, or second PTY changes. See the
  [Alacritty ledger](../alacritty_terminal/PATCHES.md) for worker/ConPTY details,
  and [verification](../../docs/verification/issue-7.md) for measured limits.

- Three upstream `view.rs` test initializers now set terminal mode directly
  rather than reassigning Default fields, enabling all-target workspace Clippy
  with denied warnings. Test behavior and production view code are unchanged.

## R09 / issue #9 — expose safe launch arguments and shell selection

- Default Windows BackendSettings to automatic shell selection (empty program,
  no args) instead of WSL. An explicit program is an unquoted executable name
  or path; never parse a full command line or silently fall back after failure.
- Add Windows `BackendSettings::escape_args`, true by default, forwarding to
  Alacritty's existing encoder. Set false only for deliberately constructed
  shell syntax such as cmd /D /S /C. No extra interpreter for ordinary args.
- Re-export `LaunchError` on Windows; the existing io::Result API carries this
  typed error through `get_ref().downcast_ref()`. Redact configuration Debug.
- Update synthetic harnesses to pass unquoted paths/argv and to opt into raw
  cmd scripts explicitly. Separate PowerShell scripts from native argv quoting.
  Existing lifecycle test scripts explicitly retain raw cmd semantics.

## R10 / issue #10 — prepare session state before UI attachment

- Add `PreparedTerminal::{new,shutdown_handle,attach}`. Preparation owns only the
  existing backend and bounded event receiver and is `Send`; it creates no font,
  widget ID or canvas cache. `attach` performs those UI operations on the caller's
  UI thread. No unsafe Send assertion or extra PTY implementation is used.
- Keep `Terminal::new` as a synchronous compatibility wrapper over those steps.
  Dropping an unattached prepared value uses Backend's existing nonblocking close
  and explicit completion handle. A retained event receiver does not own the PTY.
- Add explicit adapter regressions for worker-to-UI handoff and disposal without
  font attachment. Application startup uses bounded shared work, typed IDs,
  cancellation and completion-aware capacity; that policy stays outside the
  adapter. No dependency, feature or lockfile changes.
- See [issue #10 verification](../../docs/verification/issue-10.md), including
  native-Windows verification still required outside GitHub Actions.

## R11 / issue #11 — bounded history and viewport-only presentation

- `BackendSettings::history` accepts a validated `HistoryLimit`: default 2,000,
  maximum 10,000 lines, with zero disabling normal-screen history. The existing
  Alacritty Config/set_options path applies changes to the primary buffer even
  while alternate screen is active. `BackendCommand::SetHistoryLimit` updates
  the limit live, including retained exited models.
- Replace `RenderableContent::grid` with `viewport: ViewportSnapshot`, an owned
  row-major vector of only visible cells. Retain absolute points, display offset,
  cursor cell/point, selection and mode metadata. Refresh reuses its allocation;
  no production Grid/history clone or transcript remains. Cursor and view code,
  lifecycle tests and the shutdown example use the new viewport API.
- Prepared backends do not take snapshots. UI attachment makes them visible,
  or `attach_hidden` keeps snapshots at zero through the first activation. UI
  attachment seeds cell metrics without changing the initial grid. After backend
  completion, valid layouts reflow the retained model without notifying the PTY;
  pending EOF drains keep their parsing geometry. Layouts received during a drain
  are replayed before the next visible snapshot after completion, including after
  bridge failure; closed PTYs remain unnotified and input remains rejected.
  The application handoff exposes
  `PreparedSession::attach_hidden` through its existing ownership/cancellation gate.
  `Terminal::set_visible(false)` releases the viewport and canvas cache; callers
  must still subscribe and route every live session's events. Hidden handling,
  explicit refresh and completion perform zero snapshots. Activation refreshes
  once, even after exit; `final_content` returns None until activation. Snapshot
  counters expose metadata only and create no sampler/background task.
- Copy and URL extraction read the authoritative model on demand, so selection
  spanning multiple history pages does not depend on presentation copies. Mouse
  protocol encoding reads current modes rather than a stale hidden snapshot.
- This is a line cap, not a byte cap. Alacritty retains up to 1,000 spare rows;
  cell extras (combining marks and hyperlink strings) can grow within a cell.
  Snapshots share extras via the existing Arc/COW representation. These residual
  limits are measured and documented, without changing the emulator or PTY.
- Eleven new regressions cover saturation, zero/reduced history, normal/alternate
  screen, scrolling/selection, allocation reuse, wide/combining cell metadata,
  hidden native flood, hidden font changes, deferred shutdown layout after normal
  close/bridge failure and reactivation of exited output. An opt-in release probe
  compares the exact R03 full-grid clone algorithm with viewport/hidden paths;
  the clone exists only in that ignored profiling test.
- No dependency, lockfile, feature or Alacritty source changes. Remove this patch
  when the adapter exposes equivalent validated history and visibility-aware
  viewport snapshots upstream. See [Windows verification and residual byte
  limits](../../docs/verification/issue-11.md) for commands and measurements.

## R12 / issue #12 — cache font metrics and deduplicate resize work

- `font.rs` caches one advanced-shaped measurement by font family, size and
  relative line height; repeated settings and ordinary events never reshape.
  `FontSettings.scale_factor` remains line height, not DPI. Invalid values
  retain valid settings; finite extremes bound shaping to size 1–256 and
  relative height 0.1–8. Iced's layout already uses logical coordinates.
- `geometry.rs` separates latest positive layout, cached fractional cell metrics
  and applied terminal size. Geometry/selection/rendering use the same floats;
  integer conversion stays at the native boundary. Invalid/sub-cell layouts
  preserve the applied grid; positive sub-cell bounds remain current for later
  font reduction. Preserve new fonts across
  tiny layouts, and cap viewport allocation at 512×512 cells. Initial synthetic
  bounds use f64 products to preserve initial dimensions exactly.
  Native pixel metadata also bounds cell-by-row/column products to u16, avoiding
  Unix winsize overflow without changing logical metrics or Alacritty.
- `backend.rs` reflows only for changed rows/columns. Windows native resize
  comparison uses ConPTY's rows/columns; Unix additionally compares rounded
  pixel metadata. Preserve R11's deferred EOF-drain reflow and closed-PTY rule.
- Separate snapshot/render dirty state. Output/selection/geometry/history/final
  completion capture a visible viewport; title/bell/protocol/input-at-bottom
  and no-ops do not. Theme/hover changes repaint without snapshotting. Hidden
  events stay snapshot/cache free; activation and explicit refresh retain their
  contracts. Explicit final refresh now invalidates its render cache too.
- `terminal.rs` exposes metadata-only `WorkStats` for measurements, notifier
  calls, grid resizes and explicit cache clears. No background sampler, timer,
  task, polling, unsafe code or extra allocation per ordinary event is added.
  Widget layout commands no longer carry a possibly stale font measurement.
- Thirteen new regressions plus an opt-in native size/flood release probe verify
  counters, fractional/invalid/extreme dimensions, four logical scale values,
  font changes and hidden work. See [Windows evidence and remaining physical
  DPI/minimize validation](../../docs/verification/issue-12.md).
- No dependencies, lockfile, feature, renderer default or Alacritty changes.
  Remove when upstream offers equivalent cached logical metrics, validated
  geometry and mutation-specific presentation invalidation.

## R14 / issue #14 — legacy keyboard bytes and explicit consumption

- Add `keyboard.rs`: a pure legacy VT/xterm encoder with event-local modifiers,
  Ctrl+A–Z/C0 mappings, normal/application cursor modes, F1–F20 and modified
  navigation/function keys, Alt prefixes and committed-text fallback. The
  Ctrl+U=0x15 correction covers Ctrl+Shift+U too, following
  [upstream PR 89](https://github.com/kemokempo/iced_term/pull/89), head `f331a47`.
  Correct malformed modified F1–F4 SS3 sequences and Alt+Insert's wrong code.
- Keep only platform clipboard/custom overrides in `BindingsLayout`, rather
  than allocating the large default keyboard table per session. Lookup borrows
  character keys without lowercasing allocations; latest matching overrides win.
  A missing binding is None; explicit `BindingAction::Ignore` consumes without
  bytes. Copy/Paste consume even when no command/text is produced.
- `view.rs` captures consumed keys independently of emitted commands and uses
  each key's modifiers. Repeat presses encode once; releases emit no legacy
  bytes. Committed text is borrowed until R06 preflight; input reaches the same
  bounded writer. Ctrl+Alt printable text takes precedence over default legacy
  encoding, preserving AltGr; dead/uncommitted keys do not manufacture text.
- Disable default link activation and make explicit `LinkAction::Open` inert
  until R44. Delete the panic-prone launcher and remove `open` from the normalized
  manifest and root lockfile, also dropping its unused is-wsl/is-docker packages.
  Published archive files and original manifest/lockfile remain provenance.
- Set Alacritty kitty configuration explicitly false during preparation and
  history changes. Do not advertise or encode negotiated kitty behavior (R45).
- Honor ANSI newline mode for Return without changing Ctrl+M; remove the
  inherited macOS Command+H-to-DEL binding so the host shortcut emits no bytes.
- Ten keyboard regressions and one backend policy regression extend the
  adapter suite to 81 passing tests per profile. The raw-byte widget/PTY probe
  exercises 168 cases in all four cursor/newline modes, byte admission, and a synthetic Vim
  edit/save/exit. No production polling/tasks or new dependencies are introduced.
  See [verification, policy and native limitations](../../docs/verification/issue-14.md).
