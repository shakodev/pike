# PR #85 integration review — 2026-11-17

Reviewed `26ca1d447d90ecbc664494112c9e2681f4d7a4c3` against main
`9b2b43590324e3cecd3ea6fb87b48cda577e8e22`. The owner requested review and
merge of the next PR. R38 / PR #84 depends on this R34 change, so #85 was selected
first. That request supersedes the previous draft/unmerged instruction; it does
not establish a Windows runtime test pass. Keep issue #34 open for native
Windows acceptance, and retain the existing physical #13/#14 follow-up.

No blocking implementation finding remains after integration. Reviewed typed
error ownership/bounds/privacy, view-captured recovery leases, clipboard write
receipts, renderer readiness/failure, cleanup cancellation/execution capacity,
fallible native resize, latest-result slots, UI identity and resource lifetime.
The four previously reported defects are covered by passing current-tree tests.

## Integration changes

- Combine main's pinned native wgpu feature anchor with R34's acknowledged-write
  runtime dependency and patch. The root lockfile remains locked.
- Preserve main's disabled hyperlink opening: the R34 fallible-link action
  remains available as an API, but LinkAction::Open is inert and no launcher is
  restored. Preserve the R14 encoder and existing host input ownership.
- Teach the shared finite command fixture to handle WriteWithResult and
  acknowledge its own synthetic clipboard write. This is fixture behavior,
  not proof of native Windows clipboard acknowledgement.
- Production R34 App, runtime, winit and Alacritty source is otherwise unchanged
  from the reviewed PR. Historical evidence stays intact.

## Verification ran locally

All checks ran on aarch64 macOS with Rust 1.97.1, locked dependencies and
CARGO_BUILD_JOBS=4. Existing target caches were reused. Runtime fixtures used
one renderer (tiny-skia), since error ownership, clipboard routing, resize and
cleanup are independent of rasterization. A separate native wgpu fixture tests
real compositor failure; Windows compilation uses the default native wgpu
profile. No full-workspace/both-renderer test checklist was used. Actions was
not enabled, dispatched, rerun or replaced.

Raw command results and provenance are in
[merge-review](../baselines/issue-34/merge-review/).

| Local check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `git -c core.whitespace=-blank-at-eof diff --check origin/main` | Passed. The unmodified retained logs have terminal blank lines; the ordinary check reports those only. Raw logs were preserved. |
| `cargo clippy --locked --no-default-features --features renderer-tiny-skia --all-targets -- -D warnings` | Passed, including startup_lifecycle (the previously inherited compile failure is fixed by this PR). Rechecked after the command-fixture adaptation. |
| `cargo test --locked --no-default-features --features renderer-tiny-skia --lib app::tests::errors -- --test-threads=1` | 10 passed; 2 graphics cases explicitly selected below. |
| Same command, with `-- --ignored --test-threads=1` | Both graphics/root recovery cases passed, including healthy typing across first/last error and constraint rows. |
| Same root test command, selectors `app::close::native`, `app::boot::`, `executor::`, `native_cleanup::`, `diagnostics::`, `errors::tests`, `errors::writes` | 4 + 4 + 5 + 3 + 2 + 3 + 1 passed. Total selected root tests: 34. |
| `cargo test --locked -p iced_runtime --lib` | 3 passed: actual task/action acknowledgment and bounded ordered renderer events. |
| `cargo test --locked -p iced_winit --lib` | 6 passed: renderer recovery/receipts, clipboard metadata and existing IME contracts. |
| `cargo test --locked -p iced_term --no-default-features --features iced/tiny-skia,iced/tokio --lib resize` | 8 passed, including current/stale resize receipts and actual PTY input/cleanup. Same command with `mvp_never_enables_kitty_modes_or_launches_links`: 1 passed. |
| `cargo test --locked -p alacritty_terminal --lib resize` | 4 passed. The local-socket case was initially sandbox-denied; the same scope passed with required socket access. |
| `cargo clippy --locked -p pike -p iced_term -p alacritty_terminal -p iced_runtime --no-default-features --features pike/renderer-tiny-skia --lib --tests -- -D warnings` | Passed. |
| `cargo clippy --locked -p iced_winit --lib --tests -- -D warnings -A deprecated` | Passed with the existing try_next deprecation allowance. |
| `cargo check --locked --target x86_64-pc-windows-msvc --lib --bin pike --tests` | Passed; compile checking only, including Windows ConPTY/dialog source. |
| `cargo build --release --locked --no-default-features --features renderer-tiny-skia --example command_routing --example remappable_shortcuts --example keyboard_input --example input_fixture` | Passed; the two host examples were rebuilt after their shared fixture adaptation. |
| Release `command_routing <fresh-dir>` and `remappable_shortcuts <fresh-dir>` | Passed with owned cleanup receipts. |
| Release `keyboard_input <fresh-dir> /usr/bin/vim` | Four 168-case byte matrices and Vim passed. Counts 338/338/340/340 and hashes remain unchanged; all four raw child PIDs were independently checked absent. |

The 34 root, 9 adapter, 3 runtime, 6 window-adapter and 4 Alacritty tests total
56 selected passing tests. No failed test remains in that selected scope.

## Native renderer check and measurements

Copied the retained native fixture's source unchanged to an isolated review
crate. Its manifest points at this integrated worktree and uses the production
native wgpu profile, without re-enabling iced/wgpu defaults. The fixture alone
enables the finite compositor fault hook. Its resolved dependencies introduce
no extra package version beyond the root lockfile; manifest, lockfile, source
hash and binary hash are retained. Built with `cargo build --offline --release
--manifest-path <review-fixture>/Cargo.toml` from a copy of the root lockfile.

`/usr/bin/time -l <fixture> first-frame <fresh-dir>` passed: zero owned PTYs and
zero children. `/usr/bin/time -l <fixture> attached <fresh-dir>` passed: both
owned shutdown handles completed before application return, both controlled
children were absent at return and on an independent PID check, and pool
cleanup was acknowledged. Runs required macOS window-service access; the
sandboxed attempt could not reach those services and was explicitly terminated.

Measured first-frame failure: wall 0.64 s, user 0.09 s, system 0.06 s, maximum
RSS 96,649,216 bytes. Measured attached failure: wall 0.28 s, user 0.10 s,
system 0.07 s, maximum RSS 103,727,104 bytes. These are short synthetic failure
scenarios on macOS, not steady idle resource measurements, Windows performance,
physical responsiveness or acceptance of a release resource budget.

## Remaining platform limits

Native Windows execution is unavailable in this session. The user32 dialog's
Wait/default/Enter/Escape/X/Force and cancellation/destruction behavior, real
Windows clipboard write/locked-write acknowledgement, ConPTY HRESULT/signed
resize and survivor behavior, and fatal-renderer cleanup of actual Windows
handles remain unexecuted acceptance checks. Cross-compilation, synthetic Mac
input, Unix PTYs and macOS compositor tests do not establish those results.
Physical keyboard/monitor/Narrator checks and release idle CPU/memory budgets
remain outside this review's demonstrated results. Issue #34 stays open.
