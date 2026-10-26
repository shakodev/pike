# Issue #9 — Windows executable, arguments, cwd and environment

Dependencies #4 and #8 are closed and merged on the starting main (`d5ec040`).
The issue body is self-contained. Its referenced uncommitted Windows `PLAN.md`
is unavailable in this macOS checkout, nearby project folders and Git history.

## Behavior and boundaries

`BackendSettings` on Windows uses an empty program/no args for automatic shell
selection: PowerShell 7 (session PATH, then ProgramFiles/PowerShell/7), system
Windows PowerShell, system cmd. WSL requires an explicit `wsl.exe` program.
Resolved executables retain conventional absolute path syntax for legacy hosts;
resolution does not add canonicalize's verbatim prefix.
An explicit program is an **unquoted** executable path or name; invalid settings
return errors rather than selecting another shell. Relative paths resolve from
session cwd; drive-relative executable/search paths such as `C:tools` are rejected.
Configured cwd is captured as a full path before launch. Bare names search the session PATH then System32, without implicit
cwd search. Extensionless executables gain .exe; batch scripts require an
explicit interpreter. This policy does not modify process-wide settings.

`escape_args` defaults to true in iced_term: each vector entry is a single CRT
argument, including an empty string. Program resolution is independent of that
vector. Use `escape_args: false` only when deliberately supplying raw command
syntax to an explicitly selected interpreter. For example, cmd /D /S /C has
its own quoting/expansion rules; PowerShell -Command contains PowerShell code.
Neither shell's language is treated as native argv syntax. The existing
compatibility harness and release launch probe demonstrate both separately.

Validation precedes ConPTY allocation. `io::Error::get_ref()` can be downcast to
`iced_term::LaunchError`; error Display/Debug never includes paths, user command
lines, environment keys/values or terminal output. BackendSettings and the
underlying Options/Shell Debug are redacted. OS creation errors expose only an
OS code. Environment overrides reject empty, `=`/NUL-containing names, NUL values
and duplicate keys under Windows ordinal case-insensitive comparison (including
non-ASCII keys). Inherited UTF-16 values and =C: drive-directory entries survive;
empty overrides stay empty. No duplicate diagnostic is emitted.

All preparation data is dropped after startup. There are no new subscriptions,
workers or idle tasks. No dependency versions or Cargo.lock entries change;
two feature modules are enabled in the existing windows-sys 0.59.0 package.

## Reproduction and verification

