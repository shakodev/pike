# Issue #2: Windows CI verification

Prerequisite #1 is merged as `77f1f23`; no application code or dependency
changes are included in this CI task. Existing planning files are preserved.

## Local verification

- `actionlint` 1.7.12 validates `.github/workflows/windows.yml`.
- The validator was downloaded only into ignored `target/actionlint`; its
  Windows amd64 archive SHA-256 matches the release asset digest:
  `6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9`.
- Required local Rust checks and hosted-run evidence are recorded below
  after validation completes.

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
