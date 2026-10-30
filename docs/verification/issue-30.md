# R30 — tab management

Status: independent source checkpoint; host integration and native acceptance
remain pending the coordinator's actual merged R26 checkpoint. This report is
not a feature-completion or review-approval claim.

Branch `issue-30-tab-management` starts from merged R29/PR77 main
`013690ef60db99438c51773e1e220e77b6382ce7`. Original R25 branch
`issue-25-pane-resizing`, its reports, commits and writable `target` cache remain
intact at `dbd329eccf6995b8f207a17c643aa2e0f1741147`; additional checkpoint
`checkpoint/c-r25-before-r30` retains that commit. No reset, deletion, rebase or
force-push occurred. Current `CODEX_HOME` was verified as
`/Users/daan.hoedjes/.codex-personal`; the explicitly scoped `codex login status`
reported logged in using ChatGPT. No global/account settings were changed.

Independent implementation checkpoint:
**`ca4b9aaa0449e7758be9c41eece45b2fb0f47687`** (pushed).

Changed source groups at that checkpoint:

- `src/tabs.rs` and its tests: validated `Title` plain text, 256 Unicode scalar
  limit (at most 1,024 retained UTF-8 bytes), control/line-separator/bidi-override
  removal, redacted Debug output; manual > focused-session OSC > launch-profile
  > `Terminal` fallback. Empty/whitespace-only manual name resets the override.
- `src/layout.rs`: optional manual title metadata belongs to stable TabId;
  pane/session ownership remains separate.
- `src/workspace.rs` and `src/workspace/tab_tests.rs`: stable-ID rename/reset,
  reorder relative to a stable anchor, current-order direct/cyclic selection.
  Stale endpoints reject atomically. Reorder leaves active tab, focused pane,
  zoom, grid, ratios, activity, registry and reservation identities intact.
- `src/tab_strip.rs`, declared in `src/lib.rs`: standalone compact strip and
  finite scrollable overflow presentation. Existing host controls will receive
  its callbacks only after R26 ownership handoff; no second keyboard resolver
  or lifecycle owner is introduced.

All checkpoint verification ran **locally**, macOS arm64, stable Rust/default
wgpu, offline locked dependencies, `CARGO_BUILD_JOBS=4`, and this worktree's
reused writable `CARGO_TARGET_DIR=target`:

| Exact command | Result |
| --- | --- |
| `cargo test --offline --locked -p pike --lib tabs::tests` | 7 passed: 3 new title-policy cases plus the 4 affected existing activity/tab cases. |
| `cargo test --offline --locked -p pike --lib workspace::tab_tests` | 4 passed: twelve stable tab owners, stale rename/reorder, all direct/cyclic positions, deterministic reordered removal. |
| `cargo test --offline --locked -p pike --lib tab_strip::tests` | 1 passed: compact range is bounded and contains active tab for every 1–32-tab position. |
| `cargo clippy --offline --locked -p pike --lib -- -D warnings` | Passed; existing patched-winit deprecation and `block` future-compatibility notices remain visible. |
| `cargo fmt --package pike -- --check` | Passed. |
| `git diff --check` | Passed. |

No vendor/dependency/lockfile change; patched dependency tests were not needed.
No full-workspace/both-renderer checklist, Actions enable/dispatch/rerun or
replacement CI occurred. The host, title event wiring, modal/IME ownership,
background close/native cleanup and actual-App twelve-process fixture remain
required before final PR readiness. Prior unchanged profile/query/cleanup
evidence and the corrected R29 focus/cache invariants are retained in the
R29 reports. No Windows native/ConPTY, physical keyboard/monitor, Linux or
release RAM/CPU measurements are claimed; #13/#14 physical holds and the #14
scheduling waiver remain.
