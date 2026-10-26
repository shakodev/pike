# Alacritty terminal patch ledger

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
