# R19 named-Space AltGr author correction

Original reviewed head: `47033ad34fa7e34571b5088fba9dcc35c4f8ee77`.
Corrected source checkpoint: `338f226e0fa2d5f39f622913e14664217a3becae`.
Final integrated source: `9a253aed4ca1f27a1b0fbd8395d9aede60731081`.
Original merged base: `e58838a828c9deabaa340f7119d9ce38374b4447`.
Final actual merged main: `609d702ffc93bba95414f0e26a4a463d5e720f70` (PR74/R24).
Same branch `issue-19-ime`, same [PR73](https://github.com/shakodev/pike/pull/73),
with all prior commits and artifacts retained. Actual main was fetched and
integrated once after the coordinator confirmed its merge. Only `src/terminal.rs`
conflicted; its resolution retains R24's model-active keyboard/paste/focus gates
and adds IME to the same deferred-token gate. No unmerged split implementation
was imported. Current main's AGENTS/PLAN are unchanged from the inspected baseline.

The [independent finding](https://github.com/shakodev/pike/pull/73#issuecomment-6526387256)
reproduced dropped printable text for `Key::Named(Space)`, Ctrl+Alt with or without
Shift, and supplied U+00A0. The unchanged public two-terminal Row reproducer was
copied into this author's ignored target; only absolute repository paths in its
manifest were adapted. At original source it exits 1: both cases have no Write
and capture=false, while ordinary Space, dead-key+Space and character AltGr pass.
The new vendor public-Row regression also fails old production source with
`[] != [[194,160]]`; both original red logs and commands are retained.

Pinned `iced_winit` 0.14.1 `src/conversion.rs` uses `key_without_modifiers()` and
`text_with_all_modifiers()`. Installed winit 0.30.13 Windows
`keyboard_layout.rs` returns the non-character virtual-key mapping before layout
lookup and maps VK_SPACE directly to `NamedKey::Space`. This validates the
synthetic event representation; it is source evidence, not a physical Windows
observation.

The existing printable AltGr priority now uses one guarded keypress arm for
Character or named Space before their binding arms. It still requires Ctrl+Alt,
no logo modifier and nonempty text without controls. Composition keeps precedence.
The keyboard correction does not change other named keys, bindings without
printable AltGr text, writer admission, the runtime DPI patch, pointer/history
behavior, dependencies or lockfile. The final host integration additionally
requires IME's origin to be the model's selected session, retaining the existing
identity/reservation/window/visibility/lifecycle/widget-focus checks and cancelling
all clones before any rejection. `Action::ImeRejected` remains origin-visible.

Changed source files:

- `vendor/iced_term/src/view.rs`: preserve printable named-Space AltGr text once.
- `vendor/iced_term/src/view/ime/tests.rs`: a public two-view Row event matrix
  checks one exact Write from the focused origin and capture for both variants,
  ordinary Space, dead-key/spacing accents, character AltGr, Ctrl+C/copy, empty/
  absent/control text, Ctrl-only/Alt-only/logo exclusions and named-arrow binding
  behavior. Explicit Space/Ctrl/Alt bindings still run for nontext events.
  Composing Space and shortcut variants remain zero-write, followed by commit-once.
- `examples/ime_composition.rs`: extend the existing public-widget/raw-PTY probe
  with ordinary Space and both named-Space AltGr variants, checking one command
  per event before routing it. Expected child bytes become 17 and 3.
- `vendor/iced_term/PATCHES.md`: record the correction and regression contract.
- `src/terminal.rs`: integrate IME into actual merged R24's model-active token
  gate without changing `synchronize`, Write/positive-focus authority, pointer/
  history routing or `CloseSession`.
- `src/app/tests/ime.rs`: add a real-split regression with tokens staged through
  the public widget. The split cancels earlier composition; an inactive visible
  sibling with deliberately stale backend focus cannot admit a newly staged
  token or its held clone; the selected sibling commits UTF-8 once.

Verification ran locally on macOS, stable Rust 1.97.1, tiny-skia, with
`CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR="$PWD/target/issue-20"`. No full-workspace,
both-renderer or remote CI round ran; Actions stayed off. Relevant commands:

```sh
cargo fmt -p iced_term -- --check
rustfmt --edition 2024 --check examples/ime_composition.rs
cargo test --locked --offline -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio ime
cargo test --locked --offline -p iced_term --lib --no-default-features --features iced/tiny-skia,iced/tokio bindings::tests
cargo run --locked --offline --manifest-path target/issue-19-ime/author-key-cases/Cargo.toml --bin pike-r19-key-cases-review -- target/issue-19-ime/author-key-green
cargo clippy --locked --offline -p iced_term --lib --tests --no-default-features --features iced/tiny-skia,iced/tokio -- -D warnings
cargo clippy --locked --offline --no-default-features --features renderer-tiny-skia --example ime_composition -- -D warnings
cargo build --release --locked --offline --no-default-features --features renderer-tiny-skia --bin pike --example ime_composition
```

At the source checkpoint, all 12 IME-filter tests, six binding tests and all five
unchanged reviewer-probe cases pass. Formatting, both affected Clippy targets
and the release build pass. The probe reports exactly `[[194,160]]` and
capture=true for each formerly failing case. The two existing upstream runtime
deprecations remain unchanged; the copied review fixture additionally retains
its own three unused-item warnings. No warning suppression was added to the
adapter or application.

[Evidence](../baselines/issue-19/altgr-space-47033ad/) retains reviewer source,
manifest and red log, author red/green logs and exact argv/results, the vendor
red regression, final successful targeted logs and source/binary hashes.
Initial test-scaffold compilation and binding-mode setup failures are retained
separately; they were fixed in the fixture, without expanding production policy.
The earlier [final-e58838a evidence](../baselines/issue-19/final-e58838a/),
including the undiagnosed macOS cleanup timeout, stays intact.

At the `338f226` source checkpoint, 22 of the prior 25 hashed files are unchanged; only
the widget, its IME tests and the raw-PTY example differ. Successful runtime DPI
checks and unchanged widget/AltGr checks are reused after actual-main integration.
The existing runtime red/green, provenance and cleanup evidence have not been
replaced. Reviewer1 independently reproduced green results with its original
five-case probe at exact `338f226`; final review is reserved for the updated head.

After actual merged R24 integration, targeted local host checks passed: three
IME tests (including both graphics fixtures), two paste tests (including its
graphics fixture), one queued-keyboard focus test, one visible-sibling keyboard/
positive-focus test and one real-widget visible-sibling paste test. This is the
relevant shared routing boundary; no full app/model/launch or renderer sweep was
repeated. Host formatting, denied-warning Clippy and the final release build
passed. The earlier adapter checks still cover byte-identical adapter sources.

```sh
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::ime -- --include-ignored
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib app::tests::paste -- --include-ignored
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib queued_keyboard_requires_current_pane_and_window_focus
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib visible_sibling_stale_focus_and_keyboard_cannot_change_input_authority
cargo test --locked --offline --no-default-features --features renderer-tiny-skia --lib real_widget_sibling_paste_cancels_held_clones_and_releases_pending_capacity -- --include-ignored
rustfmt --edition 2024 --check src/terminal.rs src/app/tests/ime.rs
cargo clippy --locked --offline --no-default-features --features renderer-tiny-skia --lib --tests --bin pike --example ime_composition -- -D warnings
cargo build --release --locked --offline --no-default-features --features renderer-tiny-skia --bin pike --example ime_composition
/usr/bin/time -l target/issue-20/release/examples/ime_composition target/issue-19-ime/final-609d702-probe
```

The final release probe passed exact 17/3 bytes, including ordinary Space and
both U+00A0 modifier variants, composing zero-writes/commit-once, dead keys,
Ctrl+C/copy, owner/replay/blur/modal cancellation and candidate geometry. Its
two children exited 0 with output_drained=true; PIDs 12728/12734 were independently
absent afterward. This probe uses public widgets and real macOS PTYs, while the
separate host tests exercise actual app/model routing. The single complete-probe
observation was 0.71 seconds wall, 0.03 user, 0.07 system and 17,874,944-byte
maximum RSS (about 17.05 MiB). This includes renderer startup, two PTYs, assertions
and cleanup; it is not GUI idle/private bytes, latency percentiles, a leak test
or a Windows budget result.

Native Windows IME delivery, Windows execution/cross-compilation, physical CJK/
dead-key/AltGr layouts, candidate boxes and mixed-DPI monitor transitions have
not been run for this correction. Synthetic Iced events on a Mac are not such
evidence. No new idle memory/CPU, responsiveness or resource-budget claim is
made. The #13/#14 physical holds and #14 scheduling waiver remain in force.
Exclusive Reviewer1 re-review of the final updated head is required before the
coordinator can merge; the original author does not merge.
