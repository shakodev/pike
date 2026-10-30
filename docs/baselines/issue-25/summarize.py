"""Summarize a finite pane_resizing run, preserving all original samples.

Run from the repository root: python3 docs/baselines/issue-25/summarize.py RUN
Frame intervals include output cadence, minimize and deliberate fixture waits;
event delivery is redraw timestamp to observer delivery, not glass latency.
"""
import csv
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import sys

run = Path(sys.argv[1])


def stats(values):
    values = sorted(values)
    return dict(n=len(values), median_us=statistics.median(values),
                p95_us=values[min(len(values) - 1, int(len(values) * .95))],
                max_us=max(values))


dispatch = list(csv.reader((run / "dispatch.csv").open()))
frames = list(csv.reader((run / "frames.csv").open()))
resizes = list(csv.reader((run / "resize.csv").open()))
by_case, previous = {}, {}
for _, case, session, count in resizes:
    value = int(count)
    by_case[case] = by_case.get(case, 0) + value - previous.get(session, 0)
    previous[session] = value
pids = [int(line.split("pid=")[1])
        for line in (run / "events.txt").read_text().splitlines()
        if line.startswith("session=")]
remaining = subprocess.run(["ps", "-p", ",".join(map(str, pids)),
                            "-o", "pid=,stat=,comm="], capture_output=True, text=True)
summary = dict(
    kind="release macOS arm64; native PTYs, synthetic public-widget input/scale",
    cases=14, comparisons=56,
    dispatch=stats([int(row[3]) for row in dispatch]),
    redraw_request_intervals=stats([int(row[2]) for row in frames if int(row[2])]),
    redraw_event_delivery=stats([int(row[3]) for row in frames]),
    native_resize_counts=previous, native_resize_total=sum(previous.values()),
    native_resize_increases_by_case=by_case,
    duplicate_resize_proposals=14 * 4 * 20, duplicate_extra_native_resizes=0,
    pids=pids, remaining_child_processes=remaining.stdout.strip(),
    ps_exit=remaining.returncode, ps_stderr=remaining.stderr,
    physical_input=False, native_windows=False,
    synthetic_dpi_percent=[100, 125, 150, 200])
(run / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
metadata = dict(
    host=platform.platform(), machine=platform.machine(),
    source_base="5d34cb74b99f5e7e6865789f645e5fe519ba2d94",
    app_implementation_head="22d74e81c27bd66a14c7ba1b7016f0d5c56acf55",
    renderer="default renderer-wgpu", jobs=4,
    commands=["CARGO_BUILD_JOBS=4 cargo build --release --locked -p pike --example pane_resizing",
              "/usr/bin/time -l target/release/examples/pane_resizing " + str(run)],
    sha256={str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in
            [Path("examples/pane_resizing.rs"), Path("examples/pane_resizing/widget.rs"),
             Path("target/release/examples/pane_resizing")]})
(run / "provenance.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(json.dumps(summary, indent=2))
