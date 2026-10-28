# Buffered native failure at unchanged executable source

[Native run 40503069279](https://github.com/shakodev/pike/actions/runs/40503069279)
at `59971770792cd5c9eb6436d7599fca4f9c11a58f` failed wgpu trial 01 and
stopped that profile immediately. All ten tiny-skia trials passed. This head
changes only documentation/evidence from `7fb8683`; the executable, vendor and
workflow sources are unchanged. All eleven attempted launches are preserved in
`all-attempts.json`, including the failed verdict and diagnostic stimulus.
The manifest hashes all 207 original downloaded files; selected provenance is
also retained as UTF-8/LF text. No failing trial was retried.

The failed trial's session 20001 passed every strict query, metadata, focus,
status-41 and drained completion check. Session 20002 passed sanitized title,
reset title and bell, then stalled before its initial query batch completed.
Its formatter trace contains every expected initial response in order and with
the correct session-specific geometry/palette. Before any diagnostic padding,
the parent observed:

```text
session=20001, InputStatus { outstanding_bytes: 0, queued_bytes: 0, written_bytes: 456, cancelled_bytes: 0, closed: true }
session=20002, InputStatus { outstanding_bytes: 148, queued_bytes: 148, written_bytes: 55, cancelled_bytes: 0, closed: false }
```

Thus 148 bytes admitted to the existing bounded FIFO had not progressed to
the OS writer after the unchanged 30-second deadline. The 55 acknowledged
bytes equal startup DA + STRB handshakes + two geometry replies + the first
foreground reply; this arithmetic does not establish child consumption.
There is no session-20002 child-input/mode trace: failure-only marked padding
did not obtain a persisted child trace before requested shutdown. Its cleanup
acknowledgement is `Requested,None,false`; session 20001 is
`ChildExit,Some(41),true`. The failed process exits 1 and has no success marker.
The passing-profile workflow checks both fixture PIDs absent after completion;
the failed profile retains acknowledged backend cleanup but did not reach its
post-success PID assertion. Do not infer unrecorded child bytes or console mode.

This failure narrows the problem from unknown query/read progress to an
admitted FIFO writer-progress failure. It does not yet establish the exact
runtime interleaving. Source inspection identifies an actionable candidate:
the loop attempts writes only on readiness events and rearms writable interest
only when its boolean changes, while pinned piper retains a writer waker only
when full. A notify-only admission wake while writable interest stays enabled
can therefore require explicit nonblocking write servicing. A deterministic
regression and independent review are required before claiming a cause/fix.
The separate historical `938a48b` trace remains less informative and is not
retroactively claimed to prove this same interleaving.

The local portable regression now proves the failed phase-state behavior:
after consuming a real `Poller::notify()` with an empty readiness batch, the
faithfully extracted original block leaves all 148 bytes queued despite a
ready nonblocking sink. `deterministic-red.patch`, its provenance and red log
preserve exact reconstruction from base `5997177`. This is a deterministic
state regression, not a claim to record the original thread interleaving.
The candidate services pending input before updating interest and includes
retained partial bytes in the idle-wake predicate. Three new tests cover the
observed state, full-capacity FIFO ordering/zero/WouldBlock/no idle writes, and
notify-only retained-partial progress with propagated write error, exact
cancellation and disposal-before-completion. Native candidate verification
and independent review remain required.
