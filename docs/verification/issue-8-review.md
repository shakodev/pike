# PR #54 review and final verification

The review found and fixed three issues before merge. The pinned runtime,
existing PTY implementation and shutdown policy are retained. No dependencies
or background workers are added by these review fixes.

## Resolved findings

1. **Failed startup could execute user commands.** `CreateProcessW` started the
   shell before wait registration and pipe-worker creation could still fail.
   Error cleanup terminated the root, but could not undo its filesystem side
   effects or reliably dispose descendants after closing raw output early.
   A regression delayed failure 200 ms at each post-process setup stage, using
   a synthetic command that writes a marker. Both Windows cohorts reproduced
   markers at wait registration, writer-worker and reader-worker failure in
   [run 40320092406](https://github.com/shakodev/pike/actions/runs/40320092406).
   The root now starts with `CREATE_SUSPENDED` and resumes only after the watcher
   and both pipe workers are owned by a complete PTY. Failed earlier setup never
   executes the shell. Resume failure uses full PTY disposal. The primary-thread
   handle remains owned through resume and drops on success or failure. Installed
   windows-sys 0.59 bindings and Microsoft's
   [creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)
   and [ResumeThread contract](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-resumethread)
   were checked before implementation.
2. **Callback context crossed moves of a unique owner.** A pointer derived
   through a mutable Box borrow was retained by Windows while the Box moved
   into the returned watcher/PTY; Windows may invoke a wait callback during
   registration or while its owner moves. Rust documents that Box-derived raw
   pointers cannot remain in use across such moves under the compiler's
   [Box aliasing rules](https://doc.rust-lang.org/std/boxed/index.html#considerations-for-unsafe-code).
   Context now uses Arc::as_ptr and shared ownership, retaining the same one
   allocation until synchronous wait unregistration completes. Atomics and
   mutexes guard shared state. Existing cancellation, early-exit, failed
   registration and in-flight callback destruction regressions pass. This
   ownership correction does not claim runtime tests can detect aliasing UB.
3. **Packaging accepted an incompatible executable.** The runtime is pinned
   for x64, but `-Executable` previously accepted any file and produced a ZIP.
   Packaging now checks the PE signature, AMD64 machine, PE32+ header and
   executable/DLL flags before creating output or restoring the runtime. Tests
   reject a synthetic ARM64 PE, non-PE data and the real ConPTY DLL; release CI
   continues to package actual x64 Pike successfully. Header fields were checked
   against Microsoft's [PE specification](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format).

## Verification

Source **4c2c751** passes the
[Windows audit matrix](https://github.com/shakodev/pike/actions/runs/40320678860)
and [complete renderer matrix](https://github.com/shakodev/pike/actions/runs/40320678764).
Both Server 2022/build 20348 and Server 2025/build 26100 pass:

- The new delayed startup regression at wait, writer, reader and resume failure.
- 161 backend unit tests, 45 reference cases and one doctest. Five isolated
  probes/fixture tests remain excluded from the ordinary suite and are invoked
  individually by CI.
- Strict isolated release resource checks: 100 cycles for each of **14**
  scenarios, including new resume-failure injection, with **zero handle growth
  and zero thread growth** in every scenario on both cohorts. Thresholds remain
  unchanged. Private-byte deltas range from 0 to +49,152 bytes on Server 2022,
  and -4,096 to +36,864 on Server 2025; these observations are not absolute
  process-memory bounds.
- Attached root/child/grandchild closure, flood closure, injected close-worker
  failure and >180 KiB natural output drain with final sentinel, EOF and code 37.

| Release observation, one run each | Server 2022 / 20348 | Server 2025 / 26100 |
| --- | ---: | ---: |
| Attached tree closure | 3.292 ms | 4.990 ms |
| Tree closure under flood | 3.555 ms | 5.782 ms |
| Tree closure with close-worker failure | 3.066 ms | 5.657 ms |
| Natural startup/output/drain/disposal | 29.792 ms | 38.962 ms |

Raw before/after evidence and OS/compiler/runtime metadata are retained in
[baselines/issue-8-review](../baselines/issue-8-review/). Resource CPU/cycle
counters include diagnostic sampling and report I/O, as in the original audit.
Tree times include observing all three retained process handles signaled; these
are individual observations, not latency percentiles. They do not measure GUI
idle behavior, renderer performance or whole-process-tree memory.

The complete Windows jobs pass for wgpu and tiny-skia: formatting, feature and
measurement-support guards, app/examples/dependency tests, explicit adapter
suite (46 tests), Clippy, release builds and package verification, bounded-input
and adapter shutdown-lifecycle integration. Local pinned Rust 1.97.1 formatting,
root all-target Clippy, application tests (4), explicit backend Clippy/tests
(146 unit, 45 reference, one doctest) and adapter tests (44) also pass.
Windows-target all-target backend Clippy passes. The later evidence/documentation
commit does not alter the tested Rust or scripts.

Windows client cohorts, other architectures, detached descendants, GUI idle
resources and real shared-host thread exhaustion retain the previously stated
limitations. There are no remaining actionable review findings in this PR.
