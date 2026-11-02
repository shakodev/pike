# R31 — versioned settings and explicit live reload

Author implementation and local verification are complete. The original owner
stop report remains unchanged in [issue-31-owner-stop.md](issue-31-owner-stop.md),
with failed runs, prior evidence, checkpoints and build caches preserved. The
owner resumed this same task; no unmerged source was imported. Independent
Reviewer 2 owns the final review; the author does not merge this PR.

## Source and integration identity

- Original exact base: actual merged R30/PR80
  `9bafb5f4f2686d61c45478667737b433bee634ce`.
- Preserved clean pushed stop checkpoint:
  `e393033daa9411d67ab4bcbff21b225bf11f2202`, containing original product
  `f561096f7140c83c27328e6e0533e14c46c4d905`.
- Fetched/verified actual merged R27/PR79:
  `1e90fb3754d69ec8e60632f6010432bf1273f370`, integrated once in
  `adbcf0fbea283a05da912eadcc98485c069b1440`. Only `app/view.rs` imports
  conflicted; R27 pane-grid extraction, gesture synchronization and focus
  ownership were preserved. Pane/workspace-moving/vendor files are unchanged
  relative to actual main.
- Complete compiled source including the focused combined fixture:
  `d4b410801c2afc60fe1a3a7ddc60b0f145a401c5`, tree
  `1d96770cc741757863943a3dfee308c79a0f842e`. Product source is identical to
  `adbcf0f`; subsequent report/artifact commits do not change compiled code.
- Native combined fixture executable SHA-256:
  `0f178665c8cc388448c8e795d7690bbabb3a07a21f344a5fbb9ae2cdaa37f3b8`.

Dependencies R9 (`081ec9c`), R12 (`0a1bf7f`), R20 (`e227317`) and R30
(`9bafb5f`) are ancestors of verified actual main. Original issue26 branch
and `checkpoint/a-issue26-before-r31-b8cba87` remain at
`b8cba879bcc0fdefa75845309135cd8a84750726`. No refs, reports or caches were
deleted. Personal `CODEX_HOME="$HOME/.codex-personal"` login was verified;
no global/account settings or additional workers were introduced.

## Behavior and acceptance

- [x] Version 1 UTF-8 TOML at `%LOCALAPPDATA%\Pike\settings.toml`; missing files
  select documented defaults without creating anything. Reads are bounded to
  64 KiB. Whole-file validation rejects unknown versions/fields, bad types,
  invalid executable/cwd/args/environment, font/ranges/theme/history/close
  policy and shortcut schema. Errors identify a field or source location without
  echoing values; failed reloads preserve the entire last valid configuration.
- [x] **Reload settings / Ctrl+Shift+F5** uses the one existing typed resolver
  and captured owner, with Help, tab-dialog, close-modal and IME priority.
  Bare F5 stays terminal-owned. No watcher, periodic read, background settings
  service, automatic save or settings editor was introduced.
- [x] Font family/size, terminal palette/application theme and history apply to
  visible, hidden and exited terminal owners without restarting processes.
  Reload resets per-pane font zoom; reducing history discards oldest lines,
  raising it permits future growth. Pending attachments use current appearance.
- [x] Up to sixteen named launch profiles support literal args/cwd/env, with a
  visible profile picker. Stable profile identity is separate from frozen launch
  settings: new/split/restart resolves current settings, with current explicit
  cwd winning and frozen source cwd as fallback. Removed profiles fail
  recoverably without a replacement reservation or silent fallback.
- [x] Conservative close confirmation or `never` uses the existing captured
  session set and cleanup owner. Shortcut data is typed, bounded, normalized
  and validated; **R32 owns applying remaps** through the existing resolver.
- [x] Explicit `Store::save` validates, creates an exclusive same-directory
  temporary, writes/flushes/closes it, then replaces via `std::fs::rename`.
  Broken targets and prior bytes survive failed writes/flush/replace; only the
  caller's temporary is cleaned. No process-global environment mutation,
  terminal transcript or credentials logging was added.

