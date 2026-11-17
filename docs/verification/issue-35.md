# Issue #35 — macOS implementation; Windows acceptance pending

Implemented on `issue-35-keyboard-access`, based on current merged main
`748675fa32b91644593ab498ecdf53967a138ac7`. GitHub main was checked during
implementation and still matched that base. The checkout was fast-forwarded
before verification. The owner's existing Cargo.toml development-profile edit
and untracked `.DS_Store` were preserved; Git retained the integration autostash
as a backup after the manifest conflict was resolved. They are not R35 changes.

## Behaviour and changed files

- `src/app/commands/{reference,recovery,host}.rs`: the F1/effective Help menu is
  the keyboard route to all existing commands and now to passive error recovery
  and opt-in diagnostic copy/clear. Tab/Shift+Tab wraps, Enter/Space activates,
  selected rows have a textual marker and outline, and scrollbars cannot cover
  the selection border. A menu opening freezes stable ErrorIds; removed or
  ineligible actions become inert instead of following a different notice.
- `src/app/errors.rs` and `errors/view.rs`: menu and banners share retry
  availability. Clipboard recovery restores the original input owner, then uses
  the existing fresh, one-use focus/window receipts. No background error can
  paste into the currently focused terminal.
- `src/app/tab_dialog.rs`, `resize/capture.rs`, `close/view.rs`, `tab_strip.rs`:
  rename cycles Name/Cancel/Save with IME priority, follows native modifier
  changes and reveals the selected control in narrow windows. Old focus/reveal
  tasks lose their widget IDs when the traversal stop changes. All-tabs gains
  Delete-to-request-close and immediate native window-focus tracking. Close and
  slow-cleanup actions remain visible below scrollable explanations.
- `src/settings/appearance.rs`, `app/settings.rs`, `ui/style.rs`: explicit
  `high-contrast-dark` / `high-contrast-light` settings, cached Iced themes,
  default terminal text/cursor palettes and visible choice/input outlines.
  Standard dark/light settings retain their original chrome palettes. Existing
  terminal font zoom remains bounded to 10–28 DIP and is exercised here.
- `tools/export-accessibility-captures.py` exports the fixture PPM files with
  byte-preserving PNG scanlines using the Python standard library.
- Scoped regression tests, [user guidance](../accessibility.md), the settings
  reference and README document keyboard routes, limitations and Windows work.

There are no dependency, lockfile, vendor, PTY, runtime or Actions changes.
Terminal SessionIds and widget identities survive the operations exercised by
the fixture. The added menu is bounded by the 64-command catalog, at most three
actions for each of 64 notices, and two opt-in diagnostics actions. It retains
typed action identities, not terminal transcripts. No watcher, polling loop,
continuous accessibility transcript or background accessibility worker is added.

## Local verification

All verification ran locally on macOS arm64 with Rust 1.97.1, locked/offline
dependencies, the default wgpu renderer and `CARGO_BUILD_JOBS=4`. Graphics tests
needed local graphics access outside the filesystem sandbox. They inject
synthetic Iced events and use controlled real Unix PTYs; they are not physical
keyboard, Windows/ConPTY, display or Narrator verification.

The scopes below total **66 ordinary tests plus 5 explicitly selected graphics/
PTY tests**, all passing. Earlier successful unaffected scopes were reused;
the focus fix reran the affected tab tests and graphics cases, and the final
palette guard reran contrast, lint, Windows compilation and the two new graphics
cases. No whole-library/workspace or both-renderer sweep is claimed. Vendor
code was unchanged, so patched-dependency suites were not needed.

