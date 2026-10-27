# Native writer-candidate wave and independent deadline correction

[Run 40507568597](https://github.com/shakodev/pike/actions/runs/40507568597)
at exact source `d405c6dfac2a82eb11434de27aa1bf59162eb701` passed wgpu trials
01–05, then failed trial 06 and stopped. All ten tiny-skia trials passed.
`all-attempts.json` retains all sixteen launches, their bytes/events/cleanup
and failure-only diagnostic stimulus. The manifest hashes all 301 original
downloaded files. Selected provenance files are UTF-8/LF copies.

Both origins in the failed launch completed the exact initial nine-response
stream (`replies-1.bin` and `replies-2.bin`). Both then stalled in
`metadata-query-flood`, with console mode 512 at every recorded boundary.
Only six / five of ten flood color queries reached the formatter. Before any
failed-verdict padding, writer status was:

```text
20001: outstanding=0, queued=0, written=347, closed=false
20002: outstanding=0, queued=0, written=323, closed=false
```

The raw child traces confirm the initial replies and those six / five flood
responses, followed by marked diagnostic padding. Both children then exited
80 with drained natural cleanup; the parent still failed and wrote no success
marker. Post-success PID assertions were not reached for that failed trial.
This differs from the earlier `5997177` failure with 148 bytes still queued.
It demonstrates missing query forwarding before formatting, not lost admitted
writer bytes. The fixture emits no synchronized-update sequence, so the
deadline correction below is not claimed to explain this native failure.
Implementer C owns the separate read-budget/readiness prerequisite investigation.

Independent review also demonstrated an expired synchronized-output deadline
could force immediate waits while partial input remained backpressured. The
original public-API probe at d405 recorded 10,392 writer calls after 300ms and
withheld its output sentinel. Its source, runner, red result and the exact
candidate compile/link invocation are retained here. These are **macOS**
regression records, not Windows behavior claims. The corrected local source
records three attempts, sentinel flushed, queued zero / outstanding two, and
requested cleanup under the identical probe. The helper's unused-import warning
is from the preserved independent scratch source, not a production lint waiver.

The correction handles the existing expired deadline before the idle decision,
independently of queued/partial input and readiness. Future deadlines remain
pending without early flushing or a wakeup. A deterministic matrix covers
future/expired deadlines, idle/queued/retained-partial input, notification-only
wakes and actual socket readiness, plus the existing backpressure/error/cleanup
tests. No timer, thread, self-notification or reader continuation is added.
Final local formatting, both application/adapter Clippy profiles, application/
example/doc tests and all 154 Alacritty unit / 45 reference / one doc tests pass.
The previously recorded macOS adapter cleanup failures remain explicit.

The deadline patch is committed for independent review while pushes remain
held for the C-owned prerequisite. Native acceptance remains blocked; the
historical 938 trace still does not establish its exact interleaving.
