# Issue #14 — legacy keyboard encoding and binding precedence

Based on main `fb9e5fc`, independently of draft PR #62. AGENTS.md and issue #14
with comments (none) were read. The referenced PLAN.md was absent from the
checkout and Documents/Downloads planning folders; the issue is self-contained.
Dependencies #4 and #6 are closed and merged. No layout/session ownership or
PTY implementation changes are included.

## Encoding and interception policy

The published table sent Ctrl+U as 0x51, used COMMAND instead of CTRL for some
arrows, emitted malformed modified F1–F4 sequences, and mapped Alt+Insert to
the wrong key. Character matching also used stale cached modifiers, while an
explicit Ignore action could fall through to text. Copy/Paste with no backend
command did not capture their key event.

The new small encoder follows the
[xterm legacy control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys).
The Ctrl+U correction follows
[upstream PR 89](https://github.com/kemokempo/iced_term/pull/89), head `f331a47`.
Installed Iced 0.14 keyboard events/modifiers, Winit 0.30.13 Windows text and
AltGr conversion, and Alacritty 0.25.1 mode/configuration sources were checked.

Precedence is explicit:

1. Most recently configured matching binding, including platform clipboard
   bindings and mode-specific overrides. Character matching borrows the logical
   key and compares ASCII case without allocating a lowercase String.
2. Committed printable Ctrl+Alt text, and non-ASCII/multicharacter Alt text,
   takes precedence over default legacy encoding. This preserves AltGr and
   Option/dead-key text. Iced has no side-specific AltGr modifier; genuine
   Ctrl+Alt chords with printable text are indistinguishable and follow this
   text policy unless explicitly overridden.
3. Named navigation/function/control keys and ASCII Ctrl mappings use the
   current event's modifiers. Arrows/Home/End use CSI in normal cursor mode
   and SS3 in application cursor mode; modified keys use xterm parameters.
   Ctrl+A–Z map to 0x01–0x1a, including Ctrl+C=0x03 and Ctrl+U=0x15.
   Ctrl+Space/@/2 and punctuation/digit aliases cover NUL, ESC, FS, GS, RS,
   US and DEL. An OS-provided C0 value on a non-Latin layout is preserved.
4. Other committed text is sent once. Alt character input receives one ESC
   prefix; uncommitted/dead keys do not manufacture base-key text. Unmapped
   Ctrl combinations retain OS-provided text. Unbound Logo shortcuts remain
   available to the host without terminal text fallback.

Repeat presses encode once per event. Releases emit no legacy bytes. Enter,
Shift+Enter and Ctrl+Enter send CR by default, or CR LF when the child enables
ANSI newline mode (LNM / CSI 20 h); Ctrl+M remains CR. This follows the
[VT100 Return/LNM contract](https://vt100.net/docs/vt100-ug/chapter3.html).
Ctrl+Tab shares Tab, and Ctrl+Shift+Tab
shares back-tab. Shift+Backspace shares DEL; Ctrl+Backspace sends BS. Legacy
encoding cannot distinguish all modifier combinations. Application keypad,
modifyOtherKeys, kitty event types/releases and IME composition are not added
here; IME and negotiated extended encoding remain #19/#45.

`BindingAction::Ignore` now means **consume without output**, distinct from a
missing binding. The widget captures it, Copy, and Paste even if they emit no
command, so text cannot leak through to the terminal. A host can reserve an app
key with an Ignore binding while it handles the application action separately;
complete app-shortcut routing remains #26. Windows/Linux clipboard defaults are
Ctrl+Shift+C/V; macOS defaults are Command+C/V. Command+H is unbound so
it cannot send DEL while the host handles the shortcut. Ctrl+C stays terminal ETX.

Only platform/custom overrides are retained in a per-session vector; the large
allocated default table is gone. Committed text/custom sequences are borrowed
until R06 admission checks, then submitted once through the existing bounded
writer. Oversized key text returns InputRejected without another input copy.
No production timer, polling loop, worker or new dependency was added.

Default link activation is removed and explicit LinkAction::Open is inert until
#44. The panic-prone external launcher and `open` dependency are removed; the
root lockfile also drops is-wsl/is-docker without upgrading other packages.
Archive provenance files are untouched. Alacritty kitty configuration is
explicitly false during creation/history changes: enable/query requests do not
enable or advertise a protocol the encoder does not implement.

## Checks and end-to-end evidence

- Formatting passed: `cargo fmt --all -- --check`.
- Denied-warning all-target application and explicit adapter Clippy passed for
  wgpu/tiny-skia, on macOS and the Windows MSVC cross target.
- Application/all-example suites passed for both renderers: 39 library, two
  application, five compatibility and 39 source-import probe tests. Two
  compile-fail doctests passed; existing opt-in timing tests remain ignored.
- Release application and keyboard/child example builds passed for both renderers.
- Explicit adapter suites passed for both renderers: 81 passed, one existing
  profiling test ignored. Ten new encoder regressions plus a backend policy
  test cover control letters, modes, modified keys, Unicode/AltGr/dead keys,
  repeat, overrides/Ignore, clipboard defaults and disabled kitty/link behavior.
- Equivalent Cargo graph checks passed for both Windows profiles: renderer
  isolation, disabled vendor defaults, minimal Tokio features, and no link
  launcher dependency. PowerShell guard scripts could not execute on this host.

The `keyboard_input` example exercises the **actual widget update path**, event
capture, command routing, R06 writer and the existing raw child fixture. Each
profile runs 168 literal-oracle cases in all four combinations of normal/
application cursor mode and reset/set newline mode. The child emits each mode
sequence before the ready marker, and the probe checks the emulator flags. It deliberately sends stale modifier notifications before key
events. Each produced command must match its exact expected bytes and session;
consumed, clipboard, unbound host-shortcut and release cases must produce none. It also injects an
oversized text event and checks whole-input rejection and unchanged work counters.

After a final FIFO sentinel, each raw child reports length/hash metadata. Both
cursor modes receive exactly 338 bytes with newline mode reset (including the
sentinel), or 340 with it set. Normal/application mode hashes with LNM reset are
6240427134811137018 / 14833512360581434354; with LNM set they are
15479964774545947290 / 2903191449820718642. All eight review-run raw fixture
PIDs were absent after Requested shutdown acknowledgement.
No arbitrary terminal transcripts or real key input are recorded.

A separate profile-free Vim 9.1 session enters insert mode, inserts `ab`, moves
left through the encoder, inserts `x`, saves and exits. Both renderer runs produce
only the known synthetic file `axb\n` and acknowledge exit code 0. This is a real
full-screen editor check, with synthetic Iced events rather than OS key injection.

On Unix, the fixture configures its inherited PTY with `/bin/stty raw -echo`;
this adds no PTY implementation or production dependency. On Windows its
keyboard mode enables VT input/output while disabling processed/line/echo input,
using the existing console handle. This follows Microsoft's
[SetConsoleMode contract](https://learn.microsoft.com/en-us/windows/console/setconsolemode)
and [VT input documentation](https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences#input-sequences).
The older input-backlog fixture invocation retains its original Windows mode.

Raw reports, synthetic editor output, resource counters and source hashes are in
[the evidence directory](../baselines/issue-14/).

## Release observations

Original implementation: one complete run per renderer on macOS 26.7.1 arm64 / Rust 1.97.1, outside a
debugger. `/usr/bin/time -l` includes renderer initialization, two raw fixtures,
the oversized-event check, Vim and teardown. The renderer supplies the widget
API; this probe does not draw a GUI or measure keyboard-to-render latency.

| Renderer | Wall s | Reported user/system CPU s | Maximum RSS MiB | Max synthetic dispatch µs |
| --- | ---: | ---: | ---: | ---: |
| wgpu | 1.00 | 0.05 / 0.09 | 29.14 | 25.208 |
| tiny-skia | 0.90 | 0.03 / 0.09 | 20.02 | 59.959 |

Dispatch includes constructing/layouting the synthetic view and processing one
key; it excludes later command routing. Values are maxima from these single
small runs, not latency percentiles or Windows observations. RSS is the tool's
reported test-program high-water value, not application idle/private memory.
All matrix key events leave font, PTY/grid resize, cache and snapshot counters
unchanged after initial layout. No application memory-budget improvement is
inferred from this offscreen test.

## Review of PR #63

Review of `b16b6c9` reproduced two defects before applying fixes:

- The inherited macOS Command+H override emitted DEL instead of leaving the
  host shortcut available. It is now unbound; regressions cover absent and
  present event text, including the actual widget capture path.
- Return always emitted CR even after a child enabled ANSI newline mode.
  The encoder now borrows CR LF in that mode. Unit cases cover normal and
  application cursor modes, Shift/Ctrl/Alt combinations and unchanged Ctrl+M.
  Raw fixtures enable/reset LNM before readiness to verify mode propagation
  through the emulator, widget, bounded writer and PTY.

Both renderer suites, explicit adapter suites (81 passed, one existing ignored),
native and Windows-cross all-target Clippy, formatting and release builds passed
after the fixes. No dependencies or production background work were added.

[Review evidence](../baselines/issue-14/review/) preserves source hashes, all four
mode reports per renderer, cleanup metadata and the synthetic Vim result. The
original two-mode reports above remain unchanged. One complete review run per
renderer, including four raw fixtures and Vim, measured:

| Renderer | Wall s | Reported user/system CPU s | Maximum RSS MiB | Max synthetic dispatch µs |
| --- | ---: | ---: | ---: | ---: |
| wgpu | 0.99 | 0.06 / 0.13 | 29.48 | 20.000 |
| tiny-skia | 0.76 | 0.04 / 0.08 | 20.05 | 5.542 |

The same single-run/offscreen measurement limits apply. Native Windows transport,
physical layouts and PSReadLine remain unavailable, so the review does not clear
the native acceptance criteria or authorize treating the issue as complete.

## Reproduction and remaining Windows verification

Build profiles sequentially because Cargo overwrites example paths. Use fresh
directories. On Windows, stage the pinned runtime first:

```powershell
powershell -NoProfile -File tools/prepare-conpty.ps1
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo build --release --locked --example keyboard_input --example input_fixture
& ./target/release/examples/keyboard_input.exe target/keys-wgpu

cargo clippy --locked --all-targets --no-default-features --features renderer-tiny-skia -- -D warnings
cargo test --locked --all-targets --no-default-features --features renderer-tiny-skia
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
cargo build --release --locked --example keyboard_input --example input_fixture --no-default-features --features renderer-tiny-skia
& ./target/release/examples/keyboard_input.exe target/keys-tiny-skia
```

Supply a Vim executable as the second probe argument on Windows to run its editor
check. Without one, `editor.txt` explicitly records that it was skipped. On macOS
omit `.exe`; `/usr/bin/vim` is selected if available. Wgpu needs graphics-device
access even for this offscreen widget API. A successful run has completed.txt,
four summary rows, and matching raw result records.

This development host has no Windows runtime or PowerShell. Cross-target
Clippy is compilation only. **Physical US/AltGr/dead-key layouts and PSReadLine
remain unverified**: before merging, exercise physical key repeat/composition on
the supported Windows layouts and check Ctrl+U/Ctrl+C/navigation in a profile-free
PowerShell/PSReadLine session. These acceptance checks keep the PR draft.

## Native Windows Actions follow-up (2026-10-27)

The current authorization permits Windows GitHub Actions. The existing
`windows.yml` renderer matrix now executes the existing release keyboard probe,
requires Git for Windows Vim for the full-screen editor check, and verifies all
four raw fixture PIDs are absent after acknowledged shutdown. Official actions
remain SHA-pinned, permissions stay read-only, and Cargo uses four build jobs.
No production source, dependency or feature changes were needed for this follow-up.

The workflow retains per-renderer `keyboard-input-*` artifacts even on failure,
with synthetic raw results, capture/byte-oracle summaries, editor output and
commit/OS/executable hash provenance. A successful synthetic run establishes
native ConPTY transport and editor behavior for injected Iced events; it does
not establish physical layout, OS key-event conversion or PSReadLine behavior.
The original macOS evidence above is preserved and is not Windows evidence.
Native run results will be recorded after dispatch; they are not yet claimed.

The first native dispatch (`40486804722`, source `bc21abe`) exposed a raw-child
fixture problem: Rust 1.97.1's Windows stdin wrapper strips a trailing Ctrl+Z
after ReadConsoleW. Depending on read batch boundaries, the Ctrl+A–Z matrix
therefore missed a byte and timed out. The Windows keyboard child now uses
ReadConsoleA directly with the already-selected UTF-8 console code page and no
Ctrl wakeup structure. All byte oracles are unchanged, and the original bounded
input fixture keeps its original reader. See the
[pinned Rust implementation](https://github.com/rust-lang/rust/blob/1.97.1/library/std/src/sys/stdio/windows.rs)
and [ReadConsole contract](https://learn.microsoft.com/en-us/windows/console/readconsole).

The same probe now also starts profile-free PowerShell 7 with its shipped
PSReadLine module and built-in Emacs bindings. It sends Ctrl+U, Ctrl+C,
post-cancellation input, arrows, Home/End and Backspace through the real widget
and ConPTY and checks known returned lines. It does not invoke editing methods
directly or mock ConsoleKeyInfo. The synthetic report records actual PowerShell
and PSReadLine versions. This check remains pending until its native run passes.
