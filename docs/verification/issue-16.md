# R16 — bounded mode-aware clipboard paste

Implementation base: main `8d9d8be177db88434e731a0a0557dddb502e7c68`.
Verified locally on macOS arm64 / Rust 1.97.1, 2026-10-28. The updated AGENTS
policy applies: targeted local checks, Actions disabled, no remote CI round.
The remaining physical #14 checks are waived as a downstream scheduling gate.

## Shared paste contract

`Terminal::paste(&mut self, Option<String>) -> Action` is the menu/clipboard-result
entry point. Ctrl+Shift+V, Shift+Insert and existing platform Paste use the same
backend policy. `None` yields `PasteError::ClipboardUnavailable`; an empty or
fully filtered string emits no bytes or markers. `Terminal::preflight_paste(&str)`
is advisory only and reserves nothing. Hosts must capture the originating stable
session ID during an async read and reject stale/hidden/closing/exited results.
C owns the R23 session-host/menu wiring. The current host also displays every
`Action::PasteRejected` notice so shortcut failures are visible before R23 lands.

| Input | Policy |
| --- | --- |
| Original UTF-8 >1 MiB | Reject before any write, even if filtering would shrink it |
| CRLF / lone CR / LF | One LF per break in bracketed mode, one CR otherwise |
| Tab / printable Unicode, including combining marks | Preserve UTF-8 exactly |
| Other C0/C1 controls and DEL, including ESC and ETX | Strip in both modes |
| Forged ESC[200~/ESC[201~ | ESC stripped; literal `[200~`/`[201~` stays text |
| Bracketed paste enabled | One ESC[200~ + body + ESC[201~ frame |
| Mode disabled | Body only; no markers |
| End of paste | Never append Enter or any other suffix |

Normal mode cannot distinguish pasted newlines from keystrokes: a newline already
in the supplied text becomes CR and can submit a line. There is no promise that a
multiline normal-mode paste is an editable bracketed block. No extra Enter is added.

## Pending requests, ordering and actual admission

A widget publishes `BackendCommand::Paste(PasteRequest)` through the same deferred
application event path as keyboard Write, MouseReport, FocusChanged and close.
A token is owner-bound, Debug-redacted and cloneable with one consumable payload.
A separate finite pending slot holds at most one original <=1 MiB boxed UTF-8
string per session; a second pending request returns `PasteError::Pending`.
This avoids queuing arbitrary clipboard Strings outside a bound and preserves
mixed Iced event-batch order. Original keyboard/mouse/focus handling is unchanged.

**Queued/preflighted is not accepted input.** Ordered handling consumes the request
once, checks current model mode and current encoded-byte credit, then calls the
existing sender once for the whole frame. Its atomic R06 admission repeats the
budget check; success retains writer credit through actual OS writes and uses
ordered 8 KiB chunks inside the existing 2 MiB budget. A stale preflight may fail
without any prefix, marker or accepted-byte loss. No Alacritty code is modified.

Replay/cancel is a no-op. A wrong owner yields `WrongSession`, cancels the payload
and never redirects it. Dropping the last clone releases the pending slot.
Shutdown and backend completion cancel the slot even if a token clone remains.
Stale host events must be dropped/cancelled; explicit `PasteRequest::cancel()`
invalidates all clones. The slot uses a Weak reference so discarded UI messages
cannot leave a retained payload behind. No timer, task, transcript or PTY is added.

The finite pending cap is separate from writer accounting: one MiB pending,
at most one MiB+12 temporary encoded bytes during handling, plus the existing
2 MiB outstanding writer budget/chunk overhead. The platform clipboard's original
returned String allocation is outside these caps. Boxed storage prevents an
oversized String capacity from persisting behind a short clipboard value.

Failures include clipboard unavailable, oversize, pending request, wrong owner,
full/closed input and failed writer wake. Display text contains metadata and useful
retry guidance, never clipboard contents. Rejections do no font measurement,
viewport snapshot, canvas invalidation or history scroll. Success retains the
existing return-to-bottom behavior. Copy extraction and normal Ctrl+C are unchanged.

## Source review

Reviewed [upstream91](https://github.com/kemokempo/iced_term/pull/91) at
`ea95f8edd29fdd4d0e093fd3b528a3ea8902f4ec`, and Alacritty's UI
[paste implementation at the pinned terminal crate revision](https://github.com/alacritty/alacritty/blob/12082407d9cb3384a2bd83c578e556ec3faa2c4d/alacritty/src/event.rs).
Upstream91 strips ESC/ETX only when bracketed and has no whole-paste limit or
budget. Alacritty converts normal-mode CRLF/LF to CR but writes bracket markers
separately. Pike keeps their protocol intent with the stricter explicit policy
above, canonical bracketed newlines and a single atomic frame admission.

## Targeted local verification

All commands used `CARGO_BUILD_JOBS=4` and this worktree's warmed
`CARGO_TARGET_DIR="$PWD/target/issue-20"` (no shared target). One renderer profile
is sufficient: the patch affects input/lifecycle, not drawing or renderer choice.
No full workspace or both-renderer round was run.

Passed:

```sh
cargo fmt -p iced_term -- --check
rustfmt --edition 2024 --check examples/bounded_paste.rs
cargo clippy --locked -p iced_term --lib --tests --no-default-features --features iced/tiny-skia,iced/tokio -- -D warnings
cargo clippy --locked --no-default-features --features renderer-tiny-skia --example bounded_paste -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio paste
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio bindings::tests
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio backend::selection_tests
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio backend::history_tests
cargo test --locked -p alacritty_terminal --lib input::tests
cargo build --release --locked --no-default-features --features renderer-tiny-skia --example bounded_paste
target/issue-20/release/examples/bounded_paste target/issue-16-paste/probe-5
```

Explicit affected adapter tests: four paste/token/widget tests, six bindings,
five selection/copy, four history tests (the only history change is a mechanical
lifecycle test-constructor argument). Eight unchanged R06 queue tests verify the
underlying whole-send, concurrent credit, actual-writer accounting, UTF-8 order,
failed wake and cancellation contracts. Tests are meaningful behavior fixtures,
not snapshots of clipboard content or a production transcript.

The opt-in `bounded_paste` example uses the existing PTY/ConPTY stack and a raw
synthetic child. On this Mac it proves exact normal/bracketed bytes once, both
real widget paste shortcuts and captured events, mouse→focus→key→Ctrl+C→paste
batch order, pending-request mode-off→mode-on changes, Unicode crossing an 8 KiB
chunk, a 1 MiB input, forged controls, no extra Enter, a stale successful preflight
rejected after other writes fill credit, and close ahead of pending paste.
Pending token redaction/ownership/replay/cancel/last-clone release is deterministic
unit coverage. Clipboard reads are synthetic; no real clipboard or user files are used.

[Retained raw metadata](../baselines/issue-16/) includes source/executable hashes,
OS/compiler provenance, per-attempt writer counters/latency, external RSS samples
and cleanup. The fixture reports byte equality as pass/fail without a transcript.

| Single local release probe observation | Result |
| --- | --- |
| Additional 1 MiB attempts while reader stalled | 32, all rejected whole |
| Outstanding bytes during all 32 observations | 2,096,130, constant and below 2 MiB |
| Largest full-paste call, including scan/allocation of caller String | 10.400 ms |
| Close request / completion after request | 0.342 / 1.178 ms |
| External RSS during blocked attempts >=4 | 23,968–26,032 KiB; samples20–31 at26,032 |
| Synthetic child cleanup | All three recorded PIDs absent |

These are short single-run Mac probe observations with external `ps` sampling,
not p95 input-to-render latency, Windows measurements, long-duration leak proof,
private bytes, GUI idle budgets or a resource-budget claim. The full writer counter
plateaus under rejected pastes; the small RSS range includes allocator warming.
Natural byte-fixture exit is code0 with output_drained=true. Explicit stalled/close
cancellation is Requested with unknown code and output_drained=false; unwritten
accepted input is explicitly cancelled rather than falsely reported drained.

## Remaining platform/manual limits

No native Windows host is available in this worktree session; PSReadLine, a real
bracket-aware Windows TUI, native clipboard failure modes and physical keyboard
checks were not run. Mac or cross compilation is not Windows runtime evidence.
The same opt-in fixture includes the existing pinned Windows raw-console setup
and is ready for a future authorized local native run with a fresh output path;
that path has not been executed here. No Actions were enabled, dispatched or rerun.
R23 owns actual session-host/menu integration; this PR supplies its tested shared
paste API and typed errors, plus the current single-terminal host notice handling.

## PR70 host feedback follow-up

Review found the current host handled InputRejected but discarded PasteRejected.
The terminal event handler now puts PasteRejected's metadata-only Display text in
the existing visible notice, preserving the session and input. The focused host
regression routes widget-shaped unavailable/oversized/pending/full/closed paste
commands through `Message::Terminal`, checks visible feedback and unchanged
input/session identity, and confirms stale-session failures cannot overwrite it.

Only this host changed after the adapter/native evidence above; no repeated vendor,
renderer or fixture round was needed. Additional targeted local checks passed:

```sh
rustfmt --edition 2024 --check src/main.rs
cargo test --locked --no-default-features --features renderer-tiny-skia --bin pike failed_keyboard_paste_is_visible
cargo clippy --locked --no-default-features --features renderer-tiny-skia --bin pike -- -D warnings
```
