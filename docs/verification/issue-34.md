# R34 — recoverable errors and bounded private diagnostics

This is the complete implementation checkpoint for issue #34, submitted as a
draft while exclusive R1 review and the explicitly unperformed native Windows
checks remain open. It does not claim final approval or expand the existing
physical #13/#14 holds or owner14 dependency waiver.

## Source and behavior

Starting base was actual merged main `a5fe9fc4d8f0df53db240155ecebe0088d9898f4`.
Actual R32 `8543f823c35003528413efbd6b547334532d94c8` was deliberately merged
at `2908823`; actual R33 `f1737cfd3b5b85986a9a34acafd4f0c0cf1abfef` was
deliberately merged at `ae363c0`. Original refs, checkpoints, raw reds, reports,
staged/untracked artifacts and this worktree's build caches were preserved.
The source/evidence commit containing this report is identified in the PR and
worker checkpoint; no self-merge, reset, history rewrite or CI action occurred.

- `src/errors.rs`, `src/errors/writes.rs`, `src/diagnostics.rs`: typed categories,
  stable source/session IDs, checked notice/write identities, 64-notice/record
  bounds, repeated-error counts, stale-control/ack rejection and memory-only
  opt-in retention. No arbitrary error context string can enter the collector.
- `src/app/errors*`, app/terminal/config/clipboard/lifecycle adapters: visible
  owner-checked retry/dismiss controls. One capped recovery row sits outside
  PaneGrid after the strip, using actually merged UI tokens. Labels derive from
  current tab/session ownership; cards are keyed by checked ErrorId. Root space
  is consumed once; no extra per-pane error footer obscures a prompt.
- Explicit controls restore only today's active terminal through the existing
  guarded focus operation. Admission, rendering, async replies and cancellations
  never select an error origin. PaneGrid focus, capture, gestures and command
  resolution remain the original single paths. Stopped/start/failed/exited panes
  retain coherent lifecycle presentation.
- Paste retry uses the existing reader, owner reservation and app/window leases.
  Recovery reads additionally carry their originating ErrorId; revoked replies
  passively end only that pending attempt. Copy uses the existing acknowledged
  writer, with a bounded current-operation receipt ledger and no readback.
- Alacritty/iced_term resize adapters return typed native results on the existing
  PTY worker/bridge. One latest slot plus current serial/size checks rejects
  stale/duplicate results. Frames deduplicate attempted geometry; an explicit
  retry sends the current desired size once. Resolved resize notices retain their
  root space until dismissed. Link-open failure returns a safe action, not panic.
- Patched iced_runtime/iced_winit expose acknowledged writes and ordered renderer
  readiness/failure. Boot starts no PTY until that sink exists. Startup without
  a renderer gives a fixed safe report; fatal rendering reuses owned shutdown.
- Native cleanup uses fixed user32 standard controls, Wait default/Escape/X and
  explicit Force only. One current timeout token, synchronized dialog lifetime,
  cancellation and actual disposal guard every result. The same runtime has
  four lazy blocking slots: at most two preparations, one config read, one prompt.
  Config/prompt permits live inside their closures through cancellation/return.

User policy and retention: [errors.md](../errors.md). Published pinned archive
provenance/license and exact changes are retained in each vendor patch ledger.
The native fixture alone enables the finite renderer fault hook; production has
no environment/ordinary runtime injection trigger or second renderer/PTY policy.

## Independent findings corrected by the original author

R1's original source, exact bodies and red evidence remain retained for central
advisory publication. Both P1s were independently green again on `cd5a82e`:
blocked preparation jobs no longer starve native recovery; renderer failure
after an existing timeout now queues its same single choice. The exact original
strict prompt probe is still red only on a surplus macOS Cancelled-only result
assertion after it proves entry-before-release; the reviewer's unchanged portable
correction passes with truthful Unavailable(None), all pool work drained.

The later P2 canceled-paste probe now passes unchanged on this integrated source:
`stale_reply_wrote=false retrying_after_reply=false fresh_recovery_available=true`.
Native-focus/modal revocation and an old reply against a newer recovery are also
covered. Input admission, source ownership and one-use guards stay intact.

## Verification ran locally

All Cargo commands used locked dependencies, CARGO_BUILD_JOBS=4 and this
worktree's own reused target. GitHub Actions remains disabled. Selected native
graphics/socket checks ran outside the filesystem sandbox where required; no
provider/account/global environment was changed. Personal scoped CODEX_HOME
login was verified at start. Logs below are in `docs/baselines/issue-34/`.

