# R29 review correction — launch intent and hidden cache ownership

Same branch `issue-29-live-tabs` and [PR77](https://github.com/shakodev/pike/pull/77).
Initial source `b45c4cd726756444617922a07be63dcf33e0e105` and publication
`042061d6365c7e0b7cbd15627a5c634dc4b4331b` remain preserved. Corrected production,
fixture and test source: **`81523ac06aa7c69ba96bc77e03ad9d2fd16c5841`**.
The first correction checkpoint `678ebc1d3b5c94182b24db0c2a0921f57dcba0ce`
is also preserved.
The final publication adds only reports and bounded synthetic receipts; exact
final head is in the PR/coordinator handoff. Base remains actual merged main
`52f5f90675c11ce15d19a11554c9105f803056a3`, integrated once before verification.

## Findings and correction

R2's native actual-App probes found two gaps which the initial passing fixture
did not cover. A pending split/retry retained its deferred focus intent after
a tab round trip and newer explicit sibling selection; completion then selected
itself and transferred input authority. A cached hidden grid could also retain
old scalar SessionIds after pane close or in-place restart. No live resource
leak was demonstrated by the cache finding.

The author reproduced both original native failures before production changes.
A separate hidden-restart regression also failed on its obsolete cached owner.
Immutable reviewer probes/logs, author reds and corrected greens are under
[review-fix](../baselines/issue-29/review-fix/), with hashes and exact commands in
its [provenance](../baselines/issue-29/review-fix/provenance.json) and
[command manifest](../baselines/issue-29/review-fix/commands.json).

`src/app.rs`, `src/app/tabs.rs`, `src/app/split.rs` and `src/app/close.rs` now
supersede the existing `focus_on_attach` intent on accepted newer pane/tab/
create/split/retry/committed-close choices. Only a valid pane/tab choice cancels
an intent; failed or stale requests do not. An explicit selection of the pending
pane still receives its normal active-session attachment focus. This changes
launch intent only: no additional focus authority, epoch, shortcut mapper or
`src/app/focus.rs` edit was introduced. A's agreed R26 authority seam and the
public `NewTab` / `ActivateTab(TabId)` controls are unchanged.

`src/app/resize.rs` compares cached scoped pane/session membership with today's
canonical grid, replacing only stale lightweight membership while retaining the
bounded canvas and frozen state. It does not fit hidden trees, measure cells,
capture viewports or resize PTYs. In-place retry now synchronizes immediately
after reservation/submission, so the old cache owner disappears while the
replacement is still pending. Existing modal/shutdown and retained-32 policies
remain active.

Five new tests cover split focus with and without a tab round trip, hidden
close, pending hidden restart/cache replacement/newer selection, explicit retry
focus, and hidden attached retry with zero snapshots/unchanged sibling work.
The two independently strengthened reviewer restart cases were adopted as
permanent host regressions, with non-vacuous pre-existing cache membership.

Re-review passed those four sensitive probes and the native tab/GUI checks,
then found that an empty stale non-window close scope still retired a valid
pending intent. The author's matching edge check also reproduced this on
`678ebc1`. `commit_close` now returns before any effect for empty pane/tab
scopes, while empty **window** close still performs pool shutdown. Three more
permanent regressions isolate all six stale controls against removed owners,
preserve pending intent across valid Cancel, and await disposal of a removed
prepared owner after the model/window is empty. The six-case independent probe,
its red and the author red are retained; this is the same PR/issue correction.

## Local evidence

All verification ran **locally**, native macOS arm64, default wgpu, locked and
offline dependencies, `CARGO_BUILD_JOBS=4`, this worktree's writable target.
Native PTY/graphics checks explicitly ran outside the sandbox. No GitHub Actions,
full workspace, both-renderer or unrelated suite ran. Exact long filter lists
and results are retained in `commands.json`; the table names each receipt.

| Command / scope | Result / receipt |
| --- | --- |
| Original reviewer split-round-trip / hidden-close actual-App probes | **2 failed** on old source; byte-preserved `reviewer-original-red.log` and `reviewer-focus-probe.rs`. |
| Author native reproduction of those two probes | **2 failed** before fixes; `author-original-red.log`. Focus transferred to pending session; removed SessionId remained cached. |
| Independent strengthened retry focus / hidden-restart cache probes | **2 failed** on old source; `reviewer-restart-red.log` / `reviewer-complete-probe.rs`. |
| Author hidden-restart/cache/newer-selection regression | **1 failed** before fixes on obsolete cached SessionId; `restart-original-red.log`. |
| Three sensitive native author regressions | **3 passed**; `sensitive-green.log`. |
| Remaining affected tab/split/retry/resize/modal/IME cases, excluding two already-green cache cases | **21 passed**, including explicit ignored-by-default native IME, both reviewer retry variants and extended split focus with/without round trip; `affected-native-green.log`. The extended split test was repeated because its fixture gained the no-round-trip case. |
| `cargo build --offline --locked -p pike --example live_tabs` | Passed; `fixture-build.log`. |
| `target/debug/examples/live_tabs docs/baselines/issue-29/review-fix/native-final` | **Passed**, native actual App/window; `native-final.log`, `native-final/completed.txt`, `events.txt` and controlled child files. |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example live_tabs -- -D warnings` | Passed; `clippy.log`. |
| Same Clippy command with `--target x86_64-pc-windows-msvc` | Passed, cross-compilation only; `windows-cross-clippy.log`. |
| `cargo fmt --package pike -- --check` and source/report diff check | Passed; `format.log`, `diff-check.log`. |
| Author stale-control edge check / independent six-control probe on `678ebc1` | **Both failed**; `stale-controls-original-red.log`, `reviewer-stale-intent-red.log`, `reviewer-intent-probe.rs`. Only stale pane/tab closes failed the independent six-case check. |
| Final stale-control/Cancel/empty-window cases plus two affected R28 close/shutdown neighbors | **5 passed**; `stale-intent-green.log`. The six stale controls run independently inside one regression; real empty-window cleanup retains its owned-resource acknowledgement. |
| Final native / cross-Windows targeted Clippy | Passed; `stale-intent-clippy.log`, `stale-intent-windows-cross-clippy.log`. |

The first author restart fixture had not synchronized the failed sibling into
the presentation and unwrapped an absent cached pane. That is retained as
`restart-unprimed-fixture.log`, explicitly a fixture-setup failure. Priming the
actual host presentation before hiding it produced the meaningful old-owner
red above. The independent reviewer similarly strengthened its cache
precondition; its passing/failed oracles were not weakened. No original receipt,
test deadline, input or cleanup oracle was discarded or relaxed.

The corrected native GUI run at `678ebc1` observed four distinct PIDs
55503/55507/55520/55528, 128×32 same-size terminals and 63×15 nested panes. Exact
hidden query replies, active-only `active29\r`, unread/bell/exit flags, pending
attachment, remembered widget focus, selection/history/modes/ratios, zero hidden
snapshots/presentation work and native exit/shutdown cleanup all passed again.
Seven immediate activation refreshes remained one each; synchronous dispatch
samples were **227–262 µs**, debug synthetic/macOS samples rather than release
percentiles or resource budgets. This successful GUI evidence is reused for the
final empty-scope guard: its ordinary tab/close path is unchanged. The changed
stale-scope and empty-window branches were verified by the five final native
tests above, rather than rerunning the GUI or earlier passing suites for a
different publication hash.

The original 4 tab/activity and 16 workspace tests, adapter checks, native
geometry/input/lifecycle receipts and their hashes remain valid for unchanged
algorithms. Only checks affected by this production correction were repeated.
No vendor/dependency/lockfile, capture or focus-module change requires another
patched dependency suite. Existing deprecation/future-compatibility notices
remain visible.

Native Windows/ConPTY, Linux, physical keyboard/monitor and release RAM/CPU
verification remain unavailable; the #13/#14 physical holds and explicit #14
scheduling waiver remain. No Actions enable/dispatch/rerun or replacement CI.
Independent re-review and coordinator-owned merge are separate gates; the
author has not merged PR77.
