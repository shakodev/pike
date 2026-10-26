# Issue #55: eliminate legacy inbox ConPTY handle growth

The fix uses Microsoft's pinned `Microsoft.Windows.Console.ConPTY`
**1.25.260930003** DLL and host through the existing Alacritty backend. The
strict resource audit now passes on both Server 2022 build 20348 and Server
2025 build 26100. This resolves the measured blocker for issue #8; the inbox
OS defect itself remains reproducible in the independent SDK control.

[Runtime provenance and deployment](../dependencies/conpty/README.md) documents
package/archive and per-file SHA-256 pins, license, the supported x64 packaging
policy, offline restoration, and preparation/packaging commands. The native
assets add 1,208,984 bytes (about 1.15 MiB) on disk. There is no Cargo dependency,
lockfile change, second PTY implementation, Job Object, updater or permanent
worker. The initial Windows target remains x86_64-pc-windows-msvc.

## Implementation

The adapter requires the matched `conpty.dll` and `OpenConsole.exe` beside the
running executable. It loads an absolute path with dependency search restricted
to that directory and System32, and uses the package header's Conpty-prefixed
exports. Missing files, invalid modules or missing exports return startup errors
and release acquired module ownership. It does not search PATH or fall back to
the leaking inbox API. Package hashes are validated during explicit preparation
and packaging, outside Cargo and the session I/O path.

PowerShell deployment checks verify offline restoration, idempotence, repair of
a modified asset, inclusion of the license, and rejection of a corrupt archive
before creating executable output directories. CI stages the runtime for debug
and release apps, dependency tests and examples, and successfully packages both
release renderer profiles with the verified native pair, license and manifest.
Generated native binaries remain under target; none are committed.

Integrating the newer runtime exposed an emergency-cleanup flaw in source
`d0bf4f0`. Normal attached-tree/flood close and all resource tests passed, but
injected close-thread creation failure left descendants alive. The fallback
closed the output stream before the host had finished sending client close
events. This reproduction is retained in
[run 40307609048](https://github.com/shakodev/pike/actions/runs/40307609048).

The corrected fallback uses the existing reader thread to drain directly to a
sink when piper backpressure would otherwise park it. The disposing thread can
then close synchronously while output remains serviced through client shutdown.
An atomic control flag reuses the existing registration allocation and is
checked only on reader backpressure; `unpark` handles the flag/park race. No
cleanup thread is needed in the failure path. Explicit cancellation still does
not promise preservation of unread output. A unit regression verifies reaching
source EOF without any piper consumer, and the three-process flood fixture
verifies actual descendant exit under injected close-thread failure.

## Windows measurements

Final Rust/script source: **54a71a9**. The
[resource audit run 40308238165](https://github.com/shakodev/pike/actions/runs/40308238165)
and [complete renderer matrix run 40308238467](https://github.com/shakodev/pike/actions/runs/40308238467)
pass. Each host-process scenario warms up ten times, then measures 100 cycles
in an isolated release test process. Raw observations, compiler/OS metadata,
staged runtime metadata, control output and test results are retained in
[baselines/issue-55](../baselines/issue-55/), with source/run provenance.

| Host handle growth over 100 cycles | Server 2022 / 20348 | Server 2025 / 26100 |
| --- | ---: | ---: |
| Original missing-executable launches, before #8 | +300 | +300 |
| #8 cleanup with inbox API, missing executable | +100 | 0 |
| Final pinned runtime, missing executable | **0** | **0** |
| Final pinned runtime, invalid cwd | **0** | **0** |
| Final pinned runtime, successful create/close | **0** | **0** |
| Each of ten injected startup failure points | **0** | **0** |
| Independent bare inbox SDK control | +100 | 0 |

The strict resource threshold and 100-cycle counts are unchanged; no allowance
is added for the old OS leak. Injection covers both pipe creations, console
creation, attribute sizing/init/update, process creation, callback registration
after allocating context, and both pipe-worker creations. Additional regressions
cover missing/invalid runtime files, actual invalid-size console calls, callback
cancellation/in-flight teardown, and close-worker failure.

| Final successful-close series | Server 2022 / 20348 | Server 2025 / 26100 |
| --- | ---: | ---: |
| Handles, first to last sample | 96 → 96 | 94 → 94 |
| Private bytes, first to last sample | 1,060,864 → 1,085,440 | 1,142,784 → 1,236,992 |
| Threads, first to last sample | 6 → 6 | 8 → 8 |
| Host CPU / wall, including diagnostics | 796.875 / 1563.494 ms | 1500.000 / 2759.424 ms |

Private bytes are flat from cycle 50 through 100 in both successful-close
series. Across all 13 scenarios, private-byte changes range from 0 to 24,576
bytes on Server 2022 and 0 to 94,208 on Server 2025. The latter's first
missing-executable series gains two threads at cycle 45, then remains flat;
subsequent scenarios gain none. These are observations, not proof of an
absolute process-memory/thread bound. CPU and cycle timings include expensive
system thread snapshots, handle/private-byte sampling and report output; they
cannot isolate terminal startup CPU or establish GUI idle compliance. The
sampler is test-only and the process counters exclude console-host descendants.

| Release closure/drain observation, one run each | Server 2022 / 20348 | Server 2025 / 26100 |
| --- | ---: | ---: |
| Attached root/child/grandchild all exited | 2.948 ms | 4.430 ms |
| Same tree closed under flood | 3.185 ms | 5.221 ms |
| Same tree with injected close-thread failure | 2.769 ms | 4.256 ms |
| Natural >180 KiB output, final sentinel, code 37, EOF | 23.654 ms | 37.717 ms |

Tree measurements include disposal and observing all three previously opened
process handles signaled. Natural timing includes startup/output/disposal.
These single observations are not percentile or hardware-independent latency
claims. All paths use existing pipe workers and the session's shutdown contract.
Attached descendants pass without a Job Object; detached descendants remain
outside the cancellation policy.

## Verification and limitations

Both Windows cohorts pass **160 backend unit tests**, 45 reference cases and
one doctest; five isolated probes/fixture tests are excluded from the ordinary
suite and invoked individually by exact name. Both strict release resource
probes, bare SDK controls, attached-tree/flood/emergency-close probes and natural
output-drain probes pass.

The full Windows matrix passes for wgpu and tiny-skia: formatting, feature
isolation/guard tests, all-target application and explicit dependency Clippy
with warnings denied, application/examples/support tests, explicit adapter
suite (46 tests), backend tests, release app/example builds, verified packaging,
bounded-input and adapter shutdown-lifecycle integrations, and dependency-file
cleanliness. Subsequent documentation/evidence changes do not alter this source.

Local macOS arm64 with pinned Rust 1.97.1 also passes formatting, root Clippy and
4 application tests, explicit backend Clippy/tests (146 unit, 45 reference, one
doctest), and the explicit adapter suite (44 tests). Windows-target all-target
backend Clippy passes. The pre-existing macOS `block` future-compatibility notice
remains outside this change.

Windows 10/11 client OS images, architectures other than x64, detached
processes, real system-wide thread exhaustion, GUI idle CPU/RAM and whole-tree
memory remain unverified. Thread-exhaustion paths use deterministic fault
injection instead of exhausting shared CI resources. The native distribution's
advertised minimum OS is not a substitute for client-cohort measurements.
