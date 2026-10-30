# R19 — IME composition without duplicate terminal input

Integrated merged main `be04b8d495fb5af31d77272f39d28c48cf560819`;
final merged-host integration is pending the coordinator's PR72 merge handoff.
PR70's priority current-host error fix was delivered at `38fea554689878241508c9baa35cf215608912e3`
and is part of that merged base; its branch/artifacts remain retained. R19 was
started independently at8d9 and integrated only actual merged main, with no
unmerged PR63/70/72 imports. Updated AGENTS/PLAN guidance applies: targeted local
checks only, no Actions or remote CI. The #14 physical gate is explicitly waived.

## Event, input and lifecycle policy

- `Opened` starts this requested/eligible session's composition. Preedit is local
  UI only, including the empty preedit immediately before Commit. It never sends
  PTY text, Enter, control-key fallback or bracketed-paste markers.
- Composing key presses/releases are captured before bindings: candidate arrows,
  Enter/Space and control/app shortcuts belong to IME until `Closed`. Modifier
  updates remain synchronized. No heuristic text deduplication discards genuine
  repeated characters; native Commit is its transaction's authoritative text.
- One native Commit creates one redacted, owner/epoch-bound deferred token. Its
  UTF-8 is admitted atomically once through the existing writer path when handled.
  Closed clears the preview without invalidating a correctly queued preceding
  commit. Replay cannot send again; wrong-session routing is rejected.
- Explicit cancellation (blur, hide, input-owner/focus loss, replacement, modal
  disable, close or native backend completion) clears preedit and unaccepted
  pending commits. Stale/cancelled token replay is a no-op. Unaccepted text may
  be cancelled by an owner transition; accepted writer bytes retain R06 ordering
  and are cancelled only through the existing explicit close/failure policy.
- Untagged late Preedit/Commit events require a fresh Opened after native reset.
  A shared per-window weak IME owner forces one Disabled redraw followed by one
  re-enable redraw on transition. This prevents old composition state following
  a newly focused session and adds no timer or continuous idle work.
- Outside composition, dead-key events with absent/empty text send nothing;
  completed Unicode and dead-key+Space spacing accents use supplied text.
  Printable Ctrl+Alt (AltGr) text takes priority over control bindings. Ordinary
  Ctrl+C remains ETX and Ctrl+Shift+C keeps the authoritative copy path.

Preview is capped at4096 UTF-8 bytes and invalid byte selection ranges become
no selection, avoiding invalid slicing. Oversize preview is cleared with a typed
error, never silently truncated. Pending commit payloads total at most2 MiB and
256 distinct tokens per session; last-clone drop releases data and the weak
tracking entries are pruned. The existing2 MiB actual-writer budget is checked
atomically at handling, so queueing is not writer acceptance or a reservation.
A failed send admits no prefix. Pending requests are a separately capped input
stage, not terminal output/history or a second PTY/writer implementation.

Errors contain only counts/reason; ImeCommit Debug never exposes text.
`Action::ImeRejected` is displayed by the current host and regression-tested.
No output transcript, environment dump, clipboard access or input logging is added.

## Candidate placement and host contract

The widget requests Iced's input method only during RedrawRequested, using the
current viewport cursor, display offset, logical cell metrics and layout origin.
A wide glyph/continuation protects the full two-column span. The anchor is clipped
inside pane/ancestor bounds and clamped while the actual input cursor is scrolled
offscreen. An empty viewport disables the strategy. No monitor multiplier is
applied in the adapter: iced_winit forwards logical position/size and winit's
Windows backend applies the current scale. The existing R18 glyph renderer is
unchanged; Iced provides the bounded over-the-spot preview overlay.

Each real window must share one WindowFocus and keep widget focus exclusive.
Existing set_visible(false), set_focused(false), window blur and shutdown cancel
composition. Binding the identical window authority again is idempotent. Host APIs:

```rust
Terminal::is_composing(&self) -> bool
Terminal::cancel_composition(&self)
Terminal::set_input_method_enabled(&mut self, enabled: bool)
```

Application shortcut routing must yield while composing. For a modal/input owner,
cancel and disable IME as part of that UI transition; restore it with valid focus
on exit. No current application modal/chrome or shortcut wrapper is invented here.
C confirmed these hooks fit R23's actual WindowFocus/session routes. Future R23
integration must classify BackendCommand::ImeCommit as user input, preserve its
stable origin/visibility gates and display Action::ImeRejected like PasteRejected.
No unmerged R23 code was imported; current thin-host error handling is included.

## Exact source inspection

Pinned sources inspected locally: iced_core0.14.0 input_method/Shell, iced_widget
0.14.2 text_input, iced_winit0.14.1 conversion/window and winit0.30.13 Windows
keyboard/IME/window code. Shell requests are honored on redraw only; preedit
selection is byte-indexed. Windows START emits Opened; COMPOSITION emits preedit
and result/commit events; END handles the Hangul final result before Closed.
Winit's keyboard adapter ignores character messages with no associated key event,
which its source identifies as probable IME. Adapter tests still deliberately
inject composing keyboard fallbacks and verify they produce no second input.

