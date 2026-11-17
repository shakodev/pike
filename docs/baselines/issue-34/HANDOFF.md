# R34 owner-stop handoff — partial, not acceptance-ready

The owner explicitly requested that all agents finish their current increment
and stop. No further scope or integration should be inferred as authorized by
this checkpoint. The full dispatched task remains incomplete; report outcome is
**failed at owner stop**, with useful partial work preserved.

## Identity and repository state

- Issue: https://github.com/shakodev/pike/issues/34.
- Owned branch: `issue-34-recoverable-errors`.
- Exact starting/merged base: `a5fe9fc4d8f0df53db240155ecebe0088d9898f4` (PR81/R31).
- Source/evidence checkpoint: `94c739f9cc30a1529ebb2dec69da2dfa407b2198`.
- This handoff and final two bounded check logs are committed immediately after
  that source checkpoint; final report commit is supplied in the worker result.
- Task `task_4dec81a9f018`, Dispatch `ctx_21e936f0c79b`.
- Existing `issue-30-tab-management` at `92aac7b`, main, other refs, reports and
  build caches were preserved. No reset, rewrite, self-merge or CI action ran.
- No PR closing #34 or review approval was created. No exclusive reviewer was
  assigned for this unfinished checkpoint. Actual A32/B33 integration is pending.
- Git identity was absent in this shell. Commits use the exact existing owned
  branch/base author through per-command `git -c`, without configuration or
  account changes. Personal CODEX_HOME login was verified at task start.

[Exact checkpoint file list](checkpoint-files.txt) includes all 108 changed paths.
The new same-version runtime archive accounts for many of those files; 15
unmodified published files were byte-compared against installed pinned sources.

## Implemented bounded increment

- `src/errors.rs`: typed failure categories, stable owners, opaque checked error
  IDs, at most 64 coalesced notices, stale-button rejection and category-specific
  success resolution. Context has no arbitrary string input.
- `src/diagnostics.rs`: explicit opt-in, memory-only typed metadata, 64-record /
  15-minute retention from first occurrence, saturating coalesced counts,
  event-driven expiration and actual allocation release on clear/disable. No
  logger, transcript, periodic task, file or network export.
- `src/app/errors.rs` and `src/app/errors/view.rs`: checked recovery handlers,
  public metadata/capture hooks, bounded passive view methods and height API.
  **View methods are not inserted into shared chrome yet.** Paste retry is not
  wired; its proposed SessionId-only route/readback was removed as instructed.
- `src/terminal.rs`, `src/app/startup.rs`, `src/app.rs`, `src/app/close.rs`: typed
  reporting after existing current owner/reservation checks, fresh-ID launch
  retry, healthy-session preservation, cleanup-source metadata and normal
  lifecycle cleanup for renderer failures.
- `vendor/iced_runtime`: additive acknowledged clipboard write API; metadata-only
  WriteError; redacted clipboard Action Debug; bounded renderer Ready/Failure
  task protocol. Exact 0.14.0 provenance and MIT license retained in its ledger.
- `vendor/iced_winit`: existing native writer returns actual metadata without
  readback or raw error logging; existing compositor recovery has a finite
  three-retry budget, success/surface reset and safe Result instead of OOM panic
  or unbounded logging/redraw. One sink, no alternate renderer/PTY.
- `src/app/boot.rs`: production boot waits for renderer readiness before backend
  creation/config loading, retaining the same App, WindowFocus, theme and input
  authority. Immediate legacy constructors keep their original semantics.
  Guards cover pre-ready controls and lower-level creation entry points.
- `src/main.rs`, `src/startup_report.rs`: use boot gate, explicit --diagnostics,
  fixed safe startup/renderer stderr and fixed Windows MessageBoxW report. The
  tiny unsafe call has documented UTF-16 buffer/handle lifetime justification.
- `examples/startup_lifecycle.rs`: the existing whole-library nested include had
  broken root-relative imports. Its focused inline source facade now includes
  only original pool/model dependencies with one identity domain.
