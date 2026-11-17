# PR #63 integration review — 2026-11-17

Reviewed legacy keyboard input from `dc64541c05773d26a9706b4ec7ec91ee883bf184`
against main `f1737cf`. The owner explicitly requested review and merge,
superseding the earlier draft/unmerged hold. Physical checks remain pending;
issues #13 and #14 should remain open for that follow-up.

## Integration changes

Resolved conflicts in the root manifests/lockfile and adapter backend, bindings,
module declarations and widget. Main's session ownership, pointer/mouse handling,
IME composition guard, ordered bounded paste and authoritative selection copy
are preserved. Native encoding now supplies the host-delegated history and
clipboard fallback keys; local history/clipboard defaults are restored when host
delegation is disabled. Explicit overrides retain newest-match priority through
both host-mode transitions.

Retained named-Space dead-key/AltGr commits and unidentified committed text.
Named navigation keys cannot become AltGr text. Added regression coverage for
these cases and for host delegation, history modes and override restoration.
IME widget expectations now include the R14 legacy Ctrl+Space/Alt+Space bytes.

Updated both keyboard probes for main's explicit window-focus authority. The
headless byte fixture declares its synthetic focused window and delegates host
shortcuts; the native-window fixture observes actual focus/blur events. Empty
paste now checks main's typed ClipboardUnavailable rejection without changing
any byte count/hash or deadline.

No blocking implementation findings remain after integration. No production
polling, task, buffer, PTY implementation or resolved dependency package was
added by the integration.

## Local verification

All commands ran locally on aarch64 macOS with Rust 1.97.1 and the pinned lockfile.
Commands used `CARGO_BUILD_JOBS=4` and an existing target cache. Checks selected
one renderer for runtime fixtures because the encoder, widget dispatch, input
writer and host routing are renderer independent. Both Windows feature graphs
were inspected, and the changed Windows examples were cross-checked with wgpu.
No GitHub Actions workflow was enabled, dispatched or rerun.

- `cargo fmt --all -- --check` and `git diff --check origin/main`: passed.
- `cargo test --locked -p iced_term --no-default-features --features iced/tiny-skia,iced/tokio --lib keyboard::tests`: 12 passed.
- Same adapter command with `bindings::`: 6 passed; with `view:: -- --test-threads=1`: 30 passed, including IME, paste, pointer and state integration.
- Same adapter command with `mvp_never_enables_kitty_modes_or_launches_links`: 1 passed.
- `cargo test --locked --no-default-features --features renderer-tiny-skia --lib app::commands`: 19 passed.
- `cargo clippy --locked -p pike -p iced_term --no-default-features --features pike/renderer-tiny-skia --lib --bin pike --example keyboard_input --example keyboard_layout --example input_fixture --example command_routing --example remappable_shortcuts -- -D warnings`: passed.
- `cargo build --release --locked --no-default-features --features renderer-tiny-skia` with those five examples: passed (built in two scoped invocations).
- Release `keyboard_input <fresh-directory> /usr/bin/vim`: four 168-case matrices passed, plus admission/capture checks and Vim insert/move/save/exit. Counts/hashes below are unchanged. All four raw child PIDs were checked absent after acknowledged shutdown.
- Release `command_routing <fresh-directory>` and `remappable_shortcuts <fresh-directory>`: passed with acknowledged cleanup. These exercise the actual App/Iced/PTY route, custom shortcuts, released native chords, clipboard routing, help, focus and stale-input rejection.
- `cargo tree --locked --target x86_64-pc-windows-msvc --no-default-features --features renderer-<profile> -e normal,build --prefix none --format '{p}|{f}'`: both profiles passed native-backend/renderer-isolation assertions. wgpu includes DX12/Vulkan/WGSL, without defaults/GLES/WebGL/glow/khronos-egl; tiny-skia has no wgpu. Adapter defaults remain disabled.
- `cargo check --locked --target x86_64-pc-windows-msvc --no-default-features --features renderer-wgpu --lib --bin pike --example keyboard_layout --example keyboard_input --example input_fixture`: passed. This is cross-compilation checking, not native execution.

| Cursor/newline mode | Cases | Bytes | FNV hash |
| --- | ---: | ---: | ---: |
| normal | 168 | 338 | 6240427134811137018 |
| application | 168 | 338 | 14833512360581434354 |
| newline | 168 | 340 | 15479964774545947290 |
| application + newline | 168 | 340 | 2903191449820718642 |

## Limits and inherited failure

The initial all-target Clippy attempt failed compiling the unrelated
`startup_lifecycle` example (97 unresolved imports/methods caused by compiling
`src/lib.rs` as a nested module). The smallest reproducer,
`cargo check --locked --no-default-features --features renderer-tiny-skia --example startup_lifecycle`,
also fails on a separate clean main worktree at `f1737cf`. Those example and app
sources are unchanged by this PR. Scoped changed-target Clippy above passes.
The unchanged vendored iced_winit emits two try_next deprecation warnings.

The initial release keyboard invocation exposed missing synthetic window focus;
a fresh run passed after adapting the fixture. This did not require relaxing
byte oracles. Historical Windows evidence remains historical: no current-head
Windows execution, PSReadLine run, physical keyboard/layout/composition check,
interactive editor check or mixed-monitor DPI/focus/clipping check ran in this
macOS session. The newly integrated native-window focus plumbing is compile
checked, with native execution still outstanding. No new idle memory/CPU budget
or hardware responsiveness claim is made; the earlier wgpu memory limitation
remains separate work.
