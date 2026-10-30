# R26 — commands before terminal input and stable focus

Verified source: `e60d188f3d039ebfdb6b8c5fd2629dd0b51e6863` on
`issue-26-command-routing`. Actual merged consumer base:
`013690ef60db99438c51773e1e220e77b6382ce7` (R19/R24/R28/R25/R29).
The original `52b878f` work, settled R19 head `618ff1993d386e12ff94fbfebdebd4764e755320`,
earlier reports/failures and own Cargo cache were retained. All verification ran
locally, with personal `CODEX_HOME` verified and explicit personal-home
`codex login status` reporting `Logged in using ChatGPT`.

## Result and acceptance

The existing R25 capture wrapper resolves typed application commands before the
terminal widget can encode input. Its single callback falls back to the existing
resize mapper. It reads live composition first; modal views own input. Exact
event modifiers preserve Ctrl+C, AltGr and dead-key text. Structural repeats are
consumed without another action. Terminal copy/paste/history keep their existing
widget consumers.

Keyboard tab commands reuse merged R29 creation/activation, frozen launch
profiles, admission and remembered focus. Close commands reuse R28 confirmation
and cleanup. Directional selection uses canonical pane geometry and the same
accepted selection control that retires older attachment intent, including an
edge with no adjacent pane. Zoom and bounded font changes use existing model and
font/resize transactions. F1 opens a bounded shortcut reference; Escape/F1 or its
button dismisses it and restores a still-valid remembered session.

A focus lease follows stable SessionId, window and modal authority. Deferred
widget operations check it both before presence traversal and before the chained
focus pass. Background synchronization of an unchanged owner preserves the
lease; pane/modal/blur transitions revoke it. A removed or superseded visible
widget cannot steal focus. Pane headers use the pane grid's one click selection
instead of dispatching a second selection on button release. Window return
requests guarded focus for the active session.

Expected missing-tab, empty/unready-pane, new-tab and split refusals expose a
typed `commands::Error` and actionable text. The existing admission/spawn error
pane and Retry paths remain owned by R29/startup. Failed commands leave prior
sessions and focus usable; a valid command clears its earlier command error.

| Path | Observed evidence |
| --- | --- |
| Ctrl+Shift+O/E; Alt+Arrow | One action, no unintended input; exclusive model/widget owner and typing destination after each direction. |
| Ctrl+Shift+T; Ctrl+Tab / Ctrl+Shift+Tab; Ctrl+Shift+1…9 | Nine ordered tabs and eleven independent children, wrapping selection, remembered split focus, no input to hidden/inactive children. |
| Ctrl+Shift+W/Q | One close request, merged confirmation, repeated close suppression, Escape cancellation and actual widget focus restoration; confirmed pane/tab removal and native shutdown receipts. |
| Ctrl+Shift+Enter | Maximize/restore preserves the owner and split tree; repeated toggle is consumed. |
| Alt+Shift+Arrow | Four resize directions use the existing resize consumer with unchanged owner and no stray bytes. |
| Font Plus/Minus/0; F1 | Real cell geometry grows/shrinks/resets; help owns typing/shortcuts, suppresses repeated dismissal, and restores focus on Escape. Font commands clamp to 10–28 DIP. |
| Copy/paste/history | Real selection copied through Ctrl+Shift+C; exact synthetic clipboard bytes pasted by Ctrl+Shift+V and Shift+Insert; Shift+Home/PageDown/PageUp/End move the real viewport without terminal input. |
| Ctrl+C, AltGr/dead keys, IME, modifiers/blur | ETX remains `03`; printable AltGr including named Space and dead-key text stay text; composition captures application-looking keys and commits once; blurred input produces no action or bytes and return restores typing. |
| Refusal/recovery | Full-host geometry and missing-tab errors preserve ownership/typing and recover through valid activation; pure host capacity refusal invokes the existing 32-session guard atomically; inherited spawn-error/retry regression passes. |

## Local checks