| Local command / evidence | Result |
| --- | --- |
| `cargo fmt --all -- --check` / integrated-format.log | Passed. |
| `cargo clippy --locked --all-targets -- -D warnings` / integrated-clippy-after-cancel.log | Passed after P2 and full wiring; two existing dependency try_next deprecation warnings remain outside root warnings-as-errors. |
| `cargo test --locked` / integrated-tests.log | 206 library tests passed / 12 explicitly ignored, plus model6 and doctests2. This full root run preceded the final P2 change; targeted affected checks below passed afterward. No duplicate full-workspace/both-renderer run was substituted. |
| `cargo test --locked --lib app::tests::errors` / integrated-errors-after-cancel.log | 10 passed / 2 graphics tests explicitly ignored by ordinary selector, after P2. |
| `cargo test --locked --lib root_recovery_row -- --ignored` / integrated-root-widget-final.log | Passed on actual production App::view/root capture/pane grid + real PTYs: background Dismiss/Retry, current healthy input, root-only space and fresh synthetic runtime clipboard routing/privacy. |
| `cargo test --locked --lib reviewer_clipboard_cancel -- --nocapture` / integrated-paste-cancel-probe.log | Exact unchanged R1 P2 probe passed after actual PTY cleanup. |
| Native/modal canceled-read and newer recovery / integrated-paste-cancel-order.log | Passed, including no stale payload write and no old-reply cancellation of a newer attempt. |
| Resize/split geometry / integrated-resize-geometry.log, integrated-split-geometry.log | 2 and 8 passed / 1 existing graphics case ignored; root error space does not add another per-pane footer. |
| `cargo check --locked --lib --bin pike --tests --target x86_64-pc-windows-msvc` / integrated-windows-check.log | Passed after final P2/root changes. Cross-compilation only. |
| Explicit patched dependency tests | iced_term resize8, Alacritty Unix resize2 and fallible-worker receipt passed; native Windows Alacritty tests cross-compiled. Runtime clipboard/readiness3 and winit renderer/clipboard6 earlier passed on unchanged retained patch source. See the two resume manifests and raw logs. |
| Patched dependency Clippy | Alacritty + iced_term library/tests passed; final iced_term stale-receipt rerun passed. Earlier unchanged runtime passed; winit passed with its documented existing deprecated-method allowance. |
| Actual native Iced first-frame fatal renderer / integrated-native-first-frame | No PTY/child was created: handles0, completions0, children0, live0. Pool cleanup=false here is truthful: no owned backend existed to shut down. |
| Actual native Iced fatal renderer with attachments / integrated-native-attached | Passed: handles2, completions-at-return2, controlled children2, live-at-return0, live-after-wait0, pool_cleanup=true. |

The native Iced/PTY and offscreen checks above ran on macOS. They are not native
Windows or physical keyboard/monitor checks. New user32 interaction/cancellation,
ConPTY signed conversion/HRESULT runtime behavior and emergency capacity on a
native Windows host remain explicit gates; root is asking the owner about host
availability. The owner has now confirmed that no Windows host is available;
these native gates remain unexecuted with no new waiver. No release memory/CPU/responsiveness measurements or physical-input
pass are claimed. No new platform waiver or broader acceptance claim is inferred.

All development reds are retained, including sandbox graphics/socket denial,
fixture-only borrow/SDK Debug/path compile corrections, root lint findings, the
strict probe's portable expectation and earlier renderer-readiness investigation.
The prescribed live-ID heartbeat was natively rejected once with no effects
(request `f665606d-8684-4d45-8bc4-4a1f8bf83a68`); exact check/status continued,
and no terminal/task/dispatch capability was substituted or blindly retried.

## Root widget identity correction after PR85 head 50abc7d

R1 reproduced another P1: conditional passive root siblings shifted the ordinary
Column's pane child and rebuilt its widget tree, losing healthy typing focus.
The source fix keeps permanent empty slots for both legacy and typed passive
rows. Empty slots use Shrink height (zero actual layout height), since pinned
Column::push intentionally omits Fixed(0) children. No passive refocus task,
selection/capture/gesture change or new authority was introduced.

The expanded actual-root regression is red before the correction
(`root-identity-red.log`); the first Fixed(0) attempt remains red in the retained
`root-identity-green.log` despite that early filename. The corrected non-void
slot version passes in `root-identity-stable-slot-green.log`: default-off first
error admission, last asynchronous write-success removal, constraint-row
appearance/removal, explicit background controls and fresh clipboard input all
preserve exact healthy-owner typing. Formatting and all-target Clippy pass in
`root-identity-format.log` and `root-identity-clippy.log`.

The unchanged pinned independent root/runtime/Cache probe also confirms healthy
widget focus stays true across first-error admission and exact x/FRESH34 bytes
reach only the healthy child; both handles acknowledge cleanup. Its later
Copy-diagnostics lookup stops at the already-identified no-scroll fixture
assumption (`root-identity-native-probe`), so this original-body run is retained
as partial coverage, not advertised as a complete probe pass. R1 owns that
fixture's scrollbar refinement and will rerun an immutable correction head.
Shared dependency identities match; the initial comparison incorrectly required
unused root-lock packages to remain in the standalone probe and is retained
separately. The corrected comparison permits pruned unused packages and adds
only the probe package (`root-identity-probe-lock.txt`).
