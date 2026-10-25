# Issue #2: Windows CI verification

Recorded 2026-10-25. Prerequisite #1 is merged as `77f1f23`; no application code or dependency
changes are included in this CI task. Existing planning files are preserved.

## Local verification

- `actionlint` 1.7.12 validates `.github/workflows/windows.yml`.
- The validator was downloaded only into ignored `target/actionlint`; its
  Windows amd64 archive SHA-256 matches the release asset digest:
  `6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9`.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --locked --all-targets -- -D warnings`: pass.
- `cargo test --locked`: pass (0 application tests at this baseline).
- `cargo build --release --locked`: pass on local Windows x64 MSVC.

## Cache-key verification

The key contains two separate `hashFiles` expressions for the toolchain
file and root Cargo.lock, with no cross-toolchain/lockfile restore prefixes.
Using GitHub's documented two-stage SHA-256 construction for each single
file, an in-memory comment was appended to a copy of each input in turn.
Each mutation changed its corresponding digest and the complete key, while
the other digest stayed unchanged. Tracked inputs were not modified.
The construction is documented in the official
[hashFiles reference](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions#hashfiles).

The [three calculated keys](issue-2-cache-keys.json) retain this fixture
evidence. The baseline key exactly matches the hosted cache key below.

## Hosted verification

| Run | Outcome | Evidence |
| --- | --- | --- |
| [Initial clean run](https://github.com/shakodev/pike/actions/runs/40202771637) | Pass | Fresh hosted checkout, cache miss, all required commands pass, cache saved |
| [Intentional formatting violation](https://github.com/shakodev/pike/actions/runs/40203522724) | Expected failure | `Check formatting` exits 1; cache/compile/test/build steps are skipped |
| [Restored source](https://github.com/shakodev/pike/actions/runs/40203609448) | Pass | Exact cache hit/restoration; formatting, Clippy, tests, release, and dependency-file checks pass |

The whitespace-only fixture changed the `main` signature's formatting in
commit `b323f4f`, then was reverted in `384f265`. The final application
source, Cargo.toml, Cargo.lock, and rust-toolchain.toml match `main` exactly.
Local Rust checks were repeated after the revert and passed.

Inspected logs report active `1.97.1-x86_64-pc-windows-msvc`, installed target
`x86_64-pc-windows-msvc`, rustc `1.97.1 (8bab26f4f 2026-07-14)`, and Cargo
`1.97.1 (c980f4866 2026-06-30)`. The workflow prints compiler/toolchain
metadata and its two fixed Cargo settings; it does not dump the environment
or launch terminal sessions. The final dependency-file diff is empty.

The cold job took 8m26s; the restored job took 1m17s. Its log reports an exact
cache hit and successful restoration of 692,037,162 archive bytes. These are
single hosted-job observations, not hardware performance thresholds or Pike
runtime measurements.

## Scope and limitations

No vendor patches exist yet, so there are no dependency-patch suites to run.
[The CI guide](../ci.md#testing-dependency-patches) supplies the exact standalone
Cargo command, lockfile requirements, and marked workflow insertion point
for each future patch's focused regression tests. Application tests currently
contain zero tests; CI does not claim otherwise.

Hosted CI exercises build/check behavior on Windows MSVC. It does not launch
a terminal, dump environment variables, or set hardware-specific performance
thresholds. There is no runtime change to re-profile; the recorded
[native Windows baseline and limitations](issue-1.md) remain applicable.