[Settings documentation](../settings.md) records defaults, bounds, profile
examples and exact existing/new-session semantics. Exact pinned Iced 0.14,
iced_term 0.8 settings/font/terminal/backend, Alacritty launch and TOML/serde
sources were inspected. Existing `ChangeFont`, `ChangeTheme` and
`SetHistoryLimit` suffice; no vendor extension was needed. The
[Rust rename contract](https://doc.rust-lang.org/std/fs/fn.rename.html) and
[Windows replacement/ACL contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
were checked: replacement never deletes the target first or falls back to
copy/delete. The Windows sharing-violation regression compiles in the MSVC
check; it was not executed on Windows. Atomic publication does not promise
arbitrary power-loss recovery or synchronization with a concurrently writing editor.

## Local verification on the integrated source

macOS **26.7.2 / 25G332**, arm64; rustc **1.97.1**; default wgpu. Cargo checks
used `CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=target` in this worktree, offline and
locked. Native graphics/PTY checks ran outside the sandbox. Logs, exact commands,
results and hashes are retained in
[resume/provenance.json](../baselines/issue-31/resume/provenance.json) and
[resume/artifacts.json](../baselines/issue-31/resume/artifacts.json).

| Command | Actual result |
| --- | --- |
| `cargo build --offline --locked -p pike --bin pike --example settings_reload --example pane_moving --example command_routing` | PASS at integrated `adbcf0f`; production binary and affected native drivers compiled. |
| `cargo build --offline --locked -p pike --example settings_reload` | PASS at `d4b4108`, including the final combined fixture. |
| `cargo test --offline --locked -p pike --lib settings` | **18 passed**, 0 failed: schema/ranges, malformed/oversized/future version, last-valid state, actual permission failures, interrupted atomic save, broken-byte retention, profiles, close policy and shortcut aliases. |
| `cargo test --offline --locked -p pike --lib app::` | **86 passed**, 0 failed, 9 ignored: affected App/command/pane/launch/tab/dialog/close neighbors. |
| `cargo test --offline --locked -p pike --lib app::tests::tab_management::tab_dialogs_cancel_held_terminal_ime_clones_before_and_after_dismissal -- --exact --ignored` | **1 passed** with native graphics/PTY, explicitly exercising the affected ignored modal IME case. |
| `cargo fmt --all -- --check` | PASS. |
| `git diff --check` | PASS. |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example settings_reload --example pane_moving --example command_routing -- -D warnings` | PASS. |
| Same scoped Clippy command with `--target x86_64-pc-windows-msvc` before `-- -D warnings` | PASS; cross-compilation only. |
| `target/debug/examples/settings_reload /private/tmp/pike-r31-resume-family-adbcf0f` | Complete actual-App/Iced/native-PTY PASS, including the saved explicit font-family extension. |
| `target/debug/examples/settings_reload /private/tmp/pike-r31-resume-joint-1` | Complete acceptance plus focused actual-main pane/dialog/stale-owner PASS at `d4b4108`. |

The current AGENTS.md proportional verification rule supersedes the issue's
older blanket all-target/root-suite wording: checks cover the affected root
library, binary, tests and three relevant native drivers. No full-workspace,
both-renderer, unrelated examples or patched-dependency suite ran; vendor is
unchanged. Eight explicitly ignored App cases remain unexecuted in this round
(including the subprocess helper used by its ordinary parent); ignored status is
not a pass. The complete settings fixture separately exercises close controls
and established terminal IME priority. Unchanged iced_winit deprecation warnings
and the `block` future-compatibility notice remain in logs; Pike's scoped Clippy
passes with warnings denied.

## Native observations

[Combined events](../baselines/issue-31/resume/native-joint-1/events.txt) record
five distinct controlled child PIDs: **14765, 14768, 14774, 14777, 14780**.
Initial A/B profiles used distinct cwd/env; existing children retained PID,
SessionId, reservation, pane target, widget identity and terminal modes across
reload. Font metrics changed from **8.428711 × 18.199999** at 14 DIP to
**12.041016 × 26.0** at 20 DIP, with exactly two measurements per existing
terminal at that checkpoint. Exact OSC10/11 replies confirmed light palette;
saturated history shrank from **80** to **7** and remained capped after new
synthetic output. Original cwd/env/args stayed unchanged; new B tab/split and
restarted A received current profile cwd/env/args.

The focused extension rejects a stale view-generated reload receipt after newer
owner selection, verifies rename taking priority over reload/cancelling an armed
drag and stale dialog edit/dismissal receipts being rejected, then checks fresh
move/max/restore. Reload at 18 DIP/history **0** while maximized updated the hidden
sibling, retained its zero cached viewport, and restored both owners/geometry.
These actions sent no terminal input. Complete acceptance also checks
Help/established IME blocking reload, visible-toolbar invalid reload retaining
broken bytes/last-valid settings, conservative close Cancel, `never` cleanup and
recoverable refusal of a removed profile.

[Cleanup receipts](../baselines/issue-31/resume/native-joint-1/cleanup.txt) cover
the four remaining live children through the App intentional-close path;
`Requested, output_drained=false` proves disposal, not natural-exit output
draining. Original A's natural exit completion was awaited before restart.
The App model was empty after native cleanup acknowledgements. Both resumed
runs passed on their first execution and leave no known fixture work in flight.
The earlier failed development runs and red/green F-key/environment regressions
remain preserved and explained in the unchanged owner-stop report.

## Limits and files

All verification ran locally. GitHub Actions remained **OFF** with no dispatch,
rerun or replacement CI. No native Windows/ConPTY, physical keyboard/monitor,
Windows IME candidate UI, Linux runtime, release RAM/CPU or latency measurement
was performed; no such pass or performance claim is made. Existing R13/R14
physical holds and the R14 owner dependency waiver are unchanged.

Changed files are the settings model/appearance/error/I/O/schema/tests;
`app/settings.rs` and its tests; focused App/startup/tab/split/command/close/view
and theme wiring; `terminal::Entry::terminal_mut`; direct lock-pinned serde and
one TOML format dependency; finite settings fixture and shared fixture theme
response; README/settings docs; and retained synthetic reports/artifacts.
R27 pane/gesture modules, common focus/capture ownership and all vendor code
are unchanged relative to actual main. Independent final review and any merge
decision remain coordinator-owned.
