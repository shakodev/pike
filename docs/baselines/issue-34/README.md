# R34 partial checkpoint — owner requested finish and stop

This branch began at actual merged main
`a5fe9fc4d8f0df53db240155ecebe0088d9898f4` (PR81/R31). It is **not a completed
implementation of issue #34**. The owner stopped all agents before the agreed
A32/B33 integration and remaining host paths. No PR closing #34, independent
approval, merge, remote CI, Windows native/physical validation, or release
resource/performance claim is made.

The durable HANDOFF.md records source identity, exact local commands/results,
files, remaining gates, and the explicitly unimplemented resize/native timeout
plans. Existing branches, reports, original vendor provenance and target caches
are preserved.

Current source includes a bounded typed error model, opt-in in-memory metadata,
owner-checked App/terminal/startup/cleanup adapters, real clipboard-write
acknowledgements using the existing backend, finite renderer recovery, and a
same-state readiness gate. New passive error view methods are intentionally not
inserted into the shared pane/root chrome yet. Fresh paste retry and settings/
command hooks still require their agreed actual-main integration.

Diagnostics default off; `--diagnostics` opts into metadata only, at most 64
records for 15 minutes from first occurrence. Repeated errors coalesce without
extending retention. Expiration runs on admission, capture and enabled App
updates, without a timer. An idle process may retain expired allocations until
its next event/capture; expired data cannot be captured. Clear/disable release
the allocation immediately. No automatic file, upload, logger or transcript is
created. Explicit clipboard export uses the same acknowledged native writer.
Titles continue through the existing 256 Unicode scalar sanitizer; diagnostic
context accepts only typed enums and numeric metadata.

Important preserved evidence:

- `native-first-frame-initial.log`: RED, pre-ready preparation allowed Err before
  native disposal acknowledgement (one handle, zero completions at return).
- `native-first-frame-forced.log`: GREEN production gate, forced pre-ready user
  actions/late settings created zero children before first-frame error.
- `native-preparing-gated.log`: GREEN, one Starting session at arm; actual handle
  completion and pool shutdown before Err; no surviving controlled child.
- `native-attached-gated.log`: GREEN, both real child handles completed before
  Err; no surviving controlled children.
- `native-config-cancel.log`: GREEN, settings pending at arm; no late launch;
  pool cleanup completed before Err.
- `original-renderer-error-path.rs.txt`: original unbounded retry/log/OOM panic
  source, preserved before the finite policy change.
- Logs ending in `initial`, earlier `green`-named logs with a failure summary,
  and the initial runtime test log preserve actual compile/test reds. HANDOFF.md
  distinguishes these from corrected results; none is erased or claimed green.

All native-window/PTY evidence here is synthetic macOS testing with controlled
children, not physical input/monitor testing or native Windows proof. The
isolated fixture enables only the off-by-default renderer fault feature and
uses the same pinned crate versions and this worktree's reused target directory.