- New app/boot/model/privacy tests and isolated native fixture retain synthetic
  failures and secret-like test data. The fixture alone enables an off-by-default
  bounded renderer fault hook; no environment/normal-production trigger exists.

## Local verification and limits

All verification ran locally. `CARGO_BUILD_JOBS=4`, this worktree's reused target,
locked dependencies and Actions OFF were retained. Logs live beside this report.

| Command / evidence | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed at final source checkpoint; zero-byte check logs. |
| `cargo check --locked --lib --bin pike` | Passed; expected unused new view/height warnings because agreed root insertion is pending. |
| `cargo test --locked --test recoverable_model` | 5 passed on final source (bounds, coalescing, eviction, resolution, retention, off/clear allocation release and secret exclusion). |
| `cargo test --locked --lib app::tests::errors` | 5 passed; current-origin failures, actual healthy-sibling input after errors, fresh-ID retry, stale receipts, real PTY output/input/env/args/title/clipboard synthetic privacy and actual pool cleanup. `healthy-input-proof.log`. |
| Boot/readiness tests | Corrected first 3 passed in `boot-gate-tests-green.log`; forced-pre-ready admission test passed separately in `boot-admission-green.log`. Final settings-cancellation rerun is in `checkpoint-settings-cancellation.log`. |
| `cargo test --locked -p iced_runtime --lib clipboard::tests` | 2 passed (actual task/native action acknowledgement, cancellation and Debug redaction). |
| `cargo test --locked -p iced_runtime --lib system::renderer_tests` | 1 passed; Ready plus Failure survive a queued initial pinned Task scheduling yield. |
| `cargo test --locked -p iced_winit --lib` | 6 passed after readiness change; finite retry/OOM/surface reset, one sink, native clipboard mapping and neighboring IME tests. |
| `cargo clippy --locked -p iced_runtime --lib -- -D warnings` | Passed. |
| `cargo clippy --locked -p iced_winit --lib -- -D warnings -A deprecated` | Passed; existing try_next deprecation warnings remain disclosed. |
| `cargo check --locked --example startup_lifecycle` | Corrected targeted check passed; final cleaned alias check in `checkpoint-startup-example.log`. |
| `cargo check --locked --bin pike --target x86_64-pc-windows-msvc` | Earlier targeted cross-check passed, before last readiness/test polish. This is not native Windows verification; final integrated cross-check remains pending. |

The complete root `cargo test`, root `cargo clippy --all-targets -- -D warnings`,
merged-main checks and review were not completed before owner stop. Unused new
presentation methods would currently fail root warnings-as-errors until the
agreed integration. Neither renderer-profile-wide acceptance nor physical input,
monitor, native Windows, release memory/CPU or latency claims are made.

Raw red evidence remains: first facade clipboard API compile failure; Clone vs
Copy renderer test compile failure; initial first-frame disposal ordering;
readiness test assumptions corrected against pinned validation/pool semantics;
pinned Task's intentional initial yield; old startup probe import/path failures.
`runtime-final-tests.log` records a RED three-test run, followed by the corrected
single renderer-test GREEN; it is not presented as a final all-three green run.
Raw Cargo logs retain normal final blank lines; a command-scoped whitespace check
excluding blank-at-EOF passed before commit without modifying those artifacts.

## Actual native window/PTY evidence

Build: `CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR="$PWD/target" cargo build --locked
--manifest-path docs/baselines/issue-34/native-fixture/Cargo.toml`.
Run each mode with `target/debug/pike-recoverable-native-fixture MODE FRESH_OUTPUT`.

| Mode / artifact | Actual observation |
| --- | --- |
| Original first-frame pre-ready RED | Err with one handle but zero completions at return; child disappearance alone was insufficient native disposal proof. Preserved in `native-first-frame-initial`. |
| Production gate, `first-frame` | Forced pre-ready New/NewTab/NewProfile and late settings create zero children before first-frame error. `native-first-frame-forced`. |
| `preparing` | One Starting session at arm; Err after one actual completion and pool cleanup; no live child. `native-preparing-gated`. |
| `attached` | Err after both real native handles completed and pool cleanup; both children gone. `native-attached-gated`. |
| `config-cancel` | Settings pending at arm; no late launch; pool cleanup before Err. `native-config-cancel`. |

