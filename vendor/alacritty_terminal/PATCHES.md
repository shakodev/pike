# Alacritty terminal patch ledger

## Issue #67 — bounded buffered-read continuation

- Preserve `ReadStatus::More` in one worker-local bit. Notify the existing
  poller before its next wait and append one readable service opportunity to
  that turn's normal events. Read at most one existing service budget per
  turn, including when a kernel event and continuation coincide. Channel,
  write, child-exit and end-of-turn handling retain their normal paths.
- Pending/EOF/error stops continuation. An exactly-full read gets a final
  attempt to reach Pending and arm piper's reader waker; Interrupted waits
  cannot consume the only continuation wake. No idle timer, zero-timeout poll,
  extra worker, queue, PTY, production dependency or lockfile change.
- Add portable real-piper event-loop regressions and a native Windows test
  using the actual UnblockedReader/IOCP with output fully buffered before
  registration. Test-only piper uses the already locked 0.2.5 on all hosts.
  See [verification](../../docs/verification/issue-67.md) for baseline red,
  source provenance, native execution requirements and precise limits.

## Provenance

- Published crate: **alacritty_terminal 0.25.1**, retaining every archive file,
  original manifests/lockfile, tests, and Apache-2.0 license in `LICENSE-APACHE`.
- crates.io archive SHA-256:
  `46319972e74179d707445f64aaa2893bbf6a111de3a9af29b7eb382f8b39e282`.
- Upstream revision: `12082407d9cb3384a2bd83c578e556ec3faa2c4d`, retained in
  `.cargo_vcs_info.json`; archive path in that repository: `alacritty_terminal`.
- Upstream: <https://github.com/alacritty/alacritty/tree/12082407d9cb3384a2bd83c578e556ec3faa2c4d/alacritty_terminal>.
- Root `[patch.crates-io]` selects this copy. Workspace tests use Pike's root
  `Cargo.lock`; the packaged lockfile is retained solely as provenance.
- The large generated reference fixtures retain their exact archive bytes;
  `.gitattributes` disables textual diffs for `tests/ref/**` only. They remain
  present and are exercised by all 45 reference tests, outside the executable.
- The archive's ignored per-package `resolver = "2"` is removed from normalized
  `Cargo.toml`; Pike's workspace already selects resolver 3. No runtime
  dependencies or dependency versions change. The existing upstream
  `serde_json` dev-dependency resolves in the root lockfile for explicit tests.

## R06 / issue #6: bounded input through the actual writer

- Add `src/input.rs`: a 2 MiB per-session outstanding byte budget, whole-input
  preflight/admission, ordered 8 KiB packed chunks, and redacted rejection/status
  types. No input payload enters the unbounded std message channel or write list.
  Retained source capacity is bounded by the budget plus one partial chunk;
  one in-flight source chunk and the existing fixed Windows pipe remain bounded.
- `src/event_loop.rs`: preserve `Msg::Input` compatibility by routing it through
  byte admission. The loop takes one chunk at a time, without a write-list queue.
  Synchronous PTYs acknowledge actual write return values. Shutdown closes
  admission, clears queued chunks, and wakes the existing loop directly, ahead
  of metadata messages. It explicitly cancels unwritten input; it does not
  reorder accepted healthy input or promise a graceful flush. Loop destruction
  closes admission on natural exit/error/unwind as well.
- `src/tty/mod.rs`: an optional `input_writer_progress` trait hook for buffered
  writers, with a default for synchronous implementations.
- `src/tty/windows/{mod.rs,blocking.rs}`: track successful actual `AnonWrite`
  return values, including partial writes. Enqueueing into piper does not release
  budget. Worker completion/failure closes admission and wakes the loop even
  without writable interest. A broken piper writer reports BrokenPipe.
- The existing Alacritty PTY/parser/polling and Windows workers are retained.
  No second PTY, new runtime worker, polling subscription, or production timer.
- Eleven new unit regressions cover atomic rejection, payload/counter bounds,
  chunking/UTF-8/order, packed tiny writes, concurrent admission, failed wakes,
  cancellation, legacy sender routing, and Windows buffering/partial failures.

## Stable formatting

