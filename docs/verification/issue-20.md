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
| TextAreaSizeRequest | Format aggregate virtual-grid pixels, rounding rows/columns times fractional logical cell metrics once; native integer cell metadata and physical DPI are separate |
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

The actual window owner creates one `WindowFocus`, shares it with all of that
window's terminals (including hidden/new sessions), and records real Focused/
Unfocused events. Default/unknown focus is unfocused. Bind it with
`Terminal::set_window_focus` before showing a terminal. The current single-window
application retains this authority during asynchronous startup and binds it on
attachment; no workspace UI is added.

`Terminal::set_focused(bool)` supplies pane focus, gated by the shared window
state and visibility. Backend state deduplicates transitions and sends CSI I/O
only with DEC private mode 1004 enabled. Enabling the mode is not a transition.
Hiding a focused session emits its one focus-out report before releasing the
viewport; set_visible returns any scoped admission rejection.

The widget updates that same window authority from real window events. Neither
terminal-ID diff nor new tree creation invents window focus. Mouse/operation
focus stays transient per widget, and report dedup stays session-owned. No
polling or independent report queue is installed.

CSI 14t uses virtual logical-grid pixels, matching the existing cell units:
80 columns with 8.5-pixel cells report width 680. `TextAreaSize` carries aggregate
u32 dimensions independently of native `WindowSize`; products are computed in
f64, rounded once and bounded to 1..u32::MAX. PTY resize/count dedup is unchanged.
Monitor rescaling does not multiply these virtual units or reshape fonts.

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
to the fixture phase and require exactly 17 palette, one pixel-size and one
character-size request per child on BOTH platforms. Native Windows source
`31aa375` passed these counts, focus and drained exits; the initial fixture
failure counted out-of-phase console negotiation. No query-byte/order/origin
oracle was relaxed. Review corrections described below require fresh native runs.

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
example tests, 83 adapter tests and the Alacritty suite (149 unit, 45 reference,
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

## Initial independent review corrections

Review reproduced a false CSI-I after Window::Unfocused on A, replacing the ID
with B, programmatic focus and redraw without a real Window::Focused. The shared
owner-bound WindowFocus fixes replacement and completely new trees. The public
real-renderer widget probe now enables mode 1004 in its controlled children and
checks admitted bytes: no report for unknown/unfocused replacement/new trees,
exactly one CSI-I for real focus/normal focused creation, no repeated redraw
report, and one CSI-O for actual blur. Its children consume these synthetic
reports until requested cleanup.

The other reproduced bug was per-cell rounding: 80 times rounded(8.5) reported
720 instead of 680. Aggregate TextAreaSize removes that conversion from protocol
formatting. A direct VT regression checks 680x320 and another fractional grid;
the two-session release probe now uses 8.5-pixel cells in session 20002 and expects
exactly 80x20 characters / 680x320 virtual pixels. The widget event probe verifies
that Rescaled(1.5) leaves protocol pixels unchanged, with no font/grid/native
resize work. These fixes do not claim physical desktop/DPI verification.