These are synthetic **macOS actual-window and real-PTY** checks, not Windows or
physical keyboard/monitor checks. At wind-down all 6 retained controlled child
PID files were checked with `ps`; no PID remained live. Ordinary test helpers
awaited their owned pool/backend cleanup. Build caches were not deleted.

## Explicitly unfinished work / gates

1. One deliberate integration with actual merged A32/B33 main, preserving A32's
   configure_terminal/owned clipboard lease and B33's merged visual token APIs.
   No unmerged imports, command mapper, focus/capture/gesture edits were added.
2. Insert `error_view`, `error_height`, `passive_error_view` into actual pane/root
   composition, reserving space without obscuring a prompt; show a truthful empty
   boot view. This is why the current presentation helpers are unused.
3. Wire paste retry through A32's fresh captured-owner lease/reservation/generation;
   a SessionId-only check cannot reject an owner round trip. Finish typed config
   adapters and the agreed command_refused hook after actual32. Command/config
   legacy notices still remain. Application-level clipboard retry also needs
   its final view/control integration.
4. Native resize errors are unexposed: Unix OnResize calls die! after failed
   ioctl, Windows OnResize discards the HRESULT after warning. This is in full34
   scope per coordinator, but was NOT implemented after the owner's stop.
5. Paused renderer plus an existing cleanup timeout has no visible rendered
   Wait/Force UI. The proposed fixed native fallback below is NOT implemented.
6. Complete actual checks after those changes, appropriate Windows fixtures,
   source/evidence identity update, one exclusive reviewer, fixes and focused PR.
   Preserve physical13/14 holds and owner14 dependency waiver; no Actions/CI.

## Pending design only — do not mistake for code

The submitted resize plan was an additive fallible OnResize hook, checked
resize-operation serial, typed size/error result through the existing event loop,
one coalesced bridge slot, source-checked app failure, and explicit one-shot
retry of current desired geometry. A resolved notice would retain its height
until dismissed to avoid success/error banner geometry oscillation. Proposed
native fault injection was fixture-only, per owning existing notifier; no other
PTY, automatic retry or production injection. This needs renewed authorization
and review before implementation.

The submitted stalled-renderer plan reused existing Wait/Force policy/tokens,
one prompt per valid timeout, bounded existing executor, fixed Windows dialog
choices with Wait default, cancellation on actual cleanup, stale/duplicate reply
rejection and metadata-only unavailable fallback. No forced closure or periodic
rearming would occur. API investigation identified additional work before that
plan is safe: [TaskDialogIndirect requires common controls v6](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-taskdialogindirect),
and [callback handle lifetime](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nc-commctrl-pftaskdialogcallback)
must be synchronized through destruction/cancellation. Activation/import safety,
callback cancellation races and physical Windows behavior remain unimplemented.
No Win32_UI_Controls feature or native dialog lifecycle code was added.

An additional source finding for later triage: iced_term's link opener currently
panics on failed open; no URL/string was logged or retained in diagnostics by
this increment. Do not silently claim all lower-level I/O error paths are done.

## Orchestration limitations preserved

Exact prescribed heartbeat, ask and escalation commands were rejected by the
native runtime: “This terminal is a Dispatch worker and cannot report as
term_5591e01d-a76f-41c1-8f97-7eab89216678; run the command from the worker's own
terminal. No effects were applied.” Status/check/ack commands worked and were
used as directed by root. IDs/capabilities were never substituted or retried
blindly; root received the actual errors. Final worker_done is attempted exactly
once with the live preamble IDs and truthful failed outcome; any actual rejection
must be forwarded through ordinary status before stopping.
