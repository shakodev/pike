# R34 actual32 adapters and original-author P1 corrections

This remains the same active full issue34 task, with exclusive R1 review. The
previous stop was revoked; all refs, original reports, source checkpoints and
build caches remain preserved. Actual32 main `8543f823c35003528413efbd6b547334532d94c8`
was deliberately merged as `2908823`; no unmerged visual imports or App::view
edits were made. Actual33 presentation integration is still gated by root.

## Source changes

- Existing merged paste reader/leases remain the sole paste route. Error retry
  captures the existing app/window lease plus a shared one-use flag, then asks
  that reader for fresh data. Plain old controls cannot replay clipboard input.
- Copy uses the existing acknowledged writer without readback. Its bounded
  per-owner ledger checks current operation serial and original reservation;
  old/duplicate acknowledgments cannot supersede a newer success. Pending retry
  notices reject repetition and receive a fresh ErrorId on another failure.
- Config read/lookup failures and command refusals enter the typed error store.
  Successful explicit config retry resolves its category without changing focus.
  README and docs/errors.md document default-off, 64-record / 15-minute,
  memory-only diagnostics and event-driven expiration/actual clear behavior.
- Native conversion fixtures cover Unix invalid fd and saturating pixel bounds;
  Windows tests cover signed COORD limits and verbatim signed HRESULT metadata.
  The fixed user32 callback has explicit Wait/Force command mapping tests.

## Independent review findings and corrections

R1 reproduced two P1 defects on exact source
`df6087f4f22a0109bf87640c3a9ec0f0a6e7b983`. Independent original bodies and red
evidence remain in the reviewer workspace and root's advisory publication queue.

1. Two admitted launch preparations consumed both old blocking workers and
   prevented the emergency cleanup dialog from entering. The single existing
   runtime now has four lazy blocking slots: two capped preparations, one config
   read, one prompt. Config/prompt own one-slot permits **inside** each blocking
   closure until it returns; cancellation cannot prematurely admit another.
   Production blocking consumers were audited: these are the only three sites.
2. Renderer failure after an already-delivered cleanup timeout returned before
   queuing a fallback. It now hands that same current token to the existing
   one-prompt adapter, without automatically rearming a timer or choosing Force.

## Local evidence

All checks are local, locked, with CARGO_BUILD_JOBS=4 and this worktree's own
reused target. No Actions, second runtime/PTY/renderer, account/environment
mutation, physical input test or Windows native execution occurred.

| Check / log | Result |
| --- | --- |
| `cargo test --locked --lib app::tests::errors` / resume-clipboard-and-owner-final.log | 7 passed, including real PTY survivor/input/privacy and fresh owned clipboard retry. |
| `cargo test --locked --lib config_retry_and_command_refusal` / resume-config-recovery-privacy.log | 1 passed: actual async config load/retry, originating typed refusal, focus and secret-like args/env exclusion. |
| `cargo test --locked --lib app::commands::host::tests` / resume-r32-command-host.log | 8 passed. |
| Settings tests / resume-r32-settings.log | Original 4 passed; closure-slot retention separately passed in resume-config-slot-retention.log. |
| `cargo test --locked --test recoverable_model` / resume-bounded-model-green.log | 6 passed: bounds, coalescing, eviction, retention, off/clear and secret exclusion; new write-ledger test included. |
| `cargo test --locked --lib app::close::native::tests` / resume-recovery-capacity-and-order.log | 3 passed, including two real launch jobs plus gated config surrogate held while actual precancelled prompt enters. |
| Exact unchanged reviewer late-renderer probe / resume-exact-late-renderer-probe.log | Passed: native_choice_enqueued=true, renderer_response_units=1; all finite work drained before assertion. |
| Reviewer portable prompt probe / resume-unchanged-prompt-probe.log | Passed unchanged from reviewer's portable body: entry before ordinary gate release, Unavailable(None) on macOS, pool_cleanup=true. |
| Original red-body prompt rerun / resume-original-prompt-probe.log | Scheduling is corrected (entry=true, pool_cleanup=true); exit101 remains from surplus Cancelled-only expectation on macOS. Preserved original body beside portable body; production behavior was not altered to fit it. |
| Executor tests / resume-executor-budget.log | 5 passed: one runtime, budget saturation, abort and blocking-slot lifetime. |
| Launch cancellation and budget / resume-launch-cancellation.log, resume-launch-budget.log | 1 each passed. |
| Queued admission/cancellation / resume-queued-cancellation.log | 6 passed after tests explicitly filled all four worker slots. |
| Native prompt cancel/rearm retention / resume-native-slot-retention.log | 1 passed with actual PTY completion and held prompt permit. |
| `cargo test --locked -p iced_term --lib resize` / resume-resize-receipts-final.log | 8 passed, including real PTY acknowledgment, deduplication, stale/current serials and one latest bridge slot. |
| `cargo test --locked -p alacritty_terminal --lib tty::unix::resize_errors` / resume-unix-resize-limits.log | 2 passed. Earlier fallible worker receipt and neighboring socket regression also passed, with sandbox red retained. |
| `cargo clippy --locked -p alacritty_terminal -p iced_term --lib --tests -- -D warnings` / resume-vendor-clippy.log | Passed before final bridge stale-result strengthening; final relevant rerun remains due. |
| `cargo clippy --locked -p iced_term --lib --tests -- -D warnings` / resume-receipts-clippy-final.log | Passed after the final stale-result bridge strengthening. |
| Windows host tests cross-check / resume-r32-windows-host.log | Passed for lib/bin/tests; cross-compilation only. |
| Windows Alacritty test cross-check / resume-alacritty-windows-fixtures-green.log | Passed, including signed conversion/HRESULT test compilation; no Windows execution claim. |

Additional preserved reds: duplicate-module path in the independent model target
(fixed with explicit path); Windows test unwrap_err required Debug on SDK COORD
(fixed in fixture only). Earlier unused new-view warnings remain expected until
the agreed actual33 insertion. Full root Clippy/test/format acceptance, visible
banner geometry/focus evidence, final integrated native fixtures and full R1
verdict remain outstanding; no complete PR or approval is claimed here.

## Explicit-click follow-up (after cd5a82e)

Pinned TerminalView intentionally unfocuses on outside left-clicks. Explicit
recovery/diagnostic controls therefore restore only today's active widget using
the existing guarded focus_active task. Reporting and asynchronous error/result
arrival remain passive; no focus/capture/gesture module changed. An independent
offscreen composition reserves a clipped fixed 96px banner, verifies disjoint
terminal/banner bounds, clicks Dismiss, executes the real guarded operation and
admits exactly one next synthetic keybyte to the same real PTY. The sandbox had
no graphics adapter (`resume-passive-widget.log`); the exact unsandboxed local
wgpu rerun passed (`resume-passive-widget-native.log`). Relevant host errors:
8 passed / 1 explicit graphics test skipped by the ordinary selector
(`resume-explicit-control-focus.log`). These are macOS offscreen tests, not
physical input or native Windows tests.

This proves only the independent composition. Pinned PaneGrid unconditionally
emits its on_click Focus even after a child captures a banner press. Final
background-pane passive insertion requires the coordinator-approved narrow
presentation seam and an actual-grid healthy-sibling click/typing regression;
that gate is explicitly still pending and no shared closure has been edited.