[Upstream73](https://github.com/kemokempo/iced_term/pull/73) was reviewed at
`230945e443c7bded8eac0786240210a51a0e8303`. It provides the basic Iced strategy;
its unmatched-text-only suppression and widget-only flags do not supply the
owner/late-event/cancellation/closed-backend protection needed here. Its rendering
changes were not duplicated: R18 is already merged. APIs were inspected, not
assumed from a newer Iced/winit release.

The same-version local iced_winit0.14.1 patch includes native window scale in
the candidate-area cache identity. The original logical-only cache skipped the
native setter when a stationary cursor moved to another DPI; pinned Windows
winit converts to physical coordinates only inside that setter, and its
WM_DPICHANGED branch does not reapply the candidate area. The existing setters
now run once when scale changes, preserving allowed state and preedit with no
disable/re-enable, timer or polling. Coordinates stay logical in the adapter.

All12 published archive files are retained:11 are byte-identical, with only
src/window.rs changed; the new helper, exact upstream MIT license and
[patch ledger](../../vendor/iced_winit/PATCHES.md) are additional files. Archive
SHA256 is7589888c8e951899cc688247a69933bb7a0511f0b4b2e122ac3fcd5dacb37f72.
Root Cargo.lock changes only the same-version iced_winit registry entry to a
path; no package versions/features change. The excluded path dependency remains
explicitly testable without resolving unused optional workspace-root packages.

## Targeted local verification

All commands used stable Rust1.97.1, `CARGO_BUILD_JOBS=4` and own warmed
`CARGO_TARGET_DIR="$PWD/target/issue-20"`. One tiny-skia profile suffices for this
input/strategy change; no renderer implementation changes. No full-workspace or
both-renderer round, and no redundant documentation-only builds.

Passed:

```sh
cargo fmt -p iced_term -- --check
rustfmt --edition 2024 --check src/main.rs examples/ime_composition.rs
cargo clippy --locked -p iced_term --lib --tests --no-default-features --features iced/tiny-skia,iced/tokio -- -D warnings
cargo clippy --locked --no-default-features --features renderer-tiny-skia --bin pike --example ime_composition -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio ime
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio view::ime::tests
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio ime_anchor_is_logical
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio paste
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio bindings::tests
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio backend::selection_tests
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio backend::history_tests
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio focus_reports_require_mode_and_real_effective_transitions
cargo test --locked --no-default-features --features renderer-tiny-skia --bin pike terminal_errors_are_visible
cargo build --release --locked --no-default-features --features renderer-tiny-skia --example ime_composition
/usr/bin/time -l target/issue-20/release/examples/ime_composition target/issue-19-ime/probe-4
```

The ime substring run passed11 tests: nine IME tests plus two existing name-matched
bridge/protocol tests. The changed widget/anchor subset was checked after final
empty-dead-key/wide-span edge changes. Relevant regressions passed: paste4,
bindings6, selection/copy5, history4 (a mechanical constructor argument), focus
reports1, and current-host error visibility1. Alacritty is unchanged; no unrelated
upstream suite was needed. Formatting, denied-warning lint and source hashes match
retained final evidence.

The public-widget probe uses synthetic Iced composition events with two actual
PTY child byte readers, not native OS-generated IME events. It verifies preedit0,
exact UTF-8/replay-once, composing key/Enter/arrow capture, CJK, dead-key and spacing
accent, empty key text, AltGr, copy/Ctrl+C, old-A commit rejection after B activation,
window blur, modal disable/restore, and idle redraw-request absence. A controlled
VT cursor move changes the candidate from(74.21484,96.649994) to(91.072266,114.85).
Pane movement/resize and synthetic100/125/150/200% Rescaled events preserve correct
logical anchoring. This is API strategy evidence, not a physical candidate box.

[Retained metadata](../baselines/issue-19/) includes current source/executable/lock
hashes, OS/compiler, cursor rectangles, byte counts and cleanup. Final synthetic
sessions accepted12 and3 UTF-8/control bytes respectively, then a separate fixture
finish sentinel. Both children exited0 with output_drained=true and recorded PIDs
23363/23369 were independently absent. Earlier recorded probe PIDs were also checked.

Single final release probe, including renderer initialization, two PTYs, raw setup,
all assertions and cleanup: time-l reported0.56s wall,0.02s user,0.06s system and
17,940,480-byte maximum RSS (about17.11 MiB). These are one Mac test-program
observation, not GUI idle/private bytes, input latency percentiles, long-duration
leak evidence or a Windows/resource-budget claim. An earlier sandboxed profiling
wrapper denied sysctl after its fixture passed; final profiling used authorized
process-counter access, retained separately from that unsuccessful wrapper.

## Unperformed Windows/physical verification

No Windows host or physical keyboard/monitor was available. Native Microsoft
Pinyin/Japanese/Korean event ordering, real dead-key/AltGr layouts, actual Windows
candidate-window/OS overlay placement, monitor transitions and real modal UI were
not run. No Mac test or cross-build is claimed as Windows behavior. The fixture's
existing Windows raw-console setup is available for a future authorized native
local run, but was not executed here. Manual Windows validation must record OS,
IME/layout versions and candidate position, cancellation and byte output. No
Actions were enabled/dispatched/rerun and no replacement remote CI was used.
