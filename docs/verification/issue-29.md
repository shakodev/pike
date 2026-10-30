# R29 — live tabs

Work in progress on `issue-29-live-tabs`, created from exact merged main
`b29a7756f06007f4b6f6872f93b2d2038b8519fd`. The original R28 branch, commits,
verification reports, native failure receipts and build caches remain intact;
`checkpoint/b-r29-from-r28-394d973` also retains its published final head.

## Model checkpoint

`2bf91fa` adds constant-size unread/bell/exit indicators to the existing ordered
stable TabIds and independent pane trees. Selecting a tab acknowledges its
indicators. Activity requires the current reservation and retained pane, so old
restart owners, removed panes, mismatched request IDs and unattached sessions
cannot mark another tab. Hidden natural exit records status without activation.

Verification ran locally on native macOS, with `CARGO_BUILD_JOBS=4`, locked
dependencies and this retained worktree's own writable `CARGO_TARGET_DIR=target`.
These pure checks require no native PTY or graphics capability:

| Command | Result |
| --- | --- |
| `cargo test --locked -p pike --lib tabs::tests -- --nocapture --test-threads=1` | 4 passed, 0 failed; repeated nested switches preserve order/focus/ratios/owners; flags coalesce; stale owners are rejected. |
| `cargo test --locked -p pike --lib workspace::tests -- --nocapture --test-threads=1` | 16 passed, 0 failed; existing generated transaction, retained limit, close, move, restart and zoom invariants remain valid. |

Both commands reported the pre-existing `iced_winit::try_next` deprecations and
`block 0.1.6` future-compatibility notice. These results were obtained on the
model source committed at `2bf91fa`; no successful suite is repeated merely to
obtain a second receipt.

## Pending integration and verification

The coordinator accepted `Control::NewTab` and `Control::ActivateTab(TabId)` as
the public interface for R26. Shared host/view/resize files remain reserved for
R25 until the coordinator supplies the independently reviewed, merged PR76
checkpoint. No unmerged R25/R26 code is imported.

The prepared finite `live_tabs` example drives the actual App and native Iced
window with controlled children. It will verify public New tab controls,
pending attachment, same-size and nested layouts, remembered widget focus,
launch profile/cwd, exact hidden query replies, coalesced activity, zero hidden
snapshots/presentation work, one activation refresh and hidden exit cleanup.
Its execution, selected host/adapter tests, formatting, Clippy and affected
Windows compilation are pending host integration; they are not claimed passed.

No remote CI was enabled, dispatched or rerun. Native Windows/ConPTY, Linux,
physical keyboard/monitor and release RAM/CPU measurements remain unavailable.
The #13/#14 physical holds and explicit #14 scheduling waiver remain unchanged.
Rename, reorder, overflow polish, command remapping, layout persistence and a
second PTY are outside R29.
