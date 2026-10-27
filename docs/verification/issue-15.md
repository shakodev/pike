# Issue 15: selection extraction and copy

The pinned Alacritty model owns selection and extraction. R11 already replaced
display-iterator concatenation; upstream PR90's actual head
`f5b49232dededd266feafd27f1e2c67c5c3e416c` uses the same `selection_to_string`
delegation. R15 preserves its `Option` and adds one explicit user action.

`Terminal::copy_selection(writer)` returns `None` without invoking the sink for
no selection or empty extracted text. A real selection containing spaces is
copied. Nonempty text invokes the sink once after releasing the model lock,
returning its result unchanged. It never clears selection, changes viewport,
logs copied contents or sends input. No extra grid, transcript, index, cache or
background task is introduced. Full-history string allocation occurs only on
user request and is proportional to the selected retained model history.

Ctrl+Shift+C uses the key event's modifiers, copies once and is captured even
when empty or the sink is unavailable. Command+C remains on macOS. The app's
Copy selection button resolves its captured stable terminal ID against the
current running session; stale IDs and shutdown requests do nothing.

## Deterministic and live checks

`backend/selection_tests.rs` feeds VT bytes to the actual pinned model and
asserts exact strings for hard newlines, soft wraps, padding, real spaces,
blank lines, tabs, reverse/block/word/line selections, CJK continuation cells,
combining accents and wide wrap. Off-viewport history remains copyable with
presentation released. A failing writer verifies both sessions' selection,
input/work counters and that the model lock is released.

`examples/selection_copy.rs` reuses the existing offscreen renderer and real
widget update/operation harness with two controlled PTY children. It checks
eleven exact selections through Ctrl+Shift+C, session tree swaps with existing
selections, no queued terminal commands, unchanged input/work/snapshot
counters, hidden history, unavailable sink and acknowledged child cleanup.
The controlled child accepts only fixture commands; the final readiness
handshake detects accidental copied bytes. Artifacts contain case names,
lengths, counts and process IDs, never copied text.

Run for each `renderer-wgpu` / `renderer-tiny-skia` profile:

```sh
CARGO_BUILD_JOBS=4 cargo build --release --locked --example selection_copy --no-default-features --features renderer-tiny-skia
./target/release/examples/selection_copy /absolute/fresh/evidence-directory
```

Windows workflow `selection-copy.yml` runs both profiles against the exact PR
head on windows-2025, prepares pinned ConPTY, runs adapter tests and executes
the probe with `--native-clipboard`. A message-only owner HWND writes and reads
real CF_UNICODETEXT. Another thread holds OpenClipboard to exercise an actual
failed acquisition, proving prior contents and both selections remain intact.
The isolated fixture uses existing windows-sys dev dependency features only.
Movable memory is transferred only on successful SetClipboardData; guards
close clipboard handles, free untransferred allocations and destroy windows.
The workflow retains source/executable hashes, native compiler/OS provenance,
case counts and child cleanup. This fixture intentionally replaces clipboard
contents only on the disposable runner.

## Bounds and remaining limitations

Pinned `Term::resize` preserves same-size and height-only selections but clears
selection on column changes. The coordinator explicitly retained this upstream
policy instead of introducing a remapping API: after column reflow Copy is a
captured no-op and never revives old text or overwrites clipboard. Tests cover
both preservation and invalidation; arbitrary column-resize preservation is
not claimed.

Iced 0.14's clipboard `write` and clipboard Task return no OS error. The public
action supports a fallible sink and returns its error, and failure never
mutates the session; production UI cannot report an error hidden by that
facade. Native fixtures observe failure directly. The Windows probe exercises
actual OS clipboard storage through the real widget's Clipboard interface;
it does not prove an interactive desktop/Iced clipboard integration, physical
keyboard layout, monitor DPI, PSReadLine or an external editor paste. Those
interactive checks require an available Windows desktop and are not fabricated.

## Recorded execution

Local tiny-skia live fixture and targeted model/menu regressions passed before
base integration. Final profile counts, head SHA and native workflow results
are recorded in the PR after final validation. Cross-target compilation is
only an API check and is never treated as Windows execution.
