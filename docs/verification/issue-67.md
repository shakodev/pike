# Issue #67 — buffered PTY read continuation

Based solely on merged main `103e29552148aeb2273996e0d178fa781f6ec680`.
This prerequisite imports no unmerged #20 protocol, writer or deadline code.
It preserves the existing PTY, parser, session identities and lifecycle owners.

## Cause and behavior

The existing read service returns `More` after its 65,535-byte threshold.
The caller discarded that result and re-registered only when writable interest
changed. Windows UnblockedReader uses a fixed 1 MiB piper buffer. Locked piper
0.2.5 removes its reader waker while data is available; polling 3.11.0 consumes
each custom IOCP completion packet once. `PollMode::Level` does not manufacture
another notification for bytes still in that userspace buffer.

One worker-local pending bit now preserves `More`. Before the next wait it
notifies the existing poller, then adds a readable opportunity alongside normal
events. Duplicate source events cannot execute another read budget in the same
turn. Messages, writes, child exit and end-of-turn interest handling still run.
Pending/EOF/error stops continuation. Exactly-budget output receives one final
read to reach Pending and register piper's waker for later output. A wait
interrupted after consuming its notification is re-notified on the next turn.

This adds no timer, zero-timeout polling, thread, subscription, output buffer,
PTY or production dependency/feature. The original per-service threshold and
per-read terminal lock boundary are retained. A test-only piper dependency
exercises the existing locked Windows package on portable hosts; Cargo.lock
and production feature selection stay unchanged.

## Causal evidence and regression coverage

[Original investigation evidence](../baselines/issue-67/investigation/)
retains the actual-piper public-loop scratch source, baseline red and positive
control logs, exact source hashes and artifact normalization provenance.
On unmodified main, one notification/read consumes 65,535 bytes and strands
131,074 bytes with the tail status query unparsed. Unrelated metadata notify
does not rescue it; readable packets without new producer bytes do. Below-cap
and harness rearm controls pass. The committed regression also failed before
the production correction; its original output is retained separately.

The portable tests use the actual EventLoop, parser and piper, with socket
notifications standing for consumed completion packets. No child or fixture
reply parser can explain their progress result. Five tests cover:

- Below-budget and three-budget output with an exact tail-query response,
  no fresh producer output, unchanged input counters and no recurring read or
  application wake work after reaching Pending.
- Exactly-budget exhaustion followed by future producer output, proving the
  reader waker is armed rather than relying on residual nonempty readiness.
- Queued resize and actual input service between bounded reads, allowing the
  existing read-then-write ordering for an event carrying both flags.
- Tail preservation through EOF/error, exhausted I/O deregistration and close.
- Requested shutdown after the first budget without reading the remainder.

The Windows-only test constructs the real UnblockedReader and blocks its source
after all 196,609 bytes are proven in piper before first registration. The
actual IOCP registration/parser must deliver the tail query before the source
is released. Cleanup releases and joins the existing reader even if the oracle
fails. This proves the native adapter contract, not the exact scheduling of a
ConPTY host in the historical failure. Existing ConPTY audit tests separately
exercise attached descendants, output flood, natural draining and cleanup on
Windows 2022/2025; normal Windows CI runs both renderer profiles and their
explicit adapter suites, input-backlog and shutdown probes.

## Reproduction and validation

Use stable Rust 1.97.1, `CARGO_BUILD_JOBS=4` and this worktree's target directory.
Both renderer profiles are checked sequentially. Commands include:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo test --locked -p alacritty_terminal
cargo clippy --locked -p alacritty_terminal --all-targets -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
```

Repeat root checks with `--no-default-features --features renderer-tiny-skia`
and adapter checks with `iced/tiny-skia,iced/tokio`. Native Windows preparation
uses the existing pinned ConPTY script and runtime; CI job concurrency is four.
Windows-target Clippy on macOS is API checking only, never execution evidence.

Local formatting, denied-warning Alacritty/app/adapter Clippy and root/example
tests pass for both profiles. Explicit Alacritty checks pass 151 unit tests,
45 reference tests and one doctest. Native Windows adds its platform-specific
tests, including the actual IOCP regression. Exact implementation SHA, native
run links, outcomes and retained logs are recorded in the PR and handoff.

The explicit adapter suites on this macOS host have existing bounded cleanup
timeouts. Their complete failures and an untouched main103 archive comparison
are retained with the handoff; no deadline/assertion is increased or disabled
and no unrelated Unix cleanup correction is included. A baseline comparison
does not substitute for native acceptance.

The candidate records 80 passes / seven failures for wgpu and 79 / eight for
tiny-skia; main103 records 81 / six and 80 / seven respectively. Scheduling
changes which cases time out in a full parallel suite. The two names failing
only in a candidate full-suite sample were also run individually on untouched
main103 and on the candidate: `repeated_close_during_output_has_one_backend_result_and_fused_stream`
and `font_changes_measure_once_and_paint_only_changes_do_not_snapshot` both
time out at the same existing five-second cleanup assertion on both sources.
[Complete local logs and source hashes](../baselines/issue-67/local/) preserve
these failures rather than treating the local adapter suites as passing.

## Integration and limits

Merge this prerequisite independently before #20 integrates it and runs its
strict final combined native protocol fixture. The shared changes are pending
read state, wait notification/empty-wake gating, readable continuation, and
one read service per turn. #20 must still prepare deadlines every turn,
including productive read continuation, and run its end-of-turn writer path.
This issue neither imports that policy nor resets #20's review accounting.

Historical d405 trial 6 had completed initial replies and empty input queues,
but lacked read-turn/pipe-occupancy traces. The demonstrated inherited defect
is compatible with that failure; this PR does not claim to have established
its cause. Actual Windows tests/ConPTY probes do not prove physical desktop,
monitor transitions, interactive latency or GUI idle memory/CPU budgets.
Those claims are not made. Issue #21 remains blocked until #20 actually merges.
