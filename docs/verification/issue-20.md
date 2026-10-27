# Issue #20 — terminal protocol and metadata policies

The issue is the self-contained plan; PLAN.md is absent. Work started from
merged #5/#11/#12 main and incorporated merged #13 (`0ae689c`). No dependency,
feature, lockfile, additional PTY or workspace UI integration is added.

## Event contract

Every variant in pinned Alacritty 0.25.1 `event::Event` has an explicit policy.
`Terminal::subscription` retains the originating terminal ID in every
`Event::BackendCall`; callers route it to that session and interpret its Action
in the same ID context. Workspace SessionId/metadata integration remains #23.

| Event | Policy |
| --- | --- |
| ChildExit(code) | Retain actual status and return ChildExit, including while hidden |
| Exit | Return Shutdown { exit_code }; absent status remains None, never invented success; backend completion remains separate |
| PtyWrite | Admit the entire reply through the existing ordered input path without scrolling to bottom |
| ColorRequest | Resolve indexed 0–255, foreground, background and cursor using the current configured palette and parser-ordered OSC override; cursor falls back to foreground |
| TextAreaSizeRequest | Format current applied rows/columns and bounded native cell-pixel metadata without grid copying, font shaping or native resizing |
| Title | Sanitize controls/bidi overrides and bound to 4,096 UTF-8 bytes before model, bridge and application retention; return ChangeTitle |
| ResetTitle | Return ResetTitle; VT reset now emits it as well as resetting the model |
| Bell | Return Bell; bursts coalesce to one pending notification; no sound or host notification is installed |
| MouseCursorDirty | Return MouseCursorDirty and update cached mode metadata without copying cells |
| CursorBlinkingChange | Return CursorBlinkingChanged with the current blink flag, updating mode metadata without copying cells; renderer remains solid with no blink timer |
| Wakeup | Preserve visible-output refresh and hidden-session snapshot suppression |
| ClipboardStore / ClipboardLoad | OSC52 disabled in parser configuration, including after history changes; bridge drops injected requests without retaining payload or consuming FIFO capacity; backend never invokes clipboard formatters |

Invalid/non-query color indices are unsupported and do not invoke a formatter.
User-initiated keyboard copy/paste retains its separate existing policy; OSC52
never reads/writes a system clipboard. Empty titles remain empty; ResetTitle
requests the consumer's default label. Latest-title and bell slots intentionally
coalesce, while controls retain FIFO ordering and reserved lifecycle capacity.

Alacritty's saved-title stack holds at most 64 bounded titles (256 KiB maximum
title payload at pathological saturation); keyboard-mode limits are unchanged.
Sanitizers discard the original String capacity. Title set/push/pop traces no
longer expose titles. Alacritty Event Debug redacts title, reply and clipboard
payloads; this patch logs no terminal transcript or clipboard content.

One theme object serves query and rendering paths so host theme changes cannot
leave a separate response palette stale. Deferred color formatters capture the
model override at the query's parser position: set/query/reset still reports
the queried override when the entire batch was parsed before UI consumption.

Replies/focus reports use the existing whole-input admission and packed ordered
writer. Rejection returns redacted InputRejected and requests only that session's
close; required replies are not silently lost while claiming a healthy session.
Closing cancels outstanding bytes under the existing shutdown contract rather
than promising a graceful flush. Post-close queries are rejected.

## Focus ownership

`Terminal::set_focused(bool)` supplies effective pane AND window focus. Backend
state deduplicates actual transitions against the model and sends CSI I/O only
while DEC private mode 1004 is enabled. Enabling the mode is not a transition.
Hidden sessions cannot gain focus; hiding a focused session emits its one
focus-out report before releasing the viewport. Workspace activation/removal
must route focus/visibility through these APIs.

The widget routes mouse/operation and window focus through the same command,
publishing only when effective state differs. There is no polling or independent
report queue. The geometry probe now routes these focus commands before checking
its gesture/resize messages; no drawing, clipping or cell interface changes.

## Verification and reproduction

Focused tests replay VT through the pinned parser and backend, covering exact
sizes/colors/order, a deferred set/query/reset batch, RIS title reset, focus
mode/transition gating, hidden metadata/status, disabled OSC52 after settings
changes, invalid indices and scoped reply rejection. Bridge regressions verify
strict retained bounds under 10,000-event clipboard/title/bell floods plus
ordered control/lifecycle delivery. Alacritty tests verify sanitized title
payload/capacity/stack limits and redacted Event diagnostics.

The opt-in `terminal_protocol` release probe launches two copies of itself as
controlled real PTY children. It asserts exact ordered bytes for different
palettes and stable IDs, sanitized title/reset/bell handshakes, 10,000 metadata
changes interleaved with ten queries per session, disabled/duplicate/hidden
focus behavior, actual exit codes 41/42 and drained natural completion. Session
20002 starts hidden and receives its own queries and metadata. Probe output
contains synthetic bytes and numeric metadata only. Cleanup is awaited after
preparation/verification failure; completed.txt is written only after success.

The byte oracle covers every reply end to end. Query-route counters are scoped
to the fixture phase and classified separately: all 17 palette queries per child
must reach the adapter, while native Windows may satisfy a size query in the
console host before forwarding output. Native parser/backend regressions also
exercise both size formatters directly. Microsoft's [console query policy](https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences#query-state)
and [console dispatch implementation](https://github.com/microsoft/terminal/blob/main/src/terminal/adapter/adaptDispatch.cpp)
describe host-owned query responses; native artifacts record the observed routing.

Use a worktree-local CARGO_TARGET_DIR and CARGO_BUILD_JOBS=4:

```text
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo clippy --locked -p alacritty_terminal --all-targets -- -D warnings
cargo test --locked -p alacritty_terminal
cargo build --release --locked --example terminal_protocol
```

On Windows stage the pinned runtime into the actual target directory with
`tools/prepare-conpty.ps1 -TargetDirectory <target>`, then run
`<target>/release/examples/terminal_protocol.exe <fresh-output>`.
Repeat application checks/builds with `--no-default-features --features renderer-tiny-skia`
and adapter checks with `--no-default-features --features iced/tiny-skia,iced/tokio`.
The Unix fixture uses system stty raw -echo for its inherited PTY input only.

Local wgpu formatting, denied-warning app/adapter/Alacritty Clippy, application/
example tests, 82 adapter tests and the Alacritty suite (149 unit, 45 reference,
one doctest) pass on macOS. The real-PTY release probe passes both identities,
all 38 queries, 20,000 metadata changes, focus reports and drained exits. Its
first run reproduced the missing RIS ResetTitle, now fixed with a regression.

The isolated [Windows workflow](../../.github/workflows/terminal-protocol.yml)
runs the adapter suite and release probe for both renderers, retaining source,
compiler/runtime/executable hashes, checking both fixture PIDs disappear and
recording externally sampled short-probe CPU/private/working-set metadata.
Native evidence/run links will follow the first execution. These headless
observations do not establish GUI idle budgets, input/render latency or physical
Windows desktop/DPI behavior; none of those is claimed.
