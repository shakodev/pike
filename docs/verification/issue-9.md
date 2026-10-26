# Issue #9 — Windows executable, arguments, cwd and environment

Dependencies #4 and #8 are closed and merged on the starting main (`d5ec040`).
The issue body is self-contained. Its referenced uncommitted Windows `PLAN.md`
is unavailable in this macOS checkout, nearby project folders and Git history.

## Behavior and boundaries

`BackendSettings` on Windows uses an empty program/no args for automatic shell
selection: PowerShell 7 (session PATH, then ProgramFiles/PowerShell/7), system
Windows PowerShell, system cmd. WSL requires an explicit `wsl.exe` program.
An explicit program is an **unquoted** executable path or name; invalid settings
return errors rather than selecting another shell. Relative paths resolve from
session cwd. Bare names search the session PATH then System32, without implicit
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
also pass. Native Windows CI results will be added after the PR run.
Windows execution requires a Windows host; cross-target type checking alone is
not execution evidence. No RAM, idle CPU or startup-percentile claim is made.
The CI stopwatch covers the complete synthetic suite, including shell startup,
fixture I/O and teardown, rather than isolated application startup latency.

API references checked against pinned sources and official documentation:
[CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw),
[CompareStringOrdinal](https://learn.microsoft.com/en-us/windows/win32/api/stringapiset/nf-stringapiset-comparestringordinal).
