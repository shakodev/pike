# R31 — settings checkpoint; stopped at owner request

**Incomplete task; no PR opened and no merge performed.** The owner requested
all workers to finish at safe checkpoints and stop before final integration and
verification. Product source is published at
`f561096f7140c83c27328e6e0533e14c46c4d905` on `issue-31-settings`. The final
preservation commit also retains reports, synthetic evidence and one fixture-only
extension selecting an explicit font family on reload. That last fixture change
was formatted but **not compiled or run** after the stop; its exact patch is
[retained](../baselines/issue-31/unvalidated-fixture-family.patch).

The exact initial base was fetched and verified as actual merged R30 main
`9bafb5f4f2686d61c45478667737b433bee634ce`. Clean issue26 original
`b8cba879bcc0fdefa75845309135cd8a84750726` and its original branch remain,
with additional `checkpoint/a-issue26-before-r31-b8cba87`. No prior refs,
reports, builds or cache were deleted; no unmerged implementation was imported.
The coordinator later reported actual R27 merge
`1e90fb3754d69ec8e60632f6010432bf1273f370` together with the stop instruction.
It was **not integrated**, as expressly directed. Local `origin/main` was last
fetched at `9bafb5f`; this report does not claim that tracking ref is current.

## Implemented behavior

- Bounded version-one TOML in `%LOCALAPPDATA%\Pike\settings.toml`, with documented
  defaults and no startup creation, watcher, periodic work or automatic save.
  Whole-file validation and recoverable redacted errors retain the last valid
  configuration. Direct serde 1.0.229 and TOML 1.1.8 dependencies are lock-pinned;
  no second format crate, PTY or global environment mutation was introduced.
- Explicit **Ctrl+Shift+F5 / Reload settings** uses the existing common typed
  resolver/capture with Help/modal/IME priority. Bare F5 remains terminal-owned.
  Live font/palette/history use existing adapter commands without vendor changes.
  Font zoom resets to configured size; reducing history discards oldest lines.
- Up to sixteen named profiles with cwd/env/args; a visible Iced picker explicitly
  creates a tab with a chosen profile. Stable per-session profile identity stays
  separate from frozen launch settings. New/split/restart resolves current launch
  data; current explicit cwd wins, otherwise source launch cwd is inherited.
  Deleted profiles refuse inheritance/restart recoverably without fallback.
- Configurable conservative close confirmation or `never`, through the same
  captured session-set and native cleanup path. Typed bounded shortcut data is
  validated/retained; applying remaps remains R32's feature.
- An explicit `Store::save` API validates the target, writes a same-directory
  exclusive temporary, flushes and closes it, then uses Windows-compatible
  `std::fs::rename` replacement without deleting the target first. Broken files
  and prior targets survive failed writes; only the caller's temporary is cleaned.

Full schema, bounds, Windows replacement references and existing/new-session
semantics are in [settings documentation](../settings.md). The implementation
inspected pinned Iced 0.14, patched iced_term 0.8 and Windows launch sources.
The agreed R27 pane/Focus-adjacent ownership seam, focus Authority and Capture
source were not edited. Exactly the existing native worker roles were used;
no child workers were created. Personal `CODEX_HOME="$HOME/.codex-personal"`
login was verified; no global/account settings were changed.

## Verification actually completed locally

macOS 26.7.1 / 25G241, arm64, Rust 1.97.1, default wgpu, locked/offline
dependencies, `CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=target` in this worktree.
Native graphics/PTY checks ran outside the sandbox; normal model/I/O tests ran
locally. All checks already in flight at stop returned exit 0; the complete
native fixture finished with App model empty and native cleanup acknowledgements.

