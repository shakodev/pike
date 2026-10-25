# Windows CI

[Windows CI](../.github/workflows/windows.yml) runs on pull requests targeting
`main`, pushes to `main`, and manual dispatch. One `windows-2025` x64 job runs
formatting, Clippy with denied warnings, application tests, and a release
build. It uses the existing `rust-toolchain.toml`; `rustup show` installs
and reports that toolchain, its declared components, and installed targets.
Compiler details (including the MSVC host triple) and Cargo's version are
printed explicitly. No environment dump or terminal-session logging is used.
The manual trigger becomes available once the workflow lands on the default
branch.

Every dependency-resolving check uses `--locked`. The final step checks that
the manifest, lockfile, and toolchain declaration were not modified. The two
official actions are pinned to full commit IDs; checkout does not persist
credentials, and the workflow only requests `contents: read` permission.

## Cache and resource use

One cache stores Cargo's registry index/archives, Git dependency database,
and `target`. It excludes Cargo credentials, installed tools, and the
application's user data. Its key contains the Windows image family, runner
architecture, target ABI, cache schema version, and separate `hashFiles`
digests of `rust-toolchain.toml` and `Cargo.lock`.

Changing either tracked file invalidates the key. There are no broad restore
prefixes across different lockfiles or toolchains. Cargo still checks inputs
and recompiles changed application sources after a cache hit. Bump `v1` in
the key if the cache layout or build settings change. GitHub's normal branch
and pull-request cache scoping applies.

Formatting runs before cache restore to reject simple failures quickly.
The checks share one job's build artifacts, incremental compilation is
disabled for these disposable builds, and superseded runs on the same ref
are canceled. The job has a 30-minute timeout. Shared runners do not run
GUI/terminal sessions or enforce hardware-dependent CPU, RAM, or latency
budgets; those remain measured Windows integration/release tasks.

## Testing dependency patches

There are no dependency patches in this baseline. Application `cargo test`
does **not** run dependency test suites. With each future patch, add a named
step at the marked location in `windows.yml`, for example:

```yaml
- name: Test iced_term event delivery patch
  run: cargo test --locked --manifest-path vendor/iced_term/Cargo.toml --lib event_delivery
```

Replace the crate and filter with the patch's actual upstream/regression
tests, and confirm the test output executes the intended nonzero test count.
If relevant coverage spans multiple targets, omit `--lib` or add explicit
commands as needed. Record those commands in the patch ledger. Include a
reviewed lockfile for the vendor test crate's graph (including its dev
dependencies) before using `--locked`; Pike's app lockfile may not govern a
standalone vendor manifest. Do not regenerate it automatically in CI or
treat a missing test target or skipped suite as success.

Run the same command locally from the repository root. Normal Clippy,
application tests, and release builds continue to use Pike's root lockfile.

## Validating workflow changes

Run `actionlint .github/workflows/windows.yml` when actionlint is available,
then inspect an actual GitHub Actions run for the PR. To verify a regression
gate, temporarily introduce a formatting error on the PR branch, push it,
confirm **Check formatting** fails, restore the source, and push again. The
final branch must pass all checks with the temporary violation removed.

Local equivalents are documented in the [build instructions](../README.md).
CI implementation evidence is recorded in [issue #2 verification](verification/issue-2.md).
