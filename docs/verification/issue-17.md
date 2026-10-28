# Issue #17 — mouse reports and history

Implementation base: merged main `8d9d8be177db88434e731a0a0557dddb502e7c68`.
The authoritative updated AGENTS.md/PLAN.md were read before implementation;
CI remains disabled. The owner waived unmerged #14 as a scheduling dependency.
This change does not import its keyboard work or alter paste, drawing, protocol
metadata, app/session ownership, dependencies, features or lockfiles.

## Resulting behavior

Line and logical-pixel wheel events use the same positive/up convention. Pixel
deltas divide by the cached logical cell size once; fractional steps accumulate
per pane. Switching tracking/encoding/alternate/Shift routes resets remainders.
Each event performs at most 256 steps per axis, discarding excess from extreme
events. Nonfinite deltas do nothing. There is no polling, timer or new task.

Wheel targets the hovered pane without changing keyboard focus. Tracking modes
1000/1002/1003 send vertical and horizontal wheel reports. Without tracking,
normal-screen wheel scrolls history; alternate scroll sends up/down arrows,
using SS3 in application-cursor mode and CSI otherwise. Shift-wheel always
selects local history and never sends TUI input.

`BackendCommand::ScrollHistory(HistoryScroll)` exposes Lines, PageUp, PageDown,
Top and Bottom for owner/menu callers using the existing stable terminal ID.
Shift+PageUp/PageDown/Home/End bind locally on the normal screen and retain
their existing TUI sequences on the alternate screen. Local navigation uses
the pinned `Term::scroll_display`, including its vi-selection maintenance,
and is bounded by retained history. Public extreme line deltas are clamped
before the pinned grid's i32 addition. Bound/no-op requests do not snapshot.
Output preserves the scrolled content; accepted typing returns to bottom.

A gesture's route is chosen on press. Shift+left drag selects locally, including
when Shift changes mid-gesture; no tracking mode also selects locally. Tracking
presses use left/middle/right button state, 1002 held motion or 1003 any motion.
Click-only tracking does not start a simultaneous local selection. Same-cell
motion is deduplicated. Reports use screen coordinates independent of history
offset, one-based protocol positions, actual Ctrl/Alt/Shift bits, the original
button for SGR release, and legacy release code 3. UTF8/legacy bounds are checked;
encoding flags alone never enable tracking.

Owned moves/releases outside a pane clamp to its final valid cell. Unavailable
cursor release uses the last position. Window blur/leave, moved/resized panes
and effective session focus loss clear transient capture and release held
reported buttons once. Backend held state is session-owned, so hiding/replacing
a widget cannot strand it. Orphan releases do nothing. Mandatory release
admission failure follows the existing scoped protocol-reply failure/close
policy, instead of silently keeping a live peer's button held. R15 selection
remains model-owned. Deferred keyboard, mouse, focus and backend calls retain
their existing order; no eager-input or second queue was added.

## Source review and regressions