| Command (all `cargo test` commands use `--offline --locked --lib` and `--test-threads=1`) | Result |
| --- | --- |
| `cargo test … commands:: -- --test-threads=1` | 21 passed. |
| `cargo test … settings:: -- --test-threads=1` | 20 passed. |
| `cargo test … app::tests::tab_management -- --test-threads=1` | 3 passed; 1 graphics case separately selected below. |
| `cargo test … app::close::tests -- --test-threads=1` | 10 passed; existing subprocess-only fixture remains normally ignored and is invoked by its parent test. |
| `cargo test … app::tests::errors -- --test-threads=1` | 10 passed; four graphics cases separately selected below. |
| `cargo test … delayed_editor_focus -- --test-threads=1` | 1 passed; retained pre-fix failure demonstrates the delayed-focus race. |
| `cargo test … ui::style::tests -- --test-threads=1` | 1 passed. Numeric foreground contrast is at least 7:1 for explicit contrast themes, 4.5:1 for standard chrome, and 3:1 for focus accents over tested surfaces; standard Light is explicitly distinguished from the custom contrast mode. |
| `cargo test … accessibility_ -- --ignored --test-threads=1` | Both new actual-App graphics/PTY cases passed on fingerprinted final source/binary. |
| `cargo test … root_recovery_row -- --ignored --test-threads=1` | 1 passed. Existing actual-root recovery/clipboard ownership regression. |
| `cargo test … passive_banner_reserves -- --ignored --test-threads=1` | 1 passed. Existing banner geometry/dismissal/typing regression. |
| `cargo test … tab_dialogs_cancel_held -- --ignored --test-threads=1` | 1 passed. Existing deferred IME ownership regression. |
| `cargo clippy --offline --locked --lib --bin pike --tests -- -D warnings` | Passed. |
| `cargo check --offline --locked --target x86_64-pc-windows-msvc --lib --bin pike --tests` | Passed; **compile checking only**. |
| `cargo fmt --package pike -- --check` and `git diff --check` | Passed. |

The existing two `iced_winit` `try_next` deprecations and the upstream `block`
future-incompatibility notice remain disclosed in the raw logs; no application
lint error remains. No Actions workflow was enabled, dispatched or rerun.

The new journey exercises creation, split, keyboard divider resize, pane move,
maximize/restore, font bounds/reset, rename traversal/save, modal input isolation,
clipboard failure/retry, copy, error dismissal, overflow-targeted close cancel,
confirmed pane close and focus restoration. Copy starts from a seeded
authoritative terminal selection; no keyboard terminal-text selection is added.
The fixture waits for all retained owned shutdown handles and launch-pool
shutdown. It does not claim an independent OS process-tree census.

Fourteen rendered screenshots cover both contrast themes: command menus at
100/125/150/200% synthetic raster scales and rename/overflow/close at a narrow
320×240 logical viewport and 150% raster scale. Selected-control bounds and
visible accent pixels and selected menu text pixels are asserted. Screenshots were reviewed locally; these
scales do not simulate `WM_DPICHANGED`, physical monitors or Windows text output.

## Evidence and failures retained

[Local receipts and source/binary fingerprints](../baselines/issue-35/) include
the successful scoped logs, the final native provenance, PNG previews and a
hash manifest. The original PPM files and additional intermediate local results
remain under `target/issue-35/`.

The initial sandboxed graphics attempt had no graphics device. Early synthetic
fixture runs used incomplete text-input events (wrong macOS select-all modifier,
multiple characters per key, and missing modifier-change events), exact float
rectangle equality, and a stale-notice dismissal expectation after successful
paste. Those fixture problems and intermediate compile errors were fixed;
available original logs remain retained. The clipping assertion now allows only
0.001 DIP of float representation error. It does not permit a clipped physical
pixel at the tested scales. No resource budget, timeout, byte-count expectation
or product acceptance gate was relaxed.

Visual review also corrected the menu scrollbar covering the selection outline
and the close-dialog footer scrolling away. A separate failing production-focus
regression was written before the rename effect-identity fix and passes after it.

Final image review found that the system PPM-to-PNG converter omitted parts of
menu text even though the raw RGB captures contained them. The new selected-text
pixel assertion passed against raw captures. PNG export now writes those exact
RGB scanlines with no colour/profile conversion; the corrected PNGs were reviewed
again. Regenerate them with
`python3 tools/export-accessibility-captures.py target/issue-35/final-rasters docs/baselines/issue-35`.

## Remaining acceptance

Keep [#35](https://github.com/shakodev/pike/issues/35) **open**. Native Windows
keyboard/clipboard/ConPTY integration, physical input/DPI/monitor checks, Windows
Contrast-theme behaviour and a recorded Narrator/accessibility assessment are
unperformed. Explicit contrast settings do not automatically import Windows
Contrast-theme colours. Terminal ANSI/OSC/TUI-defined colours can override
default terminal colours.

Source inspection found no Pike accessibility tree/UI Automation bridge for
the canvas terminal. This is a static assessment, not a Narrator result or a
claim that chrome is exposed correctly to a screen reader. The remaining
Windows procedure and precise handoff to #46 are in
[Keyboard access and contrast](../accessibility.md).

No release RAM/CPU/startup/latency measurements were taken and no new performance
claim is made. Release budgets remain in #38/#39. The existing physical #13/#14
holds and #34 native-Windows acceptance are unchanged.
