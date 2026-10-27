# Issue #21 — steady cursor presentation

Base: merged main `e22731779647d04c14a968323f8f95ed62496ba8` (#12/#18/#20 and
#15 are merged). No dependency, feature, lockfile, PTY, input/copy or protocol
implementation change is included. The authoritative AGENTS.md, issue #21,
Iced 0.14, iced_term 0.8, Alacritty 0.25.1 and VTE 0.15 sources were inspected.

## Presentation policy

Blinking is off by default (the pinned CursorStyle default). All requested
blinking block/underline/beam variants render as their steady counterparts.
The existing R20 `CursorBlinkingChanged(bool)` action still describes the
emulator request; it does not promise animation. No cursor timer, subscription,
background task, polling or periodic redraw exists.

Visible focused cursors use the emulator's Block, Underline, Beam or HollowBlock
shape. Inactive panes/windows use a hollow block; Hidden/show-mode-off produces
no cursor. Effective focus combines the R20 backend state with its authoritative
owner-bound window focus. Focus changes invalidate paint without copying cells
or measuring fonts. An owner-focus change is also detected at an existing draw,
so cached block paint cannot survive a focus loss. Paint focus belongs to the Terminal canvas cache,
so replacement widget trees detect focus lost while no view existed. Initial
empty caches need no explicit clear. A public replacement-tree regression caught
the initial widget-local false-default key retaining the old block; it passes
with the cache-owned key.

All cell backgrounds precede cursor fill and glyphs. R18 already established
this ordering on the merged baseline; this change preserves it. Cursor geometry
normalizes a wide continuation to its lead and spans both columns. Combining
text retains its single-cell span. Pane/ancestor intersection and per-Unicode-
cell clips stay in effect, including the pinned tiny-skia mask workaround.

OSC12 cursor color, or the configured foreground when unset/reset with OSC112,
is used when its contrast with the effective cell background is at least 3:1.
Otherwise black or white with the greater contrast is used. A block's covered
glyph uses the effective cell background when contrast against the cursor is
at least 4.5:1, otherwise the better of black/white. These ratios use sRGB
relative luminance; translucent DIM/inverse/selection backgrounds are composed
over the container background first. This deliberate fallback keeps explicit
equal-color choices visible. Underline/beam/hollow cursors retain normal glyph
color when legible against the cell background, otherwise use the same 4.5:1
fallback. Application-cursor keyboard mode does not determine cursor color.

The snapshot carries shape, OSC12 override and focus metadata. Raw Unicode
buffers are keyed by viewport snapshot and font-measurement generations; colors
are resolved at draw from the current cell/selection/palette/cursor. Paint-only
focus/theme changes reuse shaping and cannot retain an old block glyph color.
Caches remain bounded to one viewport and drop with widget identity changes.

## Public and model regressions

Four adapter tests use actual VTE parsing for shapes 1–6, default steady mode,
hide/show, alternate-screen, inverse/selection/equal colors, OSC12/reset,
wide-continuation/combining geometry and a scrolled-offscreen cursor.

`steady_cursor` runs the existing real PTY backend with a controlled synthetic
child and the public TerminalView/headless renderer. Seven scenes cover ASCII,
upstream PR86 inverse text, Micro-like colored rows, selection, CJK continuation,
combining accents and equal foreground/background. Six style requests at four
scales (100/125/150/200%) give 168 pixel checks per renderer, including hidden
comparisons, steady blink-pair equality, non-block glyph preservation and partial
ancestor clipping. Installed Menlo (macOS) / Consolas (Windows) is explicit,
matching R18's installed-font fixture; this is not universal font-availability
coverage. A matched untouched-main wgpu control with generic
`Font::MONOSPACE` reproduces the historical hidden CJK absence byte-for-byte
(frame SHA256 `263c2c90dbfcb9d100f38b0692af8a4c469e0be9baf65e13206000e5b5d0b574`),
while explicit Menlo renders it. The generic family resolves to a GB18030Bitmap
fallback with infinite shaped advance; Menlo uses a finite Arial Unicode MS
fallback. Fresh/prior-case processes, theme and font-switch controls reproduce
it. This is inherited generic-family shaping/render behavior, not a newly
introduced cursor regression or an unavailable glyph. The old intermediate
candidate binary's provenance is unverified; identical frames establish image
and baseline observation equivalence, not source identity of that old binary.
No unrelated font fallback repair is included.

A separate state-only path sends DECSCUSR, hide/show, OSC12/reset and alternate
screen sequences without printable text or resize. It consumes the normal
subscription Wakeup, with no explicit refresh, then checks repaint and unchanged
printable cells. Host focus/palette changes check zero snapshots and zero font
measurements; selection and Unicode glyph colors are checked on the public widget.

Seven idle phases each wait 200ms, then inject 32 external frames: focused,
unfocused, restored, zero-layout minimized/unminimized, hidden and visible.
Both local renderers observed zero cursor redraw requests, font measurements,
snapshots and cache clears throughout each steady interval. Transition columns
separately record work: focus loss/restoration clear paint twice, hiding once,
activation takes one snapshot and two paint clears; fonts remain unchanged.
These finite offscreen observations do not measure whole-app CPU/RAM budgets or
physical OS minimize/monitor behavior. Zero layout simulates minimized drawing.

## Actual Micro and baseline provenance

Reviewed [upstream PR86](https://github.com/kemokempo/iced_term/pull/86) and
[issue100](https://github.com/kemokempo/iced_term/issues/100). The synthetic
inverse recipe is `X\x1b[7mY\x1b[27mZ\x1b[2D`; it is explicitly synthetic.
The official [Micro v2.0.14 release](https://github.com/micro-editor/micro/releases/tag/v2.0.14)
is used with isolated config and synthetic text, never personal editor data.
Official `internal/display/bufwindow.go` uses the cursor-line style's foreground
as the row background. Two actual editor scenes use cursor-line `#305070`
(colored) and `#eeeeee` (equal to the text foreground), with default
`#eeeeee,#102030`. Micro runs inside the existing PTY inherited by a wrapper;
there is no second PTY implementation. Real Ctrl-Q must exit0 with drained
backend completion, and native wrappers check all five child/wrapper PIDs absent.

Independent untouched-main runs found ordinary/default Micro and the corrected
colored row already legible in both renderers. The equal-color custom theme loses
the covered glyph on main (one interior RGB color); the candidate preserves it.
Do not describe ordinary Micro defaults as a newly fixed regression. Actual
Micro selection in the baseline emits inverse text but hides its hardware
cursor, so a shown cursor over an inverse selection is covered by the synthetic
VT/public-widget tests, not claimed as an observed actual-Micro state.

Official archive SHA256:

- macOS arm64: `e96d99cd09bde96dba3eb93e044e649fbbcf62dee63469510fbd054e1ee8f230`.
- Windows64: `ef95e181249d0547942558d65b73cf484d6e06490bcfc0c540085a04f1ee00f8`.

Extracted executable SHA256:

- macOS: `01f378e30b0c2f56ff70a54e98dedd157a51e806d9da099ce2cf24308c5a055f`.
- Windows: `ee67c64817a622d90e2d6aa3c414765000cb110a4d69bc36cee968d9afe4d4b9`.

## Verification commands and local results

Run with `CARGO_BUILD_JOBS=4`, a worktree-owned `CARGO_TARGET_DIR`, locked
manifests and both renderer feature sets. Ordinary application tests do not
execute patched dependency suites:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --no-default-features --features renderer-wgpu -- -D warnings
cargo test --locked --no-default-features --features renderer-wgpu
cargo test --locked --examples --no-default-features --features renderer-wgpu
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo test --locked -p alacritty_terminal --lib
cargo clippy --locked -p iced_term --all-targets --target x86_64-pc-windows-msvc --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
cargo build --release --locked --example steady_cursor --example unicode_cells --example widget_geometry --example terminal_protocol --no-default-features --features renderer-wgpu
target/release/examples/steady_cursor FRESH_OUTPUT PINNED_MICRO_EXE
```

Repeat with `tiny-skia` replacing `wgpu`; run all four release probes with fresh
output paths. Local macOS arm64 results: fmt, both root/application/example
suites, both denied-warning root/adapter Clippy runs, both Windows-target root/adapter
Clippy runs, full Alacritty suite, actionlint and all eight release probe runs
passed. Cross-target Clippy is compilation evidence only. Both actual Micro
scenes passed in each cursor probe, exiting naturally0/drained.

Explicit macOS adapter suites have pre-existing 5-second cleanup timeouts:
candidate93pass/6fail/1ignored in each profile; untouched
`e227` archive with identical commands/host has89pass/6fail/1ignored in both.
The four new deterministic cursor tests pass. Cancellation/startup and some
work-stat tests fail while awaiting cleanup, with variable affected cases;
no assertion is weakened and no timeout or Unix lifecycle implementation changes.
Local logs and immutable baseline evidence are retained under the worker's
`target/issue-21-checks` and `target/issue-21-baseline-e227`.

`.github/workflows/steady-cursor.yml` checks the exact PR head on Windows2025,
runs explicit adapter regressions and both renderer release probes with official
pinned Micro and ConPTY runtime, and retains source/compiler/OS/runtime/executable
hashes, screenshots, counters and cleanup evidence. Native CI results are recorded
in the PR; local macOS or cross-compilation never stands in for execution there.
Physical desktop, DPI, keyboard/IME and monitor transitions remain unmeasured;
previously deferred #13/#14 manual checks remain parked. No new manual gate is
introduced by this focused rendering change.

## Native fixture correction

The first native head `47e8569` passed all 12 existing Windows checks, but both
new cursor jobs failed a state-only glyph crop after their 168 main pixel cases.
Bounded diagnostics at `f807448` preserved the original failed verdict/oracles
and captured the actual model and hidden/block/shape frames. Both renderers captured a
case1 beam failure: hide/show/underline commands moved the cursor down rows1/2/3;
the next command scrolled the tested Y offscreen. Shapes were correctly
Hidden/Block/Underline/Beam. The all-black crop was missing scene text, not stale
cursor glyph paint. Original ZIPs and their matching GitHub SHA256 digests are
retained separately from extracted files.

Microsoft Terminal `readDataCooked.cpp` at inspected immutable upstream commit
`a65cafae07502c3ce15f6765867fd05044b41f4d` calls
`WriteCharsLegacy(...newlineSuffix...)` on Enter outside the ECHO_INPUT check
(lines739–767). This supports the observed cooked-input newline/scroll cause;
this inspected source is not claimed as the exact source build of the pinned
NuGet runtime or a captured internal interleaving.

The fixture now uses raw Windows VT input (mode512, like R20's existing fixture)
and a bounded ASCII CR/LF command reader. Actual before/after modes are retained.
Unix retains its existing canonical/no-echo path. Every non-screen-switch
state-only command must preserve the authoritative point and full cell data;
requested shapes and OSC12/reset colors must match the model before the unchanged
pixel oracles run. Two framing tests cover CR/LF/CRLF, EOF and invalid/oversized
commands. No production rendering, protocol, timeout or lifecycle change is
part of this fixture correction. Actual Micro still uses the real editor's input
mode and must pass its separate native pixel/natural-exit/cleanup checks.