Inspected pinned Alacritty 0.25.1 Term/grid/mode APIs, Iced 0.14 event and widget
dispatch, and the existing R13/R15/R18/R20/R21 adapter behavior.
[Upstream PR71](https://github.com/kemokempo/iced_term/pull/71) head
`da0d79c94d1ea945420391b714146523e7b4eaef` establishes the pixel-scroll sign.
[PR85](https://github.com/kemokempo/iced_term/pull/85) head
`38085160c1b4de91fee311b86810ac0d6521ee31` adds 1002 drag handling. Its remaining
click-only local selection is deliberately not copied: local/TUI routes must
not compete for the same gesture.

Explicit adapter tests cover exact SGR/legacy/UTF8 bytes and coordinate limits,
tracking enablement, three-button press/move/release, orphan/duplicate releases,
Shift selection through modifier changes, blur/leave/move cleanup, fractional
line/pixel signs and bounds, hovered unfocused panes, screen/history coordinate
separation, local shortcuts, history page/top/bottom/extreme/no-op bounds,
scrolled output, typing, alternate arrows and per-session cleanup. Existing
identity, authoritative copy, glyph clips and cursor/cache regressions run in
the same affected adapter suite; their production drawing code is unchanged.

`examples/mouse_history.rs` uses the public real widget update/layout/operation
path and a controlled raw PTY peer. A bounded exact byte oracle detects extra
reports as stray input, including queued mouse→key→release order. It checks
fractional line and pixel wheel at a translated fractional pane origin, an
unfocused hovered pane, Shift selection copied as the exact synthetic string,
outside release, blur/leave, alternate arrow modes and history during output.
An explicit optional Vim executable runs with an isolated synthetic vimrc,
receives the actual widget's mouse click, appends to row 2, writes the expected
three lines and exits naturally. Neither probe reads user terminal content.
No render pixels are evaluated: the lean tiny-skia headless renderer supplies
the real widget API; drawing was not changed.

## Local execution

Environment: macOS 26.7.1 (25G241), aarch64-apple-darwin, stable rustc 1.97.1.
All commands used `CARGO_BUILD_JOBS=4` and this worker's existing
`CARGO_TARGET_DIR="$PWD/target/issue-18"`. Refreshed origin/main was unchanged
at the base above before these final relevant checks.

```sh
cargo fmt --all -- --check
cargo clippy --locked -p iced_term --lib --tests --no-default-features --features iced/tiny-skia,iced/tokio -- -D warnings
cargo clippy --locked --example mouse_history --no-default-features --features renderer-tiny-skia -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio
cargo build --locked --release --example mouse_history --no-default-features --features renderer-tiny-skia
./target/issue-18/release/examples/mouse_history "$PWD/target/issue-17/native mouse final" /usr/bin/vim
```

All passed. The adapter suite passed **103 tests**, with one existing opt-in
footprint profile ignored. The native fixture passed 13 case rows; Vim 9.1
(macOS patches 1–1752) wrote exactly `alpha\nbeta!\ngamma\n`, returned code 0,
and acknowledged ChildExit. The raw fixture acknowledged Requested cleanup.
Both recorded PIDs were checked absent afterward. Small synthetic result,
cleanup, environment and source-hash metadata is retained in
[the local baseline](../baselines/issue-17/local/); full generated artifacts
remain under the fresh output path above.

Scope is the changed adapter and new example. No Alacritty source changed;
no full Alacritty/workspace suite or both-renderer matrix was needed for these
event/byte paths. Existing successful checks were not repeated for documents.

## Remaining limits

This is native **macOS** widget/PTY and actual Vim evidence. No current-head
native Windows runtime, ConPTY mouse/TUI run, physical high-resolution wheel,
physical fractional-DPI monitor, touchpad natural-scroll preference or desktop
drag outside a real OS window was available. Injected Iced events establish
adapter behavior, not hardware/OS delivery. No Windows claim is inferred from
earlier issue artifacts or Mac execution. Actions was not enabled, dispatched
or rerun; unavailable Windows/manual coverage remains explicit for review.
No idle CPU, RAM or latency improvement is claimed without measurements.

## PR71 author fix round 1

Independent review of `11f01214afabd120694d6b40f85c8a9e110ea001` found two
blocking counterexamples despite the initial scoped tests passing: the actual
Iced Row could retain focus in both children after a left-child click, and a
legacy release beyond column 222 could be discarded after an admitted press.
The review's public probe sources/evidence were inspected read-only; its old-head
verdict and retained artifacts are preserved. No claim is made that the fixes
have already received independent review.

This round integrates merged main
`713020604d2df41de84d31797eb7128818521eff` (R16 paste). The merge conflicts only
required retaining both the pointer/paste test module declarations and patch
ledger entries. Existing paste behavior remains intact. Main was fetched again
before final validation and remained at that SHA. Issue 13/14 refs, retained
artifacts, physical holds, and the owner's scheduling waiver are unchanged.

The pinned `iced_widget` 0.14.2 Row iterates every child with the same Shell.
Previously, the captured-event return skipped the later child's outside-click
unfocus. The widget now permits focus/release cleanup despite capture, while a
consumed inside click cannot acquire focus or begin a new unowned gesture.
Input remains deferred through the existing ordered BackendCall path; keyboard
encoding, focus/window ownership, selection authority and hovered-wheel policy
are preserved.

Mandatory legacy/UTF8 release encoding clamps to the protocol's last
representable column/row (zero-based 222/2014). This applies both to UI release
and session-owned focus cleanup, including an SGR-to-legacy encoding transition.
Accepted presses therefore terminate immediately at the peer. Normal reports
are unchanged, unsupported presses/motion/wheels are still rejected, orphan
releases remain silent, and admission failures still use the scoped existing
protocol-reply shutdown policy. No local-only clearing, second queue, new task,
production timer or dependency was introduced.

Public regression modules in `examples/mouse_history/` exercise:

- The actual two-terminal Row, public focus operations, one shared window owner,
  and a raw PTY peer per session. Both child orders cover local selection and
  SGR tracking, deferred click→key→release, consumed inside clicks, consumed
  outside cleanup, and hovered-wheel routing. Each plain `k` has exactly the
  expected single session target; the peer also checks exact bytes and absence
  of stray input.
- A 300-column, 230-row pane with legacy releases outside both encoding bounds,
  CursorLeft cleanup, rejected unsupported press/wheel and orphan release,
  and SGR-to-legacy UI release/backend focus cleanup. The peer requires an
  immediate exact release; subsequent releases/blur cannot add bytes.

The new backend test exposed a test-only race in older assertions that compared
currently queued bytes while the real writer could drain them. The affected
mouse tests/fixture now compare accepted bytes (written + outstanding from one
status snapshot), with raw-peer byte assertions retained. The initial targeted
run failed that assertion; the corrected assertions and implementation passed.

### Local checks for this round

All verification ran locally on macOS 26.7.1, aarch64, stable rustc 1.97.1,
`CARGO_BUILD_JOBS=4`, `CARGO_TARGET_DIR="$PWD/target/issue-18"`.

```sh
cargo fmt -p iced_term -p pike -- --check
cargo clippy --locked -p iced_term --lib --tests --no-default-features --features iced/tiny-skia,iced/tokio -- -D warnings
cargo clippy --locked --example mouse_history --no-default-features --features renderer-tiny-skia -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio backend::mouse
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio view::pointer
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio view::state_tests
cargo test --locked -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio view::paste_tests
cargo build --locked --release --example mouse_history --no-default-features --features renderer-tiny-skia
./target/issue-18/release/examples/mouse_history "$PWD/target/issue-17/fix-round1/public-probe" /usr/bin/vim
```

Formatting and both scoped Clippy checks passed. Explicit affected vendor test
filters passed **10 + 9 + 5 + 1 = 25 tests**. The last filter verifies the merged
paste widget path after conflict resolution. The release fixture passed **18
raw/history cases and 12 Row cases**; actual Vim wrote exactly
`alpha\nbeta!\ngamma\n` and exited with code 0. All three raw peers acknowledged
Requested completion; Vim acknowledged ChildExit; all four recorded PIDs were
checked absent. [Retained round-1 results, source hashes and cleanup evidence](../baselines/issue-17/fix-round1/)
identify the source used for these checks. Original round-0 evidence is retained.

Scope remains mouse/focus event routing and byte encoding, so no unrelated
workspace/Alacritty suite, renderer matrix, release-app benchmark, or Rust
rebuild for the subsequent documentation-only update was needed. The source
checks were not rerun after passing without a relevant change. CI stayed OFF:
no Actions enablement, dispatch, rerun, or replacement remote CI.

Current-head Windows/ConPTY, physical keyboard/mouse/wheel, monitor DPI,
touchpad natural-scroll preferences and real OS window leave/release delivery
remain **unverified**. Mac headless public-widget events and raw PTY/Vim results
do not establish those behaviors. No RAM/CPU/latency improvement is claimed.