Pinned Rust 1.97.1. Commands from repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo test --locked -p alacritty_terminal
cargo clippy --locked -p alacritty_terminal --all-targets -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
./tools/prepare-conpty.ps1
cargo build --release --locked --examples
./target/release/examples/launch_validation.exe
```

The Windows CI matrix also checks tiny-skia and runs the release launch probe,
bounded-input and shutdown integrations. The probe copies its executable into a
path containing spaces/Unicode, then compares a length-prefixed UTF-16 fixture
report byte-for-byte. Arguments cover spaces/tabs, quotes, trailing backslashes,
Unicode, empty strings, newlines and shell metacharacters. Two live sessions
have independent directories and case-varied environment overrides; inherited
SystemRoot and explicit empty values survive, while parent cwd/env are unchanged.
It checks typed private errors and separate cmd/Windows PowerShell/PowerShell 7
scripts, exit codes, automatic shell startup and acknowledged backend cleanup.
PowerShell 7 is reported as skipped if not installed; WSL execution is not an
implicit prerequisite and no WSL distribution is installed by the test.

Local macOS (Apple Silicon) checks pass: formatting; application and both
vendored crates' all-target Clippy; 4 application tests, 5 example tests,
44 iced_term unit tests, 146 Alacritty unit tests, 45 Alacritty reference tests
and 1 doctest. Two Alacritty tests initially hit sandbox-denied polling syscalls;
rerunning the explicit dependency suites outside the sandbox passed in full.
Windows MSVC cross-target all-target Clippy and Alacritty test type checking
also pass. Native Windows CI results are recorded below.
Windows execution requires a Windows host; cross-target type checking alone is
not execution evidence. No RAM, idle CPU or startup-percentile claim is made.
The CI stopwatch covers the complete synthetic suite, including shell startup,
fixture I/O and teardown, rather than isolated application startup latency.

API references checked against pinned sources and official documentation:
[CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw),
[CompareStringOrdinal](https://learn.microsoft.com/en-us/windows/win32/api/stringapiset/nf-stringapiset-comparestringordinal).


## Native Windows resource audit

[ConPTY resource run 40335067319](https://github.com/shakodev/pike/actions/runs/40335067319)
passes on source `c738687`, on both Server 2022 and Server 2025. Each cohort
runs 100 measured repetitions after 10 warmups in each of 14 scenarios: missing
executable, invalid cwd, close, and 11 injected startup failures. Every scenario
ends with zero handle and thread growth. Endpoint private-byte deltas range
from -159,744 to +212,992 bytes on Server 2022, and -81,920 to +172,032 on Server 2025;
these are individual allocator observations, not a retained-memory budget.

[Compact resource/CPU results](../baselines/issue-9/resources.csv) retain every
scenario's endpoint delta and host CPU/wall measurements, with OS metadata in
the same folder. For 100 close cycles the measured host CPU/wall times are
1156.250/2087.839 ms (2022) and 1015.625/1722.682 ms (2025). The existing sampler
enumerates system threads and reports CSV each cycle; these times include
expensive diagnostics and cannot establish application startup or idle CPU.
The audit also passes attached-tree/flood shutdown, natural output drain,
164 reported backend tests (5 opt-in probes ignored by the normal suite),
45 reference tests and one doctest on each cohort. The opt-in resource,
attached-tree and natural-drain probes are run explicitly in separate steps.


## Native Windows renderer matrix and acceptance

[Windows run 40335067341](https://github.com/shakodev/pike/actions/runs/40335067341)
passes on source `c738687` for both wgpu and tiny-skia. Each job passes formatting,
feature isolation, runtime checks, application/vendor Clippy, 4 app tests,
5 example tests, 46 adapter tests, 164 reported backend tests, 45 reference
tests and one doctest, plus release app/examples builds, packaging, bounded
input, shutdown integration and dependency-file cleanliness.

The release launch suite reports every check as PASS, with no PowerShell 7 skip:

- Executable path with spaces/Unicode and all native argument edge cases
  round-trip byte-for-byte without an extra shell.
- cmd raw scripts, Windows PowerShell and PowerShell 7 command semantics all
  execute independently with the expected output and exit codes.
- Invalid explicit shell/cwd return typed private errors; two simultaneously
  live sessions retain independent cwd/environment and preserve parent settings.
- Automatic shell selection starts and acknowledges cleanup successfully.

Complete-suite elapsed observations are 2068.6187 ms (wgpu) and 2128.4358 ms
(tiny-skia). These include several shell startups, filesystem fixture work
and teardown; they are not renderer benchmarks or startup percentiles.

Native verification caught a legacy PowerShell failure when resolved program
paths acquired the extended-length prefix from canonicalize. Conventional
absolute executable paths fix that failure while preserving explicit
lpApplicationName selection. The preparation regression asserts that normal
bare-name resolution does not introduce the prefix. Rust documents this
[canonicalize compatibility concern](https://doc.rust-lang.org/std/fs/fn.canonicalize.html).

WSL distribution execution, interactive GUI behavior and reference-machine idle
RAM/CPU measurements remain outside this launch fixture. No WSL distribution,
extra PTY implementation, dependency package or persistent runtime task is added.
The original evidence commit changed only documentation and synthetic measurements;
subsequent review changes and their verification are described below.


## PR review — capture cwd and reject ambiguous drive-relative lookup

Review found that joining a drive-relative executable or PATH directory such
as `C:tools` discards the session base and uses the parent's hidden per-drive
cwd instead. Executable lookup now rejects that ambiguous syntax with private
InvalidProgram/InvalidEnvironment errors, including ProgramFiles fallback.
Plain relative paths continue to resolve against the session directory.
Configured cwd is captured with std::path::absolute before validation and
CreateProcess so even a drive-relative cwd is passed as one full path.

Two native regressions cover drive-relative rejection/full cwd capture and
ordinary relative executable/PATH resolution. The release fixture launches
one live child with a drive-relative cwd and verifies its directory alongside
the other independent session. Automatic-shell cleanup now also asserts a
Requested completion, so an unrelated backend failure cannot pass that check.
The historical measurements above remain associated with their recorded source;
review verification is recorded in [PR #56 checks](https://github.com/shakodev/pike/pull/56/checks).

Review also found that the cmd probe embedded its temporary filename directly
in command syntax, allowing percent pairs in a directory name to be expanded.
The probe now supplies the path through a child environment variable with a
single expansion, and its directory deliberately contains literal `%PATH%`.
This covers percent-containing executable paths as well as native arguments
without reinterpreting them through a shell.


The path-isolation production revision `681a71a` passes
[Windows renderer checks](https://github.com/shakodev/pike/actions/runs/40338185319)
and [Server 2022/2025 resource audits](https://github.com/shakodev/pike/actions/runs/40338185328).
All 14 resource scenarios in each cohort retain zero handle/thread growth.
This run includes the two new native regressions and the live drive-relative cwd
probe. Subsequent changes affect only the synthetic percent-path probe and
review documentation; final checks are required green before merging.