The archive was formatted with upstream nightly-only settings. The added
`rustfmt.toml` uses their stable equivalents so the required stable
`cargo fmt --all -- --check` includes this workspace member. Mechanical import,
macro and expression formatting also changes `src/{index,selection,vi_mode}.rs`,
`src/grid/{resize,row,storage}.rs`, `src/term/{cell,mod,search}.rs`,
`src/tty/unix.rs`, and `src/tty/windows/{child,conpty}.rs`. These files have no
functional changes. Archive hash comparison records the exact patch boundary.

## Verification

```powershell
cargo test --locked -p alacritty_terminal
cargo clippy --locked -p alacritty_terminal --all-targets -- -D warnings
```

The Windows suite contains 145 unit tests (134 upstream plus 11 new), 45
upstream terminal-reference fixtures and one doctest. Run the root renderer
checks and ConPTY probe described in
[issue #6 verification](../../docs/verification/issue-6.md) as well.

## R07 / issue #7 — dispose off thread and drain natural exits

- Factor the existing event-loop body into `run`. `spawn` retains upstream
  ownership semantics; new `spawn_disposing` invokes source-stop notification,
  drops parser/returned backend state on the PTY worker, then publishes one
  redacted completion. Its detached JoinHandle contains only (). Catch unwind
  to report worker panics after resource destruction; register/poll/reregister
  and I/O failures also produce a reason without requiring terminal Exit.
- Senders hold a Weak poller reference so retained model/completion handles
  cannot keep the backend's OS polling handle alive after disposal. Explicit
  shutdown is distinguished from actual-writer failure with shared atomic state.
- Natural exit drains every parser chunk to EOF, including pending Windows
  pipe data. Parse each successful read before a later read can fail. The drain
  has a one-second deadline and reports TimedOut without claiming EOF. OS
  destruction can remain pending longer; no false completion is emitted.
- Add a default `EventedPty::begin_shutdown` hook. ConPTY closes in one transient
  worker while the existing reader/parser continue draining. PTY destruction
  drains/discards unread output and joins that close worker plus the two existing
  pipe workers. There is no additional worker or timer during normal operation.
  `UnblockedReader::read` distinguishes WouldBlock from EOF (its compatibility
  `try_read` remains unchanged). Close-on-full-pipe cleanup never waits on UI.
- Keep the two parent ConPTY pipe ends under RAII and drop them after successful
  CreateProcess. Their previous `into_raw_handle` leaks prevented EOF forever;
  this narrow prerequisite is required for joined reader cleanup. The remaining
  Windows handle/failure-path audit, callback ownership and detached-descendant
  policy stay in issue #8. See Microsoft's
  [creation guide](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)
  and [close contract](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole).
- Make the Windows-only writer completion helper cfg-specific and simplify the
  Unix passwd fallback to satisfy the pinned compiler's existing Clippy lints
  on the macOS verification host. Shell fallback behavior is preserved.

- Windows CI reproduced an indefinitely pending output reader when explicit
  close raced console attachment, even though ClosePseudoConsole returned.
  Explicit disposal now terminates the existing owned root process first and
  waits for its exit off thread. Natural EOF-draining exits retain their own
  path. This is cancellation, not a graceful child-shutdown guarantee. An
  opt-in `PIKE_TRACE_CLEANUP` trace records only worker stages/thread IDs.
- On poller preparation failure, move the already-created PTY to a transient
  disposal worker rather than invoking its blocking destructor on the UI.

## R07 review fixes

- On EOF/hangup, close input and remove exhausted PTY I/O from the poller while
  retaining the child-exit watcher and shutdown wake. This prevents repeated
  level-triggered EOF wakes and preserves a delayed child code. Queued resizes
  are ignored after EOF, and explicit close still wakes the remaining wait.
  `EventedPty::deregister_io` has platform implementations and an explicit
  Unsupported default for other implementations. Unix teardown remains
  idempotent after the I/O registration has already been removed.
- A Windows reader completion guard marks source unwind as an I/O error before
  the piper writer drops. A panicked reader can no longer masquerade as a clean
  EOF and set `output_drained=true`. No production polling or dependency changes.
- A portable two-socket fixture verifies one EOF read, delayed child exit/code,
  ignored queued metadata and explicit close after EOF. It models readiness,
  not another PTY. Windows also tests an injected reader panic.

- Flush active synchronized updates on I/O failure or explicit close before
  dropping the parser. Already-read output is retained even without Exit; a
  read-then-broken-pipe regression verifies the synchronized final sentinel.

## R08 / issue #8 — Windows ownership and startup failure audit

- Wrap HPCON immediately, close all startup error paths with safe output/console
  ordering, and adopt both CreateProcess handles into Rust ownership. Close the
  unused primary-thread handle immediately and retain the process until callback
  join and backend completion.
- Own initialized attribute lists in aligned storage and call
  DeleteProcThreadAttributeList before freeing them on success and failure.
- Watchers own stable callback state, reclaim failed registrations and unfired
  callbacks, synchronously unregister/join before releasing the process, and wake
  the poller when exit preceded registration. Cover in-flight callback teardown.
- Return ConPTY HRESULT and pipe-worker creation errors; failed resize preserves
  the session. Retain backend ownership if close-thread creation fails and use
  the existing reader to release the output pipe during synchronous disposal.
- Own optional DLL references across their function-pointer use and free on
  partial export lookup or final backend destruction. Remove environment values
  from duplicate-variable diagnostics and pass mutable storage to CreateProcess.
- Add isolated release resource, SDK-control, natural-drain and attached-tree
  fixtures. Test-only SDK memory/thread counters use existing windows-sys 0.59;
  no runtime dependencies, versions, or lockfiles change.
- The initial application fixes had zero linear handle growth on Server 2025/26100.
  Server 2022/20348 leaks one handle per bare SDK console cycle, reproduced
  independently of Pike. Follow-up #55 resolves that application-path blocker
  with the maintained ConPTY runtime; both strict cohort checks now pass. No opaque HPCON access or second
  PTY is introduced. See [audit and measurements](../../docs/verification/issue-8.md).


## Issue #55 — pin the maintained native ConPTY runtime

- Require the app-local Microsoft.Windows.Console.ConPTY 1.25.260930003 DLL and
  host pair; use its documented Conpty-prefixed exports and SDK-compatible ABI.
  Load an absolute path with dependency search limited to that directory and
  System32. Missing/invalid files or exports return an error; no PATH search or
  inbox fallback can reintroduce the measured legacy leak.
- `tools/conpty-runtime.json` pins archive and x64 asset SHA-256 values.
  Preparation/packaging verifies bytes, ships both files and the MIT license,
  and supports offline restoration. No Cargo dependency or lockfile changes;
  the measured native assets add 1,208,984 disk bytes. See
  [runtime provenance/deployment](../../docs/dependencies/conpty/README.md).
- Runtime integration reproduced a previously hidden emergency-close flaw:
  abandoning output let the newer host exit before terminating descendants.
  The fallback now asks the existing reader to drain directly past piper
  backpressure until EOF, while close runs on the disposing thread. It adds no
  worker and checks the control flag only on reader backpressure. A focused
  regression proves EOF without a piper consumer; attached-tree tests cover it.
- The final Windows audits pass on build 20348 and 26100 with zero handle growth
  across all 13 measured scenarios. Bare inbox controls remain separate evidence
  of the OS defect. [Issue #55 verification](../../docs/verification/issue-55.md)
  retains measurements, the regression/fix, and validation limitations.


## R08 PR review — gate execution and stabilize FFI ownership

- Create the root process suspended and retain its primary-thread handle until
  wait registration and both pipe workers are owned. Resume only after the PTY
  owns normal teardown. Any earlier failure terminates a suspended process;
  resume failure uses full PTY disposal. Failed startup cannot execute shell
  commands or spawn descendants before returning an error.
- Store callback context in Arc and derive its FFI pointer with Arc::as_ptr,
  preserving shared access when the owner moves after registration. Avoid
  Box's unique-owner aliasing guarantees while Windows may run the callback.
  Blocking unregister still precedes releasing context/process ownership.
- A delayed synthetic side-effect regression reproduced the startup race on
  both Windows cohorts before this fix. Add resume-failure injection to the
  strict release resource series; no runtime dependency or extra worker.
- The x64 packaging script rejects another architecture, DLL or non-PE input
  before creating output. See [review record](../../docs/verification/issue-8-review.md).

## R09 / issue #9 — Windows launch preparation

- Prepare and validate executable, arguments, cwd and environment before any
  ConPTY resource allocation. Resolve to an explicit executable and supply
  `lpApplicationName` separately from writable, terminated UTF-16 command data.
  Quote argv[0] and retain the existing CRT encoder for ordinary arguments.
  Raw command syntax remains an explicit caller choice via `escape_args`.
- Resolve explicit unquoted .exe names against the session PATH then System32;
  paths resolve against session cwd. Extensionless names gain .exe; scripts
  require an explicit interpreter. No implicit cmd wrapper or cwd search for
  bare names. Automatic resolution prefers pwsh (PATH, then ProgramFiles),
  system Windows PowerShell, then system cmd. Never automatically select WSL.
- Reject invalid/missing explicit executables, invalid cwd, NUL arguments,
  oversized command lines, invalid environment entries and case-insensitive
  duplicate override keys with downcastable, payload-free `LaunchError` values.
  CreateProcess errors retain an OS code without user command data.
- Merge inherited UTF-16 environment entries without lossy conversion; preserve
  drive-directory entries, explicit empty values and inherited keys. Use Windows
  `CompareStringOrdinal` for case-insensitive overrides and sorted blocks, with
  double-NUL termination. No global cwd/environment edits or duplicate logging.
- Redact Debug for Options/Shell. Only enable two API modules in the existing
  windows-sys dependency (Globalization/SystemInformation); no new package,
  version or lockfile changes. All launch allocations are temporary; no new
  runtime worker, timer or idle work.
- Windows launch unit regressions and the release `launch_validation` example
  cover preparation, native argv round trips, two live sessions, private
  diagnostics, cmd, Windows PowerShell and installed PowerShell 7. See
  [verification](../../docs/verification/issue-9.md) for commands/results.

- Keep resolved executable paths in conventional absolute form using
  `std::path::absolute`. Avoid canonicalize's automatic verbatim prefix, which
  broke the legacy Windows PowerShell release fixture; assert that bare-name
  resolution does not add it. The explicit application-name argument still
  prevents command-line executable ambiguity.

## R20 / issue #20 — bound retained metadata and preserve protocol order

- Sanitize titles before model/title-stack retention: at most 4,096 UTF-8 bytes,
  no C0/C1 controls or directional formatting overrides. Cap saved titles at 64
  (256 KiB payload bound), independently preserving the keyboard-mode limit.
  Remove title-bearing trace output and redact Event Debug payloads for titles,
  replies and clipboard stores; no transcript is retained by this patch.
- RIS reset now sends ResetTitle instead of silently clearing only model state.
  A real-PTY fixture reproduced the missing application notification.
- Capture an OSC color override in its deferred response formatter at parser
  position. Later set/reset operations cannot change an earlier queued query;
  the adapter still supplies its current configured fallback palette.
- Add source-level title bound/stack/privacy regressions; adapter tests replay
  exact VT and verify delayed color formatting and title-reset events. Both
  workspace dependency suites run explicitly; see
  [issue #20 verification](../../docs/verification/issue-20.md).
- TextAreaSizeRequest now formats aggregate TextAreaSize virtual pixel fields
  rather than multiplying native integer cell dimensions. This keeps 8.5-pixel
  cells accurate while preserving the existing OnResize/WindowSize PTY contract.
- Service bounded pending input on an event-loop wake even when writable
  interest is already enabled. Poller notifications contain no readiness event,
  and pinned Windows piper retains a writer waker only while full; queued
  replies could otherwise stall with free writer capacity. Include a retained
  partial write when deciding whether a wake is idle. Backpressure keeps the
  current bytes and waits for capacity readiness without timers or extra work.
- Preserve a native 148-byte queued-reply failure, deterministic red/green
  admission regression, full-capacity FIFO ordering/zero/WouldBlock coverage,
  and a notify-only partial-write error/disposal test. No dependency or worker
  changes; native candidate validation is recorded in the issue verification.
- Expire the existing synchronized-output deadline before the idle decision,
  independently of queued/partial input and readiness. Preserve future
  deadlines without early output/wakeups. Deterministic notification/readiness
  cases and the independent public-API probe verify bounded attempts/output
  release under WouldBlock; no extra timer, thread or self-notification.


## R09 PR review — remove ambient drive state from executable lookup

- Normalize cwd to a full path once before validation/launch. Reject
  drive-relative executable, PATH and ProgramFiles lookup paths rather than
  using the parent's per-drive cwd after PathBuf::join discards the session base.
  Ordinary relative paths retain session-relative resolution.
- Add two native preparation regressions; the release fixture now covers a
  drive-relative cwd in one of its two live sessions and verifies the default
  shell's explicit close completion reason. No new dependencies or idle work.
