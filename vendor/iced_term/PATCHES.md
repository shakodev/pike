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

## R13 / issue #13 — widget identity and pane-local geometry

- Backport the terminal-ID-aware `TerminalView::diff` from
  [upstream PR 70](https://github.com/kemokempo/iced_term/pull/70), head
  `d9b343fdd5362979c61be307cdd0fa4ab301ef80`, onto the patched 0.8.0 view.
  Replace transient state when the session ID changes, including focus,
  dragging, click history, scroll remainder, modifiers and the first-layout
  marker. Session-owned selection, scrollback, cache and PTY stay with Terminal.
- Track current widget bounds to cancel obsolete pointer gestures on a move or
  resize. Same-size origin changes never send native resize commands. Unfocus,
  window focus loss and releases outside the pane also cancel transient drags.
- Cache geometry in local pane coordinates and key the canvas cache by pane
  size. Apply the current layout origin when drawing, and clip all glyphs,
  cursor, selection and background runs to the pane/ancestor viewport
  intersection. Moving a pane can reuse its cached geometry.
- Explicit DPI events clear the canvas once without font measurement, model
  reflow or PTY resizing when logical bounds are unchanged. Font/theme changes
  retain their R12 invalidation paths. `redraw` uses existing cache interior
  mutability and a Cell counter so widget-triggered invalidation is included
  in WorkStats. No new public API, dependencies, tasks, timers or unsafe code.
- Button presses/releases recompute their grid point from the current cursor.
  A diff or move can reset the remembered point without a CursorMoved event;
  the first subsequent click must still target the correct cell. A regression
  reproduces the old (0,0) report and checks both press and release coordinates.
- Five focused adapter tests plus a real-PTY/offscreen example exercise
  equal-sized replacement, focus and drag isolation, initial layout delivery,
  nested swaps, maximize/restore, theme/font changes and 125/150/200% pixel
  clipping. See [verification and Windows limitations](../../docs/verification/issue-13.md).
  Remove this patch when upstream provides equivalent identity, local geometry
  and clipping behavior; PR 70 alone covers only the ID reset.

## R15 / issue #15 — authoritative selection and explicit copy

- Preserve R11's Alacritty `selection_to_string` delegation, also used by
  upstream PR90 (`f5b49232dededd266feafd27f1e2c67c5c3e416c`). Keep its
  `Option` instead of replacing no selection with empty text.
- `Terminal::copy_selection(writer)` extracts only on explicit request, skips
  empty text, releases the model lock before invoking the writer once and
  returns its result. No selection, viewport, input or other session mutation.
- Capture Copy even when empty/unavailable; use each character event's own
  modifiers for bindings. Add Ctrl+Shift+C on macOS while retaining Command+C.
  The app menu carries the current stable terminal ID and rejects stale IDs.
- Exact VT tests and an opt-in real-PTY/widget probe cover hard/soft lines,
  padding, wide/combining cells, history, session swaps and unavailable sinks.
  Windows-only fixture APIs perform actual CF_UNICODETEXT round trips and a
  competing-thread clipboard lock; no production clipboard implementation.
  Native OS readback references only its initialized prefix in the same open
  write transaction. Later bounded receipts validate owner/sequence/identity
  without dereferencing clipboard memory; foreign data never gains Rust references.
- Pinned Alacritty preserves no-op/height-only selection and invalidates it on
  column reflow. Follow this policy; never cache/resurrect stale copied text.
  Iced's unit-returning clipboard facade cannot expose OS errors. See
  [verification and limitations](../../docs/verification/issue-15.md).
- No production dependency, lockfile, transcript/cache or background work.
  Remove when upstream offers equivalent extraction, explicit action and
  handled-key semantics; PR90 alone covers only extraction delegation.

## R18 / issue #18 — wide/combining cell rendering

- Draw authoritative base+zerowidth text in one/two-column spans, skip both
  continuation flags, and clip Unicode ink to its cell span/row without moving
  adjacent ASCII. Wide cursor blocks normalize continuation positions and use
  the lead width. Paint all backgrounds before glyphs/cursor.
- Preserve cached ASCII/pane geometry. Pinned tiny-skia canvas text omits
  per-text clip bounds, so Unicode uses clipped core-renderer shaped buffers cached in
  widget state by snapshot/style generation, bounded to one viewport. Session
  identity reset drops obsolete spans; ordinary redraws do not rebuild them.
- Bound extra shaping/text to 32 combining scalars per cell; authoritative
  model/copy data stays intact. Document pathological mark stacks, cross-cell
  contextual shaping and complex/color emoji as unsupported display cases.
- Seven VT/model/cache regressions plus the real-PTY `unicode_cells` release
  fixture cover positions, reflow/wrap, cursor, fallback, cell clipping and
  actual widget copy across two fonts/four scales and both lean renderers.
  Run explicit vendor suites for `iced/wgpu,iced/tokio` and
  `iced/tiny-skia,iced/tokio`; see `docs/verification/issue-18.md` for native
  evidence and limits. No dependency/feature or production background work.

## R20 / issue #20 — explicit protocol and metadata contract

- Exhaustively handle all pinned Alacritty events: current size/color replies,
  title/reset, coalesced bell, cursor state, actual ChildExit/Shutdown status and
  separate backend completion. Subscription events keep their origin ID even
  while hidden; workspace metadata UI remains issue #23.
- Use the existing bounded ordered input writer for replies and mode-gated
  effective focus transitions. Reply rejection closes only its origin session;
  visibility/focus no-ops do not send reports. No reply path scrolls to bottom.
- Disable OSC52 explicitly in initial and history-update parser configuration,
  drop injected clipboard requests before bridge retention, and invoke no
  system clipboard access. Titles share Alacritty's bounded sanitizer.
- Share the existing theme object between rendering and query resolution;
  metadata/query handling does not mark the viewport dirty. Cursor blink state
  is exposed without a production timer. See the Alacritty patch ledger for
  source-level title bounds, RIS reset notification and ordered color overrides.
- Explicit adapter regressions plus a two-session synthetic real-PTY release
  probe verify bytes/order/identity, hidden metadata, metadata floods, focus and
  natural drained cleanup. See [verification](../../docs/verification/issue-20.md).
  No dependencies, features, lockfiles, polling tasks or additional PTY code.
- Review fixes: one owner-bound WindowFocus per actual window survives terminal
  replacement/new widget trees; unknown focus stays unfocused. The existing
  single-window app records actual focus during startup and binds it before
  showing a terminal. Public widget regressions check admitted CSI-I/O bytes.
- CSI 14t now uses aggregate virtual logical-grid pixels, rounding the product
  once (80x8.5 = 680). The Alacritty formatter accepts TextAreaSize separately
  from native integer WindowSize. DPI rescale leaves these virtual units and
  native PTY cell-count dedup unchanged; no physical-monitor claim is made.

## R21 / issue #21 — steady cursor shapes and contrast

- Render authoritative shape/show state and OSC12/reset color; blinking variants
  deliberately use steady counterparts with no cursor timer or redraw loop.
  Focused block/underline/beam/hollow and inactive hollow use the R20 owner focus.
- Preserve R18 background-first drawing, wide-lead cursor geometry and all cell,
  pane and ancestor clips. Resolve contrast against effective inverse/selected
  backgrounds and keep a block's covered glyph legible, including equal colors.
- Separate raw Unicode shaping generations from paint colors: focus/theme paint
  reuses buffers and resolves current glyph colors when drawing. Focus metadata
  changes paint without snapshots/font measurement; input/copy handlers stay intact.
- Four parser/model regressions and the public real-PTY `steady_cursor` fixture
  cover shapes, state-only changes, pixels, idle counters and official Micro.
  Both native renderer jobs retain controlled screenshots and cleanup evidence.
  See [verification and limits](../../docs/verification/issue-21.md), including
  existing macOS cancellation timeouts and deferred physical desktop checks.
- No dependencies, features, lockfiles, tasks, timers or PTY implementation added.
  Remove when upstream provides equivalent steady shapes, focus/contrast behavior
  and cached Unicode paint invalidation; PR86 alone covers draw ordering only.

## R16 / issue #16 — bounded mode-aware paste

- Add one shared `Terminal::paste(Option<String>)` path for menu clipboard
  results and widget Ctrl+Shift+V/Shift+Insert (plus existing platform Paste).
  Clipboard failure/oversize/pending/input rejection returns redacted
  `Action::PasteRejected(PasteError)`; ordinary copy and Ctrl+C are unchanged.
- Widget messages carry `BackendCommand::Paste(PasteRequest)`, an owner-bound,
  redacted, cloneable single-use token. A separate pending slot retains at most
  one original <=1 MiB UTF-8 payload per session as a boxed string. Queueing is
  not acceptance or writer reservation. A second pending request fails visibly;
  last-token drop, explicit cancel, wrong-owner handling, close and completion
  release the payload. Replayed/cancelled requests cannot write again.
- Keep existing deferred keyboard/mouse/focus/close ordering. At ordered handling,
  read the current emulator mode and preflight the complete encoded frame before
  allocation/write; atomically send it through R06's existing 2 MiB FIFO and
  8 KiB chunks. Budget changes after advisory queue preflight can reject the
  entire request, never a prefix or a lone opening marker. No Alacritty change.
- Normalize CRLF/lone CR/LF to LF in bracketed mode, CR otherwise. Preserve tabs
  and printable Unicode; strip other C0/C1 controls and DEL in both modes.
  Bracket only when enabled, suppress empty frames, never append Enter.
- Reviewed upstream PR91 at `ea95f8edd29fdd4d0e093fd3b528a3ea8902f4ec`
  and Alacritty UI paste at pinned crate revision
  `12082407d9cb3384a2bd83c578e556ec3faa2c4d`: retain its mode gating,
  marker-breaking ESC/ETX filtering and normal-mode newline intent. Unlike
  their separated/raw writes, Pike validates the whole frame, uses stricter
  control filtering and canonical bracketed LF, and applies one atomic send.
- No dependencies/features/lockfile or mouse/history behavior changes. The
  current app host surfaces PasteRejected in its existing notice; R23 owns
  the full session host/menu integration.
  Remove this patch when upstream provides equivalent bounded, owner-bound,
  ordered paste with the same byte/control/error contract. See
  [targeted local evidence and limitations](../../docs/verification/issue-16.md).
