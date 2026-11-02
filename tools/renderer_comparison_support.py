"""Bounded evidence validation/statistics; no terminal transcript collection."""
import csv
import hashlib
import json
import math
from pathlib import Path

MAX_FILE = 2 * 1024 * 1024


def read_csv(path, limit=4096, max_bytes=MAX_FILE):
    path = Path(path)
    if path.stat().st_size > max_bytes:
        raise ValueError(f"oversized metadata: {path.name}")
    with path.open(newline="", encoding="utf-8-sig") as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) > limit:
        raise ValueError(f"too many metadata rows: {path.name}")
    return rows


def digest(path):
    value = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def distribution(values):
    values = sorted(float(v) for v in values)
    if not values:
        return {"count": 0, "median": None, "p95": None, "maximum": None}
    if any(not math.isfinite(v) or v < 0 for v in values):
        raise ValueError("invalid observation")
    # Nearest-rank percentiles, explicitly shared by every compared build.
    return {"count": len(values), "median": values[math.ceil(.5 * len(values))-1],
            "p95": values[math.ceil(.95 * len(values))-1], "maximum": values[-1]}


def feature_guard(text, renderer):
    features = {}
    for line in text.splitlines():
        if "|" not in line:
            continue
        package, enabled = line.split("|", 1)
        name = package.split(" v", 1)[0]
        features.setdefault(name, set()).update(enabled.split()[0].split(",") if enabled.strip() else [])
    for package in ("iced", "iced_futures", "iced_renderer", "iced_term", "tokio"):
        if package not in features:
            raise ValueError(f"missing {package}")
    other = "wgpu" if renderer == "tiny-skia" else "tiny-skia"
    forbidden = {"default", "thread-pool", "linux-theme-detection", "x11", "wayland", "web-colors", other}
    if features["iced"] & forbidden or renderer not in features["iced"]:
        raise ValueError("renderer isolation failed")
    if "thread-pool" in features["iced_futures"] or features["iced_term"] - {""}:
        raise ValueError("compatibility/executor defaults enabled")
    if features["tokio"] & {"full", "fs", "io-std", "io-util", "net", "process", "signal", "macros", "test-util"}:
        raise ValueError("unexpected runtime features")
    if not {"rt-multi-thread", "sync", "time"} <= features["tokio"]:
        raise ValueError("missing required shared runtime features")
    if (renderer == "tiny-skia" and "wgpu" in features) or (renderer == "wgpu" and "iced_tiny_skia" in features):
        raise ValueError("other renderer package leaked")


