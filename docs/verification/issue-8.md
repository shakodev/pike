# Issue #8: ConPTY resource ownership and failure cleanup

The application-owned cleanup repairs and the #55 runtime remedy are implemented
in PR #54. The final packaged runtime has **zero host handle growth** across all
13 measured scenarios on both Server 2022 build 20348 and Server 2025 build
26100. [Issue #55 verification](issue-55.md) records the pinned runtime, deployment
policy and passing final runs. The inbox-only measurements below are retained
as historical evidence of why the native runtime became necessary.

Prerequisites #4 and #7 were closed and present in the checkout. `PLAN.md` is
absent from the checkout and all fetched Git history. Issue #8 confirms it was
an uncommitted local deliverable and defines self-contained requirements. This
session cannot retrieve an unshared file or another device's Codex conversations.
The user's existing, untracked `AGENTS.md` is preserved outside the commits.

## Ownership audit

| Resource | Acquisition and successful ownership | Failure and release path |
| --- | --- | --- |
| HPCON | Immediately wrapped in `Conpty` inside `Startup`, then moved into `Pty.backend` | Startup closes its output read end before dropping ConPTY. Normal disposal moves it exactly once into the transient close worker and joins it while draining. If close-worker creation fails, request direct draining on the existing reader and close synchronously on the disposing thread; the reader bypasses piper backpressure until EOF, keeping output serviced through client shutdown. `Conpty::drop` calls the matching close function once. |
| Four anonymous-pipe endpoints | `miow` owns all endpoints; ConPTY gets its own copies | Parent console-side copies drop immediately after successful ConPTY creation. A failed first/second pipe or ConPTY creation drops existing endpoints by scope. Host endpoints move into worker closures; those drop their endpoint on return, error, unwind or failed thread creation. Startup retains the raw output until transfer and joins any writer already started during rollback. |
| Child process handle | Successful `CreateProcessW` immediately becomes `Arc<OwnedHandle>`; startup retains a rollback reference, watcher retains the successful owner | Startup error terminates the suspended root, closes console/output and waits for exit. The root cannot execute until wait and pipe-worker setup succeeds. The last owning reference calls `CloseHandle`. The callback borrows the handle from its context; no borrowed `Child` handle is adopted. |
| Primary thread handle | Immediately becomes `OwnedHandle`; the child starts suspended | Retained through wait/pipe-worker setup and released after successful resume or any startup error. Resume failure disposes the fully owned PTY. |
| Attribute list and allocation | Pointer-aligned `Box<[usize]>`; owner constructed only after successful initialization | Sizing/init failure frees storage without deleting an uninitialized list. Every successfully initialized list calls `DeleteProcThreadAttributeList` once before deallocating storage, including update/process/registration failure. |
| Child-wait registration/context | Watcher owns stable Arc-backed callback state and registration | Failed registration drops the unregistered context. The one-shot callback borrows state. Destruction calls blocking `UnregisterWaitEx(..., INVALID_HANDLE_VALUE)` without holding the interest mutex, then frees state. Cancellation before callback and destruction during a callback both have regressions. |
| Pipe/close worker thread handles | Stored `JoinHandle`s; the close worker receives ownership only after successful spawn | Constructors return thread-creation errors instead of panicking. Disposal joins successful workers; join drops the native thread handle. Close-thread failure retains backend ownership for the fallback above. No permanent worker is added. |
| Packaged conpty.dll module reference | `Library` immediately owns each successful absolute-path `LoadLibraryExW`; function pointers retain that owner | Missing export drops the module. A successful API table retains it through the final close call; `FreeLibrary` runs once. The pinned DLL and adjacent host are exercised on both final measured cohorts; missing/invalid files return errors without inbox fallback. |

If synchronous wait unregistration unexpectedly fails for the exclusively owned,
valid registration, the destructor logs metadata and conservatively retains its
context/process to avoid freeing memory still reachable from Windows. This is
not a successful cleanup guarantee for corrupted/unexpected OS state. Actual
registration failures, normal callback completion, cancellation and in-flight
callback joins are covered. No terminal payload or environment value is logged.

`CreatePseudoConsole` HRESULT failure is returned without asserting. HRESULTs
retain their hexadecimal value rather than being misread as `GetLastError`.
`OnResize` cannot return an error through the existing trait, so a failed resize
logs an API error and keeps the session alive. `CreateProcessW` receives writable
UTF-16 command-line storage. All new unsafe operations have ownership/lifetime
comments and use the installed windows-sys 0.59 signatures. There are no Cargo
dependency/version/lockfile changes; two existing Windows SDK feature groups
are enabled only for test process-memory/thread observations. Follow-up #55
pins a native ConPTY distribution outside Cargo, justified by the SDK control.

## Close and drain contract

[Microsoft's close contract](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole)
states that before Windows 11 24H2/build 26100, close can block until the console
exits. Starting at build 26100 it returns immediately, and EOF is the indication
that attached clients have disconnected. Final output can arrive during close.

Normal/natural disposal preserves the #7 ordering: close in the transient
worker, drain concurrently through the existing reader, join the close and pipe
workers, wait for the root process, then unregister the callback and release its
process handle. Before pipe workers exist, failed startup closes its output
read end before closing the console, which Microsoft also permits. Emergency
close-worker failure intentionally discards pending output and does not promise
a natural output drain. No destructor joins on the UI in an owned running
session; the existing synchronous constructor's error rollback can block, and
moving startup off the UI remains issue #10.

Attached descendants are covered by ConPTY shutdown. Microsoft documents this
in its [session creation/teardown guide](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session).
The three-process synthetic tree passes normal close, full-output close and
injected close-worker failure on both measured server cohorts. There is no
demonstrated need for a Job Object. Detached descendants are not owned/killed
by this policy and remain untested.

The release natural-drain fixture emits over 180 KiB followed by a synthetic
sentinel, exits with code 37, and verifies both sentinel and EOF through shutdown.
The existing adapter lifecycle probe separately verifies parsed final content.
These are synthetic checks, not a promise to gracefully flush explicit cancel.

## Historical inbox reproduction and evidence

Reproduction commit `e168423` added measurements before the ownership patch.
[Baseline run 40303916025](https://github.com/shakodev/pike/actions/runs/40303916025)
failed on both cohorts with linear growth. Each scenario warms up ten times,
then measures 100 cycles in an isolated release test process, sampling host
handles, private bytes, thread count and elapsed time after each cycle.

| 100 cycles: host handle delta | Server 2022 build 20348, before | Server 2025 build 26100, before |
| --- | ---: | ---: |
| Missing executable | +300 | +300 |
| Invalid working directory | +300 | +300 |
| Successful creation/close | +300 | +200 |

The independent SDK control runs only pipe creation, `CreatePseudoConsole`,
pipe destruction and `ClosePseudoConsole`, without Pike backend construction,
process creation, attributes, wait registration, dynamic loading or workers.
[Control run 40305308731](https://github.com/shakodev/pike/actions/runs/40305308731)
measured +100 handles on build 20348 (94 to 194) and zero on 26100 (82 to 82).
The control's private bytes and threads were flat on both. This proves the
remaining older-cohort growth exists outside Pike's ownership code. Accessing
opaque HPCON internals or closing guessed native handles would risk invalid
memory access/double close and is not part of this patch. #55 tracks a supported
OS/deployment remedy; #8 now lists that prerequisite.

The CI workflow retains raw synthetic artifacts even on failure and runs the
control, descendant tests and backend regressions despite a strict resource
failure. Ignored probe tests are deliberately invoked by exact name; the child
fixture must never be run with an unfiltered `--ignored` invocation.

```powershell
cargo test --release --locked -p alacritty_terminal tty::windows::conpty::tests::resource_cycles -- --ignored --exact --nocapture --test-threads=1
cargo test --release --locked -p alacritty_terminal tty::windows::conpty::tests::sdk_control -- --ignored --exact --nocapture --test-threads=1
cargo test --release --locked -p alacritty_terminal tty::windows::conpty::tests::attached_tree_and_flood -- --ignored --exact --nocapture --test-threads=1
cargo test --release --locked -p alacritty_terminal tty::windows::conpty::tests::natural_output_drain -- --ignored --exact --nocapture --test-threads=1
```

Fault injection is test-only and scoped to the calling thread. It simulates
failure at both pipe creations, ConPTY creation, attribute size/init/update,
process creation, wait registration after allocating callback state, both
pipe-worker creations, and close-worker creation. SDK invalid-size calls
exercise actual failing ConPTY creation/resize. There is no production
environment-controlled injection or polling timer.

These are active headless release probes. They do not measure GUI idle CPU,
renderer memory, whole-process-tree resources or user interaction latency.
Windows 10 and Windows 11 client OS cohorts, other ConPTY versions, detached
descendants and real system-wide resource exhaustion remain unverified. The
worker-failure regressions inject deterministic creation errors instead of
exhausting a shared CI host.

## Verification

Local macOS arm64 with pinned Rust 1.97.1: formatting, root all-target Clippy
with warnings denied, root tests (4), example tests (5), explicit Alacritty
Clippy and tests (146 unit, 45 references, one doctest), and explicit iced_term
Clippy/tests for the wgpu profile (44 unit) pass. Windows-target all-target
Alacritty Clippy also passes. Existing macOS `block` dependency reports a future
compatibility warning; no new warning or dependency change was introduced.


Final source verification and measurements use `75bdda7` in
[audit run 40305577911](https://github.com/shakodev/pike/actions/runs/40305577911).
Both cohorts pass 158 Alacritty unit tests, 45 reference cases and one doctest;
five isolated/fixture probes are excluded from the ordinary suite and invoked
individually by CI as appropriate. Server 2025 passes the strict resource probe.
Server 2022 fails only that probe, with +100 handles in every 100-cycle scenario
that successfully creates a console, matching its independent SDK control.
Failures before console creation have zero handle growth on both cohorts.

Raw before/after measurements, OS/compiler metadata, control output and test
transcripts are committed in [baselines/issue-8](../baselines/issue-8/), with
source/run provenance. The final resource process measured 100 cycles each of
missing executable, invalid cwd, successful close, and ten injected startup
failure points. All scenarios have zero handle growth on Server 2025. Across
each cohort's 13 measured scenarios, no scenario gains a thread; private-byte
changes are 0–28,672 bytes on Server 2022 and 0–61,440 on Server 2025. These
small allocation changes are reported as observed, not treated as an absolute
memory bound. The successful-close series changes by 24,576/53,248 private
bytes (Server 2022/2025) and retains six threads at both its first and last sample.

| Release observation | Server 2022 / 20348 | Server 2025 / 26100 |
| --- | ---: | ---: |
| Attached root/child/grandchild closed, one tree | 4.260 ms | 5.748 ms |
| Same tree closed under flood, one tree | 6.749 ms | 5.780 ms |
| Same tree with injected close-worker failure, one tree | 4.511 ms | 5.032 ms |
| Natural drain, >180 KiB plus sentinel, code 37, EOF | 132.847 ms | 53.521 ms |
| Host CPU / wall for 100 successful create/close cycles plus diagnostics | 1031.250 / 1490.339 ms | 1421.875 / 1965.530 ms |

Tree timing includes disposal and verifying all three previously opened process
handles are signaled. Natural timing includes startup, output and disposal.
Each tree/drain timing is one observation, not a percentile claim. CPU counters
cover the host test process and include expensive diagnostic sampling and CSV
output: the thread snapshot enumerates the system on every sample. Resource
CSV cycle latency also includes that sampling. These counters cannot establish
Pike startup cost, low idle CPU or a renderer comparison. No diagnostic sampler
is included in production builds.


The complete [Windows renderer matrix run 40305578096](https://github.com/shakodev/pike/actions/runs/40305578096)
passed on source `75bdda7` for both wgpu and tiny-skia: formatting, feature
isolation/guards, application Clippy/tests, example/support tests, explicit
adapter and Alacritty tests/Clippy, release app/example builds, bounded-input
and shutdown-lifecycle integrations, and dependency-file cleanliness. The
subsequent evidence/documentation commit changes no Rust or dependency files.
Local explicit adapter Clippy also passes for tiny-skia. That inbox-only audit remained blocked at this historical checkpoint; follow-up
#55 resolves it with the maintained runtime. The final runtime, strict audits
and both renderer jobs pass on source `54a71a9` (see issue #55 verification).


The final PR review also gates shell execution until setup succeeds, uses an
Arc-derived callback pointer across owner moves, and validates x64 executable
inputs to packaging. The original measurements above remain tied to their
recorded source. See [review verification](issue-8-review.md) for reproduction,
final checks, and the added resume-failure resource scenario.
