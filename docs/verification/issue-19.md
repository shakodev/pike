# R19 — IME composition without duplicate terminal input

Initial baseline integrated actual merged main `e58838a828c9deabaa340f7119d9ce38374b4447`
(R23 host), preserving merged R17 pointer/history changes from `be04b8d`.
PR70's priority current-host error fix was delivered at `38fea554689878241508c9baa35cf215608912e3`
and is part of that merged base; its branch/artifacts remain retained. R19 was
started independently at8d9 and integrated only actual merged main, with no
unmerged PR63/70/72 imports. Updated AGENTS/PLAN guidance applies: targeted local
checks only, no Actions or remote CI. The #14 scheduling dependency is explicitly
waived; the #13/#14 physical validation holds remain open.

The subsequent [named-Space AltGr author correction](issue-19-altgr-space.md)
preserves this baseline and records its separate red/green evidence. It also
integrates actual merged R24 main `609d702ffc93bba95414f0e26a4a463d5e720f70`,
extending its model-active deferred-input gate to IME and verifying visible-sibling
ownership. The historical R24 boundary described below is resolved by that update.

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
C confirmed these hooks fit the host's WindowFocus/session routes. Future
input-owner transitions can use them without changing pane focus semantics.
The merged host now classifies BackendCommand::ImeCommit as user input, checks
the matching stable reservation, live/visible owner and pane/window focus, and
calls ImeCommit::cancel() before every rejection, invalidating all held clones.
The adapter independently checks the opaque token's actual session and epoch.
Action::ImeRejected is shown on its origin like PasteRejected. Public-widget
host tests cover stale raw IDs, removed owners, reservation mismatches, focus,
blur, replay and exact UTF-8 admission. No application shortcut interceptor or
modal exists in this host; composing-key priority remains in the widget.

No unmerged R23/R24 code was imported. Sessions::synchronize, split caller focus
semantics and pointer/history routing are unchanged. C owns R24's separate
multi-pane visibility/focus extension and the model active-session authority
checks for Write/Paste/ImeCommit and stale FocusChanged(true). The coordinator
confirmed this split-specific boundary; R24 must integrate actual merged R19,
keep focus exclusive and yield future app keyboard shortcuts to is_composing().

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

Verification ran locally on macOS with stable Rust 1.97.1,
`CARGO_BUILD_JOBS=4` and own `CARGO_TARGET_DIR="$PWD/target/issue-20"`.
One tiny-skia profile covers the input/strategy changes; no renderer code changed.
No full-workspace/both-renderer round or remote CI was run. Passing checks were
reused unless an actual source/integration change affected them.

After the same-version runtime path registration, the adapter IME filter passed
11 tests (9 IME tests and 2 existing name matches), including bounded preedit,
UTF-8 once-only commits, owner epochs, blur/modal/close and candidate geometry.
After adding host cancellation, its affected owner test passed again. The final
merged-host checks passed 2 IME tests, 2 paste tests and 1 keyboard-focus test;
the graphics-dependent public-widget tests were explicitly included, not skipped.
The IME host matrix tests held-clone replay after stale raw IDs, hidden/ended/
unfocused/blurred sessions, removed owners and reservation mismatches, plus valid
once-only UTF-8. Error tests preserve the origin and input accounting.

```sh
cargo fmt -p iced_term -- --check
rustfmt --edition 2024 --check src/main.rs src/terminal.rs src/app/tests.rs examples/ime_composition.rs
cargo test --locked --offline -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio ime
cargo test --locked --offline -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio ime_cancel_owner_change_and_modal_suspend_invalidate_all_deferred_clones
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::ime -- --include-ignored
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::paste -- --include-ignored
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib queued_keyboard_requires_current_pane_and_window_focus
cargo clippy --locked --offline -p iced_term --lib --tests --no-default-features --features iced/tiny-skia,iced/tokio -- -D warnings
cargo clippy --locked --offline --no-default-features --features renderer-tiny-skia --lib --tests --bin pike --example ime_composition -- -D warnings
cargo build --release --locked --offline --no-default-features --features renderer-tiny-skia --bin pike --example ime_composition
/usr/bin/time -l target/issue-20/release/examples/ime_composition target/issue-19-ime/probe-6
```

