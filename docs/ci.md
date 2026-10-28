# Windows CI

[Windows CI](../.github/workflows/windows.yml) runs on pull requests targeting
`main`, pushes to `main`, and manual dispatch. A `windows-2025` x64 matrix runs
formatting, Clippy with denied warnings, application tests, and a release
build separately for `renderer-wgpu` and `renderer-tiny-skia`, using
`--no-default-features`. It uses the existing `rust-toolchain.toml`; `rustup show` installs
and reports that toolchain, its declared components, and installed targets.
Compiler details (including the MSVC host triple) and Cargo's version are
printed explicitly. No environment dump or terminal-session logging is used.
The manual trigger becomes available once the workflow lands on the default
branch.

Every dependency-resolving check uses `--locked`. The final step checks that
the root/vendor manifests, lockfiles, and toolchain declaration were not modified. The two
official actions are pinned to full commit IDs; checkout does not persist
credentials, and the workflow only requests `contents: read` permission.

## Cache and resource use

Each renderer cache stores Cargo's registry index/archives, Git dependency database,
and `target`. It excludes Cargo credentials, installed tools, and the
application's user data. Its key contains the Windows image family, runner
architecture, target ABI, cache schema version, renderer, and `hashFiles`
digests of `rust-toolchain.toml`, `Cargo.lock`, and root/vendor manifests.

Changing these dependency inputs invalidates the exact key. A fallback prefix
allows reuse within the same image, architecture, ABI, renderer and toolchain;
Cargo still checks dependency/features/source fingerprints and recompiles
changed inputs after a cache restore. Bump `v2` in
the key if the cache layout or build settings change. GitHub's normal branch
and pull-request cache scoping applies.

Formatting runs before cache restore to reject simple failures quickly.
The checks share one job's build artifacts, incremental compilation is
disabled for these disposable builds, and superseded runs on the same ref
are canceled. The job has a 30-minute timeout. Shared runners run a headless
synthetic ConPTY input probe. They do not run GUI sessions or enforce
hardware-dependent CPU, RAM, or latency budgets; those remain reference-machine
integration/release tasks.

## Testing dependency patches

Application `cargo test` does **not** run dependency test suites. iced_term is
now an explicit workspace member; `default-members = ["."]` keeps the app's
normal commands from enabling vendor compatibility defaults. CI explicitly
runs the vendor unit tests, including history/viewport and font/geometry regressions, for each lean profile:

```yaml
- name: Test vendored iced_term explicitly
  run: cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
```

The other matrix entry uses `iced/tiny-skia,iced/tokio`. Both Iced renderer
and executor features must be supplied when selecting the vendor alone;
Iced rejects a build without an executor feature. These tests use the root
workspace lockfile. The vendor's original packaged lockfile is retained only
as provenance. Future patches must add focused regression coverage and
record commands/nonzero test counts in the [patch ledger](../vendor/iced_term/PATCHES.md).

Run the same command locally from the repository root. Normal Clippy,
application tests, and release builds continue to use Pike's root lockfile.
`tools/test-feature-profiles.ps1` verifies actual unified Cargo trees for
both renderers. It rejects re-enabled Iced defaults/thread pool, Tokio full
or unused I/O features, and a renderer dependency leaking into the other
profile. CI also runs the harness unit tests and measurement-support tests;
GUI measurements stay on the local reference machine. The bounded-input
probe exercises a stalled reader, atomic rejection, Unicode byte ordering in
another session, and cancellation/cleanup under ConPTY on CI as well.
`tools/test-feature-profile-guard.ps1` supplies synthetic Cargo trees to verify
that features on earlier package versions cannot be hidden by later entries,
and that renderer dependencies leaking into the other profile are rejected.

The native wgpu profile uses `iced/wgpu-bare` with exact-pinned wgpu feature
selection for DX12, Vulkan and Metal. The feature guard requires DX12/Vulkan/WGSL
and rejects defaults, GLES/WGL and OpenGL dependency leakage. This prevents the
pinned WGL worker's input-language-change deadlock; see the
[native keyboard follow-up](verification/issues-13-14-follow-up.md).

Alacritty 0.25.1 is another explicit workspace member, patched for actual-writer
byte accounting. CI runs `cargo test --locked -p alacritty_terminal` (145 unit,
45 reference and one documentation test) and its all-target Clippy separately.
The packaged dependency lockfiles remain provenance only. After building
examples, `tools/measure-input-backlog.ps1` runs the headless release fixture;
its counters are observations, with no hardware timing budget asserted.
Probe output uses the runner's temporary directory with the workflow run ID
and attempt. It stays outside cached `target`, so a restored cache cannot
make a later probe fail its fresh-output-directory check.

## Validating workflow changes

Run `actionlint .github/workflows/windows.yml` when actionlint is available,
then inspect an actual GitHub Actions run for the PR. To verify a regression
gate, temporarily introduce a formatting error on the PR branch, push it,
confirm **Check formatting** fails, restore the source, and push again. The
final branch must pass all checks with the temporary violation removed.

Local equivalents are documented in the [build instructions](../README.md).
CI implementation evidence is recorded in [issue #2 verification](verification/issue-2.md).


The shutdown lifecycle release probe also runs for each renderer:
`tools/measure-shutdown-lifecycle.ps1`. It verifies repeated close, final-output
retention and completion after canceled consumers/owners, reports latency and
external process counters, and checks owned fixture PIDs disappear. It uses a
fresh runner-temporary directory. See [issue #7](verification/issue-7.md) for the
source-completion/backend-completion distinction and measurement limits.

The release `launch_validation` example checks native argument round trips,
independent cwd/environment settings, private structured errors and separate
cmd/PowerShell command semantics on both renderers. See [issue #9](verification/issue-9.md).

The existing release `keyboard_input` example now runs on native Windows for
both renderers. It checks 168 synthetic key events in each of four cursor/newline
mode combinations through the widget, bounded writer and ConPTY, then uses the
runner's Git for Windows Vim for insert/move/save/exit. Profile-free PowerShell 7
also runs its shipped PSReadLine module with built-in Emacs bindings to check
Ctrl+U, Ctrl+C, post-cancellation input and navigation. The job checks all four
raw fixture PIDs after acknowledged shutdown and retains `keyboard-input-*`
artifacts from a fresh runner-temporary directory, including partial evidence
on failure. Metadata records the tested commit, OS, renderer, executable hash
and total probe time; only synthetic fixture data is retained. This does not
validate physical US/AltGr/dead-key layouts, and does not assert
hardware-dependent performance budgets. See [issue #14](verification/issue-14.md).

`tools/test-keyboard-layout.ps1` additionally drives a real Iced-window fixture
with SendInput scan codes and per-thread US, German AltGr and US International
dead-key layouts. It checks the actual Windows/Winit/Iced/widget/ConPTY path,
including repeated presses and Alt prefixing, and retains results under the same
artifact's `os-layout` directory. Lack of an interactive input desktop, focus or
layout support fails explicitly. This is OS-level synthetic input, without any
claim of physical keyboard hardware testing.
