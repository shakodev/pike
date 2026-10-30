# R24 — independent split terminals

Issue: https://github.com/shakodev/pike/issues/24.

Split right and Split down are wired to real toolbar buttons, using the pinned
Iced pane grid's Vertical/Horizontal axes and its initial 0.5 ratios. Each
transaction reserves a distinct session/request and Starting placeholder before
bounded asynchronous preparation. The source keeps input focus during startup;
successful attachment selects the new terminal through the same focus path used
by pane clicks. Existing emulators, process owners, subscriptions, history and
widget identities survive layout changes.

The host freezes each launch profile/directory, and splits clone that profile
rather than the mutable application default or a guessed shell cwd. Retry/Use
default shell replace an error/exit in its existing pane, preserving siblings
and ratios. The default-shell choice preserves the launch directory. Error and
startup placeholders remain removable. Close/cancel controls carry stable session
IDs, so a queued error-pane close cannot terminate an in-place replacement.
No tab, settings, move, resize or zoom
UI is added here.

Admission uses measured cell metrics, the actual grid layout area, 28 DIP
headers, 1 DIP borders and 4 DIP gutters. Both halves and all existing leaves
must retain at least 20×5 cells, including bounded status/error chrome. Pinned
pane-grid rounding is used rather than a guessed pixel division. Invalid,
missing, nonpositive or insufficient geometry refuses before reservation with
recovery text. The model and native launch pool default to 32 retained sessions;
Starting, Running, Closing, Exited and Failed model entries consume that budget.
Closing resources are not acknowledged early to make room for another split.

All active-tab siblings render, but keyboard/paste authority follows the model's
selected session as well as the existing identity/window/widget/lifecycle gates.
The previous input owner loses focus before another is enabled. Queued positive
focus receipts do not select a sibling. Deferred rejected paste tokens cancel
their held clones. Hovered mouse/wheel and local history retain their independent
visibility gates; background protocol/lifecycle events retain their origin.
The existing cancellation/disposal ownership from merged PR72 is preserved.

## Focused verification

Verification is local, on macOS arm64 with the default wgpu renderer and
`CARGO_BUILD_JOBS=4`, using this worktree's existing writable target/cache.
GitHub Actions remains disabled. No full-workspace or second-renderer sweep is
needed: this change is confined to the application/model integration and uses
the existing pinned terminal adapter. Vendor code is not edited by R24.

Relevant commands (exact receipts/source hashes belong in the task report under
`target/issue-24/`):

```sh
cargo fmt --package pike -- --check
cargo test --locked --lib workspace::tests:: -- --test-threads=1
cargo test --locked --lib app:: -- --test-threads=1
cargo test --locked --lib launch:: -- --test-threads=1
cargo test --locked --lib app::tests::split::real_widget -- --ignored --test-threads=1
cargo test --locked --lib app::tests::paste::real_widget -- --ignored --test-threads=1
cargo clippy --locked --lib --bin pike --example independent_splits --tests -- -D warnings
cargo clippy --locked --lib --bin pike --example independent_splits --tests --target x86_64-pc-windows-msvc -- -D warnings
cargo build --release --locked --bin pike --example independent_splits
target/release/examples/independent_splits target/issue-24/FRESH_DIRECTORY
git diff --check
```

Pure regressions cover exact axes/ratios, nested/rounding boundaries, invalid
geometry, atomic refusal, the 32-entry budget and retry in place at capacity.
Controller regressions exercise real preparation/attachment, preserved source
selection and input owner, actionable geometry/capacity refusal, missing-shell
failure, successful error retry, queued slow-spawn cancellation, removed-target
late success and native disposal acknowledgements. Existing host tests cover
the gated during-preparation cleanup case, duplicate/mismatched results,
background completion, retained output, input/paste errors and pointer/history
gates. Device-backed token regressions run explicitly rather than being counted
as part of a normal test run.

`independent_splits FRESH_DIRECTORY` runs the actual App inside an Iced window,
activates its public split buttons with synthetic pointer events, and spawns its
own executable as each controlled child. It checks distinct stable session and
native process IDs across right/down/nested splits, original counter progression,
widget identity/history retention, inherited launch profile/cwd and exact
per-child synthetic bytes encoded through public TerminalView key events.
It queries the real application's widget focus before each input sequence.
Owning-window focus is explicitly injected for synthetic input, so this is not a
claim about desktop activation or physical keyboard use. Its counters, 50 ms
child timers and one-shot 15 s deadline exist only in the opt-in fixture.
Cleanup completes through the normal aggregate native disposal receipt and
records zero retained model entries. Files contain only fixture data/metadata.

Local development evidence includes `target/issue-24/app-third/`: four distinct
children, exact input for all four, original counter 7→8→9→11, retained history,
and cleanup acknowledgement with retained=0. Earlier unsuccessful harness runs
are preserved: `app-first` caught duplicate focus-probe scheduling, and
`app-second` lacked synthetic owning-window focus and correctly admitted no
input. Both completed native cleanup; neither is presented as a passed check.
The final task report distinguishes this checkpoint from final-head evidence.

The coordinator finalized R24 on actual merged main `e58838a`, its only hard
dependency, after a single final main checkpoint. Unmerged R19 is not imported;
A owns the subsequent model-active IME-commit consumer after actual R24 merges.
An independent source review found no additional blocker beyond the corrected
stable-session close control. The final host batch passed 25 cases and exposed
a fixture-only cleanup race in the new hovered-sibling case: it queued a newline
and closed immediately, permitting cancellation of that queued line. The fixture
now awaits actual native completion before shutdown of the remaining child; the
smallest regression passes with the original deadline and product code unchanged.
The failed batch and repaired-case receipt remain under `target/issue-24/`.

## Remaining verification limits

Windows-target compilation/lint is cross compilation, not native Windows or
ConPTY execution. Native Windows shell behavior, physical keyboard/IME/DPI and
monitor checks, and release CPU/RAM/latency budgets remain unmeasured here.
No performance compliance is inferred from bounded structures or fixture
counters. The existing intermittent Unix partial-line cleanup limitation is
unchanged; controlled fixture lines are completed before cleanup, and no
deadline is increased to hide a failure.

The previous #13/#14 physical holds and #14 scheduling waiver remain preserved
and nonblocking. PR72's reviewed checkpoint remains archived at
`refs/archive/pike/pr72-4a507f3` (`4a507f391b0a1c59fb786cc882001f1bc1beac15`)
after coordinator-owned settled-branch cleanup; prior reports and ignored caches
remain intact. R24 must be reviewed and is never merged by this worker.