Explicit patched-runtime checks also passed and remain valid: runtime source is
identical to saved `2ead558`. Its two deterministic tests exercise the production
cache branch with a fake native setter and real winit coordinate conversion.
100/125/150/200% scales trigger four physical-area updates without another allow
call, repeated identical requests do nothing, and movement/purpose/disable retain
original semantics. Intentionally restoring the logical-only cache fails the
stationary-DPI test; the red/green logs are retained. This is synthetic evidence,
not a Windows native setter/candidate-window observation.

```sh
cargo test --locked --offline -p iced_winit --lib window::ime::tests
cargo clippy --locked --offline -p iced_winit --lib --tests -- -D warnings -A deprecated
rustfmt --edition 2024 --config max_width=80,skip_children=true --check vendor/iced_winit/src/window.rs
rustfmt --edition 2024 --config max_width=80 --check vendor/iced_winit/src/window/ime.rs
```

The runtime retains two upstream `try_next` deprecation warnings in unchanged
lib.rs. Its explicit Clippy run allows only that existing warning class; adapter
and host denied-warning checks pass without an allow flag. An initial runtime
workspace-member attempt was blocked offline by an unused optional sysinfo;
excluding the dependency resolved it without adding packages or changing features.
An excluded dependency cannot be selected with package feature flags, so its
successful explicit test uses the command above.

Relevant earlier checks remain valid for unchanged behavior: key bindings 6,
selection/copy 5, paste 4, pointer 7, widget identity/state 5, paired pointer/focus
cleanup 1 and protocol focus reports 1. They were not repeated as blanket suites.
Alacritty is unchanged. One first post-R17 macOS IME fixture timed out during its
5-second Drop cleanup after body assertions completed. The exact rerun and IME
subset passed, no source/timeouts were weakened, and the final adapter/host/probe
runs did not reproduce it. The cause remains undiagnosed; the retained observation
is a transcription of tool output, not a complete raw failure log.

The final release probe uses synthetic Iced events with two real PTY child byte
readers. It checks zero preedit writes, exact UTF-8/replay-once, composing key/
Enter/arrow capture, CJK strings, dead-key/spacing accent/empty text/AltGr,
Ctrl+C/copy, stale-owner rejection, blur and modal disable/restore. Origin/resize
and synthetic scale events retain preedit and logical anchoring. Controlled VT
output moves the model cursor from (74.21484,96.649994) to (91.072266,114.85).
It requests no continuous idle redraw. This fixture does not exercise the app
host gate; the separate host tests above do.

Both children received exactly 12 and 3 input bytes respectively, plus a separate
fixture finish sentinel, then exited 0 with output_drained=true. Final PIDs
80956/80961 and all ten earlier recorded probe PIDs were independently absent.
[Final evidence](../baselines/issue-19/final-e58838a/) records source commit
`560e32fe576f0feb9f9d5d19d86b2ea57b8d0498`, source/binary/lock hashes, OS/compiler,
commands, candidate rectangles, cleanup, runtime provenance and limitations.
Initial published evidence remains preserved in the parent directory.

The single complete release-probe observation, including renderer initialization,
two PTYs, assertions and cleanup, was 0.62 seconds wall, 0.02 user, 0.08 system
and 17,989,632-byte maximum RSS (about 17.16 MiB). It is not GUI idle/private
bytes, latency percentiles, long-duration leak evidence or a Windows/resource-
budget claim. The final process counters were read with authorized local access.

## Unperformed Windows/physical verification

No Windows host or physical keyboard/monitor was available. Native Microsoft
Pinyin/Japanese/Korean event ordering, real dead-key/AltGr layouts, actual Windows
candidate-window/OS overlay placement, monitor transitions and real modal UI were
not run. No Mac test or cross-build is claimed as Windows behavior. The fixture's
existing Windows raw-console setup is available for a future authorized native
local run, but was not executed here. Manual Windows validation must record OS,
IME/layout versions and candidate position, cancellation and byte output. No
Actions were enabled/dispatched/rerun and no replacement remote CI was used.
