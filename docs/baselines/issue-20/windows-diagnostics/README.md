# Native protocol diagnostic waves

These records preserve both authorized diagnostic waves after the unexplained
combined-source `938a48b` timeout. Every independent launch is retained in
`all-trials.json`: ten per renderer at each source, 40 launches / 80 children.
No trial was retried to obtain a pass. Both waves passed all strict byte,
origin, query-count, title/reset/bell, focus, exit-status and cleanup oracles.
This does **not** establish a repair or explain the original failure.

| Wave | Exact source | Native run |
| --- | --- | --- |
| Synchronous trace writes | `3e5ba2f9522f65979a3423c75b607fefa3874c3a` | [40500354892](https://github.com/shakodev/pike/actions/runs/40500354892) |
| Buffered traces / console mode snapshots | `7fb8683e1d9af4dac1c931c4056c34ff8b53cb59` | [40502419892](https://github.com/shakodev/pike/actions/runs/40502419892) |

The JSON retains complete lossless child-input hex, formatted reply traces,
per-origin events, statuses and cleanup, console modes where captured, and
short-parent resource observations for every trial. All 80 complete child
inputs were independently compared with the fixture's strict expected stream
when producing this record. Every trial recorded two drained natural
completions with statuses 41/42, and the native workflow checked fixture PIDs
absent after acknowledgement. None used the failure-only diagnostic padding.

All 40 buffered-wave child traces report input mode 512
(`ENABLE_VIRTUAL_TERMINAL_INPUT`) at startup, before and after RIS, and at every
later phase boundary. This rules out an observed mode change in these passing
trials only. Startup phase-zero DA replies explain the earlier counter fix;
they do not explain the `938a48b` timeout. Runtime/vendor source is unchanged
between the two diagnostic waves; both modify only the opt-in fixture/docs
(the first wave also changes the focused workflow to serial launches).

`original-artifact-hashes.json` records SHA-256 and byte length for all 708
downloaded files, including full adapter logs, original source/runtime/system
and executable metadata, PIDs and original line endings. Selected provenance
files alongside it are UTF-8/LF copies. See the linked Actions artifacts for
original files and [the separate original failure](../windows-integration-failure/).
The resource samples cover a brief release probe parent, exclude child/GUI
resources, and do not establish idle budgets, latency, process-lifetime peaks
or renderer comparisons.

The `local-induced-timeout-*` records are explicitly **macOS fixture diagnostics**,
not Windows verification. A controlled SIGSTOP/SIGCONT held one synthetic
child across the existing 30-second deadline. The process exited 1 with
`Elapsed`, saved pre-padding writer counters and marked failure-only stimulus,
retained the child's partial input, omitted `completed.txt`, and acknowledged
two drained completions (diagnostic child status 80, other child status 42).
The one-second diagnostic release cannot turn a failed verdict into a pass.

The PR remains draft: the earlier native timeout has no reproduced byte trace
or demonstrated cause/repair. Real physical Windows desktop/monitor behavior
also remains unavailable; these headless records make no physical DPI claim.