def validate_fixture(path, visible, hidden, saturated, flood, probes, seconds=None):
    path = Path(path)
    events = read_csv(path / "events.csv")
    total = visible + hidden
    select = lambda name: [r for r in events if r["event"] == name]
    if any(select(name) for name in ("deadline", "fixture_failure", "invalid_history_or_visibility", "incomplete_latency", "invalid_interval", "invalid_overlap")):
        raise ValueError("fixture reported failure")
    if not (path / "completed.txt").is_file() or len(select("finished")) != 1:
        raise ValueError("missing fixture completion")
    ready = select("ready")
    ids = {r["session"] for r in ready}
    if len(ids) != total or len(ready) != total or len({r["a"] for r in ready}) != total:
        raise ValueError("missing/duplicate session or child readiness")
    completed = select("completion")
    if len(completed) != total or {r["session"] for r in completed} != ids or any(r["a"] != "true" or r["b"] != "0" for r in completed):
        raise ValueError("missing/failed drained backend completion")
    begin, end = select("measure_begin"), select("measure_end")
    if len(begin) != 1 or len(end) != 1 or float(end[0]["ms"]) <= float(begin[0]["ms"]):
        raise ValueError("missing/invalid measurement interval")
    interval = float(end[0]["ms"])-float(begin[0]["ms"])
    epochs = select("measurement_epoch")
    if seconds is not None:
        expected = seconds*1000
        tolerance = max(250, expected*.01)
        if abs(interval-expected) > tolerance or len(epochs) != 1:
            raise ValueError("measurement interval is materially late or missing epoch")
        parent_start, parent_end = int(epochs[0]["a"]), int(epochs[0]["b"])
        if abs(parent_end-parent_start-interval) > tolerance:
            raise ValueError("wall/monotonic measurement clocks differ")
        windows = select("producer_epoch")
        if flood:
            if len(windows) != total or {r["session"] for r in windows} != ids:
                raise ValueError("missing producer interval identities")
            for row in windows:
                start, stop = int(row["a"]), int(row["b"])
                overlap = min(parent_end, stop)-max(parent_start, start)
                if abs(stop-start-expected) > tolerance or overlap < expected*.98:
                    raise ValueError("producer did not overlap the measured interval")
    before_rows, after_rows = read_csv(path / "counters-before.csv", 32), read_csv(path / "counters-after.csv", 32)
    before = {r["session"]: r for r in before_rows}
    after = {r["session"]: r for r in after_rows}
    if len(before_rows) != total or len(after_rows) != total or set(before) != ids or set(after) != ids:
        raise ValueError("counter session identities differ")
    if sum(r["visible"] == "true" for r in before.values()) != visible:
        raise ValueError("visible session count differs")
    work = []
    for session in sorted(ids, key=int):
        a, b = before[session], after[session]
        for counter in (a, b):
            if counter["visible"] not in ("true", "false") or any(int(value) < 0 for key, value in counter.items() if key != "visible"):
                raise ValueError("negative or invalid terminal metadata")
        if int(a["history_limit"]) != 2000 or int(a["history"]) != (2000 if saturated else 0):
            raise ValueError("history scenario differs")
        if int(b["history"]) > 2000 or a["visible"] != b["visible"]:
            raise ValueError("history/visibility invariant failed")
        for counter in (a, b):
            if any(int(counter[key]) > 2097152 for key in ("input_outstanding", "input_queued")):
                raise ValueError("input admission accounting exceeded its source budget")
        if int(b["input_cancelled"]) != int(a["input_cancelled"]):
            raise ValueError("accepted input was cancelled during measurement")
        if any(a[key] != b[key] for key in ("columns", "rows")) or not 1 <= int(a["columns"]) <= 512 or int(a["rows"]) < 1:
            raise ValueError("controlled terminal geometry changed")
        fields = ("snapshots", "cells_copied", "font_measurements", "pty_resizes", "grid_resizes", "cache_invalidations", "input_written")
        delta = {key: int(b[key])-int(a[key]) for key in fields}
        if any(value < 0 for value in delta.values()):
            raise ValueError("counter regressed")
        if delta["cells_copied"] > delta["snapshots"]*int(a["columns"])*int(a["rows"]):
            raise ValueError("snapshot copy work exceeded visible viewport bounds")
        if a["visible"] == "false" and any(delta[k] for k in ("snapshots", "cells_copied", "font_measurements", "pty_resizes", "grid_resizes", "cache_invalidations")):
            raise ValueError("hidden session performed rendering work")
        work.append({"session": int(session), "visible": a["visible"] == "true", **delta})
    throughput = select("throughput")
    if flood and (len(throughput) != total or {r["session"] for r in throughput} != ids or any(int(r["a"]) <= 0 or int(r["b"]) <= 0 for r in throughput)):
        raise ValueError("missing parser-observed throughput sentinel")
    latency = read_csv(path / "latency.csv", 200)
    if len(latency) != probes or {r["nonce"] for r in latency} != {str(i) for i in range(1, probes+1)}:
        raise ValueError("missing/duplicate latency observations")
    if any(float(r["readback_ms"]) < float(r["snapshot_ms"]) for r in latency):
        raise ValueError("readback precedes snapshot")
    if any(not math.isfinite(float(r[key])) or float(r[key]) < 0 for r in latency for key in ("snapshot_ms", "readback_ms", "elapsed_ms")):
        raise ValueError("nonfinite latency observation")
    ready_pids = {r["session"]: r["a"] for r in ready}
    if any(r["session"] != min(ids, key=int) or r.get("child_pid") != ready_pids[r["session"]] for r in latency):
        raise ValueError("latency source/session/child identity mismatch")
    if any(float(r["elapsed_ms"]) > interval or float(r["elapsed_ms"]) < 0 for r in latency):
        raise ValueError("latency observation lies outside the measured interval")
    latency_window = max((float(r.get("elapsed_ms", "0")) for r in latency), default=None)
    frames = read_csv(path / "frame-intervals.csv", 2002)
    if len({r["lower_ms"] for r in frames}) != len(frames) or any(not 0 <= int(r["lower_ms"]) <= 2001 or int(r["count"]) < 0 for r in frames):
        raise ValueError("invalid bounded frame histogram")
    frame_count = sum(int(r["count"]) for r in frames)
    def frame_rank(fraction):
        if not frame_count:
            return None
        remaining = math.ceil(frame_count * fraction)
        for row in frames:
            remaining -= int(row["count"])
            if remaining <= 0:
                return {"lower_ms": int(row["lower_ms"]), "upper_ms_exclusive": int(row["lower_ms"])+1 if int(row["lower_ms"]) < 2001 else None}
    geometry = read_csv(path / "geometry.csv", 1)
    if len(geometry) != 1 or any(not math.isfinite(float(geometry[0][key])) or float(geometry[0][key]) <= 0 for key in ("width_dip", "height_dip", "scale")):
        raise ValueError("missing/invalid native geometry")
    if read_csv(path / "geometry-after.csv", 1) != geometry:
        raise ValueError("native window geometry/scale changed during measurement")
    return {"startup_first_child_ms": min(float(r["ms"]) for r in ready),
            "scenario_ready_ms": float(select("scenario_ready")[0]["ms"]),
            "interval_ms": interval,
            "snapshot_latency_ms": distribution(r["snapshot_ms"] for r in latency),
            "readback_latency_ms": distribution(r["readback_ms"] for r in latency),
            "latency_observation_window_ms": latency_window,
            "input_30s_200_sample_gate": probes >= 200 and latency_window is not None and latency_window <= 30000,
            "frame_redraw_intervals_ms": {"count": frame_count, "median_bucket": frame_rank(.5), "p95_bucket": frame_rank(.95)},
            "native_geometry": geometry[0],
            "throughput": [{"session": int(r["session"]), "synthetic_bytes": int(r["a"]),
                            "writer_wall_us": int(r["b"]), "writer_bytes_per_second": int(r["a"])*1e6/int(r["b"]),
                            "parser_sentinel_observed_ms": float(r["ms"])-float(begin[0]["ms"]),
                            "parser_observed_bytes_per_second": int(r["a"])*1000/(float(r["ms"])-float(begin[0]["ms"]))} for r in throughput],
            "work_delta": work}


def write_json(path, data):
    Path(path).write_text(json.dumps(data, indent=2, allow_nan=False) + "\n", encoding="utf-8")