| Command / exact scope | Actual result |
| --- | --- |
| `cargo check --offline -p pike --lib` | Initial settings model passed and produced the pinned lockfile. |
| `cargo test --offline --locked -p pike --lib settings::` | Initial 12 focused model/I/O cases passed. |
| `cargo test --offline --locked -p pike --lib settings` | 16 host/model/I/O cases passed at the initial host checkpoint; final focused run at `f561096` passed **18**, 0 failed. |
| Exact `settings::tests::function_key_numeric_aliases_cannot_bypass_canonical_chord_uniqueness` filter with `-- --exact` | Preserved red on `f05` acceptance, then green after canonical normalization. |
| Exact `settings::io::tests::non_ascii_override_names_are_rejected_before_commit_while_unicode_values_remain_literal` filter with `-- --exact` | Preserved red on accepted É/é names; included in final 18-test green with last-valid retention and Unicode value checks. |
| `cargo build --offline --locked -p pike --bin pike --example settings_reload` | Passed for early compiled source `4a61552afbfd432e478b1ffab5a0b76a56a08d2a`, published for independent review. |
| `cargo build --offline --locked -p pike --example settings_reload` | Corrected native fixture passed compilation before publication as `20ee3360c08145e17d7ed4415682e02ba821cc24`. |
| `target/debug/examples/settings_reload /private/tmp/pike-r31-native-development-3` | Complete actual-App/runtime/native-PTY **PASS** at `20ee336`; source and binary hash retained. |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example settings_reload --example command_routing -- -D warnings` | Passed at `20ee336` after the two retained cfg/test-initializer lints were fixed. |
| `cargo clippy --offline --locked -p pike --lib --bin pike --tests --example settings_reload --target x86_64-pc-windows-msvc -- -D warnings` | Passed at `20ee336`, including Windows sharing-violation test compilation. Cross-compilation only. |
| `cargo clippy --offline --locked -p pike --lib --tests -- -D warnings` | Passed after the two review fixes and atomic temporary-handle cleanup hardening at `f561096`. |
| `cargo fmt --package pike`; `git diff --check` | Formatting and whitespace checks performed during development. These do not claim the requested final `cargo fmt --all -- --check` ran. |

Warnings from unchanged iced_winit deprecated channel methods and the existing
`block` future-compatibility notice were retained; root Clippy scopes above passed.
No full-workspace or both-renderer test round ran, and no patched-dependency tests
were required because vendor source remained unchanged. All verification was local;
GitHub Actions stayed disabled, with no dispatch, rerun or replacement CI.

## Native observations and retained failures

[Native run3 events](../baselines/issue-31/native-development-3/events.txt) show
five distinct controlled child PIDs. Initial profile A/B had distinct cwd/env;
both retained SessionId, reservation, widget identity, PID and modes during
reload. Cell metrics changed from **8.428711×18.199999** at 14 DIP to
**12.041016×26.0** at 20 DIP, with exactly two total font measurements. Exact
OSC10/11 replies confirmed light foreground/background; history changed from
80 saturated lines to 7 and stayed at 7 after additional synthetic output.
Original child cwd/env/args stayed unchanged; new B tab/split and restarted A
received the updated cwd/env/args. Help and actual established IME preedit blocked
reload; a toolbar invalid reload retained both last-valid settings and broken
bytes. Default targeted close Cancel preserved owners; `never` used the same
shutdown owner without another question. A removed profile refused a new launch.

[Cleanup](../baselines/issue-31/native-development-3/cleanup.txt) records four
intentional-close receipts with `Requested`, `output_drained=false`; those prove
disposal, not natural-exit output draining. The original A child exited before
restart and its completion was awaited. All three development runs have cleanup
receipts; they leave no known fixture work in flight.

The failed native run1 and run2 are retained intact. Run1 sent preedit immediately
after Help restoration, without driving the pinned adapter's two-paint native
IME ownership transition; its correction added both paints and an explicit
`is_composing` precondition. Run2 sampled the split source while preparation
deliberately retained source focus; its correction waits for the new attached
active identity. No production focus/capture code, deadline or native correctness
oracle was weakened. The initial host compilation/type failures and Clippy
failures are retained too.

Independent R2 reproduced two P2 findings at `4a61552`: noncanonical numeric F-key
aliases bypassed chord uniqueness, and Unicode override-name casing disagreed
with Windows ordinal launch validation. Original-author fix `f561096` canonicalizes
F-key numeric spellings and documents/enforces an ASCII override-name schema;
Unicode values and inherited backend environment remain supported. Red/green
regressions and interruption tests are retained. R2 reported independently passing
the complete `20ee336` native fixture and a six-child supplement, then continued
the narrow fix review; this report does not represent an independent final verdict.
The coordinator owns publication of independent review bodies.

## Remaining gates at the owner stop

- Integrate actual merged R27 while preserving its exclusive pane seam, then
  compile and validate the combined host. No integration was begun after stop.
- Compile/run the preserved final fixture family-selection extension; it is
  additional coverage, not evidence already obtained.
- Requested final `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`
  and full root-package `cargo test` were **not run**. Final affected-native and
  Windows cross-checks on the integrated source are also incomplete. The owner
  stopped work before these gates; there is no tooling-unavailability claim.
- Publish the focused PR with `Closes #31` and complete final independent review.
  No R31 PR exists at this checkpoint and no completion/merge is claimed.

No native Windows/ConPTY, physical keyboard/monitor, Windows IME candidate UI,
Linux, release RAM/CPU or latency measurements were performed or claimed. Windows
Clippy is compilation evidence only. Existing #13/#14 physical holds and the #14
owner waiver remain. No resource or responsiveness claim is made.

Changed files comprise `settings.rs` and focused appearance/error/I/O/schema/test
modules, `app/settings.rs` and host tests, the agreed small App/startup/tab/split/
command/close/view/theme wiring, `terminal::Entry::terminal_mut`, Cargo manifest/
lock, the finite settings fixture and deterministic shared fixture theme response,
README/settings documentation, and these reports/synthetic artifacts. All prior
branches/checkpoints/reports/caches and the worktree are retained.

See [provenance](../baselines/issue-31/provenance.json) and the
[artifact hashes](../baselines/issue-31/development-artifacts.json).