Every Cargo command used locked offline dependencies,
`CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR="$PWD/target/issue-20"` and one renderer.
No full-workspace/both-renderer suite, release benchmark or remote CI was run.
The tests were selected for command routing, deferred focus and affected
IME/paste/tab/modal ownership. Vendor source has no R26 diff, so no new vendor
test suite was needed; merged dependency fixes retain their own evidence.

```sh
cargo fmt --package pike -- --check
cargo clippy --locked --offline --no-default-features --features renderer-tiny-skia --lib --tests --example command_routing -- -D warnings
cargo clippy --locked --offline --no-default-features --features renderer-tiny-skia --target x86_64-pc-windows-msvc --lib --tests --example command_routing -- -D warnings
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::commands::
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::focus::tests
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib workspace::tests::directional_focus
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::ime -- --include-ignored --test-threads=1
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::paste -- --include-ignored --test-threads=1
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::closing::ime:: -- --ignored --test-threads=1
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::tabs::tab_switch_invalidates_deferred_ime -- --ignored --test-threads=1
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::tabs::tab_admission_errors
cargo run --locked --offline --no-default-features --features renderer-tiny-skia --example command_routing -- target/issue-26-command-routing/final-native-command-routing
target/issue-20/debug/examples/command_routing target/issue-26-command-routing/focus-race-green --focus-race
target/issue-20/debug/examples/command_routing target/issue-26-command-routing/header-once-green --header-once
```

All passed: 9 command, 4 focus, 2 directional and 8 affected existing native tests,
plus all three actual-host fixture modes. Cross-Windows Clippy passed compilation
of the library, tests and command fixture. Existing iced_winit emits two
deprecation warnings on the native build; changed application targets pass
`-D warnings`.

The final finite fixture drives actual `App::view`, pinned runtime subscriptions,
returned tasks and widget operations with synthetic Iced events. Eleven real
local PTY children record only fixture input. The independent expected byte
streams match all 90 recorded bytes; 175 action/focus observations are retained.
Nine children exit naturally with code 0 and `output_drained=true`. Two confirmed
closes report `Requested/output_drained=false`; those receipts prove requested
cleanup, not a natural-exit drain. The race/header probe cleanup has the same
requested-shutdown limitation.

## Preserved failures and artifacts

[Evidence and provenance](../baselines/issue-26/provenance.json) accompany this
report. The earlier true reds are retained: Ctrl+Shift+O leaked `0f` after ETX
(`030f`) without an app action; an older still-visible focus task overrode the
current owner; one header click dispatched twice. The unchanged focus race was
reproduced again on resume before wiring. Matching green runs are retained.

Development compile failures for the help element's Message/Element conversion
and the initial integrated IME fixture failure are also retained. The IME failure
was fixed in the fixture: a transfer requests a native disable boundary and a
follow-up paint; the finite driver must execute both paints before Opened/Preedit.
No terminal IME rule was weakened. All previous target artifacts/cache remain.

## Changed files and limits

- `src/app/commands.rs` and `commands/host*`: typed mapping, existing consumers,
  bounded help/font/error state and meaningful recovery/priority regressions.
- `src/app.rs`, `focus.rs`, `view.rs`, `close.rs`, `resize*`: one interception
  callback, stable-origin consumption, guarded focus and modal/font integration.
- `src/workspace.rs` and tests: canonical adjacent-pane selection, identity and
  lifecycle invariants.
- `examples/command_routing*`: actual-host byte/action/owner fixture. Cargo files
  expose the already-locked iced_runtime only as a development dependency; no
  package version or production dependency was added.
- This report and bounded synthetic evidence under `docs/baselines/issue-26`.

This is native macOS synthetic-event/real-PTY evidence and Windows MSVC
cross-compilation. Native Windows execution, physical keyboard/monitor layouts,
Windows IME candidates and resource/latency budgets were not measured or claimed.
The #13/#14 physical holds and #14 scheduling waiver remain unchanged; no other
dependency waiver is implied. No tab rename/reorder/persistence, move feature,
backend/PTY rewrite or new background polling was added. Actions readback remains
`enabled=false`; workflows were neither enabled nor dispatched. Independent
review and merge belong to the coordinator/reviewer, not this implementation.
