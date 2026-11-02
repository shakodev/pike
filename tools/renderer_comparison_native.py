"""Strict raw native-collector validation, shared by summaries and comparisons."""
import json
import math
from decimal import Decimal, InvalidOperation
from pathlib import Path

from renderer_comparison_support import digest, distribution, read_csv, validate_fixture

PROCESS_BYTES = 16 * 1024 * 1024
PROCESS_ROWS = 32768
ROLES = ("app", "fixture-child", "conpty-host", "owned-other")


def read_json(path, max_bytes=64 * 1024 * 1024):
    path = Path(path)
    if not path.is_file() or path.stat().st_size > max_bytes:
        raise ValueError("missing/oversized JSON metadata: " + path.name)
    return json.loads(path.read_text(encoding="utf-8-sig"), parse_constant=lambda value: (_ for _ in ()).throw(ValueError("nonfinite JSON number")))


def number(value, name, integer=False, positive=False):
    if integer:
        try:
            exact = Decimal(str(value))
        except InvalidOperation as failure:
            raise ValueError("invalid integer counter: " + name) from failure
        if not exact.is_finite() or exact < 0 or (positive and exact <= 0) or exact != exact.to_integral_value():
            raise ValueError("invalid integer counter: " + name)
        return int(exact)
    result = float(value)
    if isinstance(value, bool) or not math.isfinite(result) or result < 0 or (positive and result <= 0) or (integer and not result.is_integer()):
        raise ValueError("invalid finite counter: " + name)
    return int(result) if integer else result


def validate_native_run(directory):
    try:
        return _validate(Path(directory))
    except (OSError, KeyError, TypeError, IndexError) as failure:
        raise ValueError("incomplete native evidence: " + str(failure)) from failure


def _validate(directory):
    meta = read_json(directory / "run.json")
    manifest = read_json(directory.parent / "run-manifest.json")
    index = int(directory.name.removeprefix("run-"))
    runs = manifest["runs"]
    if manifest["schema"] != 1 or not 0 < len(runs) <= 10000 or not 0 <= index < len(runs):
        raise ValueError("run is outside the immutable manifest")
    canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)
    if canonical(meta) != canonical(runs[index]):
        raise ValueError("complete run/build identity differs from planned manifest row")
    if meta["probes"] and not meta.get("latency_repetition"):
        raise ValueError("readback observation cannot be grouped as passive resource cost")
    fixture = validate_fixture(directory / "fixture", meta["visible"], meta["hidden"], meta["history"] == "saturated", meta["workload"] == "flood", meta["probes"], meta["seconds"])
    collector = read_json(directory / "collector.json", 2 * 1024 * 1024)
    if collector.get("Success") is not True or collector.get("Survivors") != [] or collector.get("AppExitCode") != 0 or collector.get("StderrReaderFinished") is not True:
        raise ValueError("collector did not prove complete owned cleanup")
    app_pid = number(collector["AppPid"], "AppPid", integer=True, positive=True)
    app_created = number(collector["AppCreatedTicks"], "AppCreatedTicks", integer=True, positive=True)
    begin_seconds = number(collector["ExternalBeginObservationMs"], "external begin") / 1000
    ready_ms = number(collector["ExternalSceneSnapshotReadyMs"], "external snapshot-ready startup")
    if ready_ms > begin_seconds*1000:
        raise ValueError("external readiness lies after measurement begin")
    fixture["external_scene_snapshot_ready_ms"] = ready_ms
    fixture["terminal_geometry"] = [
        {"slot": slot, "visible": row["visible"] == "true", "columns": int(row["columns"]), "rows": int(row["rows"])}
        for slot, row in enumerate(sorted(read_csv(directory / "fixture/counters-before.csv", 32), key=lambda row: int(row["session"])))
    ]
    external_seconds = number(collector["ExternalDurationMs"], "external duration", positive=True) / 1000
    ready = {int(r["a"]): r["session"] for r in read_csv(directory / "fixture/events.csv") if r["event"] == "ready"}
    tree = read_csv(directory / "owned-tree.csv", 64)
    identities = {}
    for row in tree:
        pid = number(row["ProcessId"], "owned pid", integer=True, positive=True)
        created = number(row["CreatedTicks"], "owned creation", integer=True, positive=True)
        if pid in identities or row["Role"] not in ROLES:
            raise ValueError("duplicate/invalid owned process identity")
        identities[pid] = (created, row["Role"])
    if identities.get(app_pid) != (app_created, "app") or sum(role == "app" for _, role in identities.values()) != 1:
        raise ValueError("owned app identity differs")
    if {pid for pid, (_, role) in identities.items() if role == "fixture-child"} != set(ready):
        raise ValueError("owned child identities differ from ready sessions")
    # Every sampled PID must be a descendant in the verified ownership snapshot.
    parents = {int(row["ProcessId"]): int(row["ParentProcessId"]) for row in tree}
    for pid in identities:
        current, visited = pid, set()
        while current != app_pid:
            if current in visited or current not in parents:
                raise ValueError("owned process is outside the app tree")
            visited.add(current)
            current = parents[current]
    rows = read_csv(directory / "process.csv", PROCESS_ROWS, max_bytes=PROCESS_BYTES)
    if not rows:
        raise ValueError("missing native process observations")
    by_pid = {}
    samples = {}
    seen = set()
    for row in rows:
        pid = number(row["Pid"], "sample pid", integer=True, positive=True)
        created = number(row["CreatedTicks"], "sample creation", integer=True, positive=True)
        sample = number(row["Sample"], "sample index", integer=True)
        seconds = number(row["Seconds"], "sample seconds")
        if row["Phase"] not in ("measure", "end", "drain") or row["Role"] not in ROLES or identities.get(pid) != (created, row["Role"]):
            raise ValueError("sample role/PID/creation differs from verified ownership")
        if (sample, pid) in seen or seconds > external_seconds or seconds < begin_seconds-.05:
            raise ValueError("duplicate or out-of-window process observation")
        seen.add((sample, pid))
        for key in ("CpuSeconds", "PrivateBytes", "WorkingSetBytes", "Threads", "Handles"):
            row[key] = number(row[key], key, integer=key != "CpuSeconds", positive=row["Phase"] != "drain" and key in ("PrivateBytes", "WorkingSetBytes", "Threads"))
        row.update(Pid=pid, CreatedTicks=created, Sample=sample, Seconds=seconds)
        previous = by_pid.setdefault(pid, [])
        if previous and (seconds <= previous[-1]["Seconds"] or sample <= previous[-1]["Sample"] or row["CpuSeconds"] < previous[-1]["CpuSeconds"]):
            raise ValueError("process time/CPU counters are not monotonic")
        previous.append(row)
        if row["Phase"] in ("measure", "end"):
            samples.setdefault(sample, set()).add(pid)
    required = {pid for pid, (_, role) in identities.items() if role in ("app", "fixture-child", "conpty-host")}
    if not samples or any(not required <= pids for pids in samples.values()):
        raise ValueError("required owned process cost observations are missing")
    startup = bool(meta.get("startup_repetition"))
    for pid in required:
        values = [row for row in by_pid.get(pid, []) if row["Phase"] in ("measure", "end")]
        minimum = meta["seconds"]*.98
        if meta.get("dry_run"):
            minimum = max(0, meta["seconds"]-1.25)
        elif meta["workload"] == "idle" and not startup:
            minimum = max(minimum, 60)
        if not values or (not startup and (len(values) < 2 or values[-1]["Seconds"]-values[0]["Seconds"] < minimum)):
            raise ValueError("insufficient measurement-window process coverage")
        if not meta.get("dry_run") and not startup and any(b["Seconds"]-a["Seconds"] > 1.5 for a, b in zip(values, values[1:])):
            raise ValueError("one-second native sampling has a material gap")
        if values[-1]["Seconds"] > begin_seconds + fixture["interval_ms"]/1000 + .25:
            raise ValueError("cost sample lies after the measured interval")
    measurements = {}
    for role in ROLES:
        selected = [row for row in rows if row["Role"] == role and row["Phase"] in ("measure", "end")]
        cpu = []
        coverage = []
        for pid, values in by_pid.items():
            values = [row for row in values if row["Role"] == role and row["Phase"] in ("measure", "end")]
            if len(values) > 1:
                wall = values[-1]["Seconds"]-values[0]["Seconds"]
                coverage.append(wall)
                cpu.append(100*(values[-1]["CpuSeconds"]-values[0]["CpuSeconds"])/wall)
        sums = {}
        for row in selected:
            sums[row["Sample"]] = sums.get(row["Sample"], 0)+row["PrivateBytes"]
        measurements[role] = {"identities": len({r["Pid"] for r in selected}), "one_core_cpu_percent_sum": sum(cpu) if cpu else None,
                              "cpu_sample_window_seconds": distribution(coverage),
                              "per_process_private_bytes": distribution(r["PrivateBytes"] for r in selected),
                              "per_process_working_set_bytes": distribution(r["WorkingSetBytes"] for r in selected),
                              "threads": distribution(r["Threads"] for r in selected), "handles": distribution(r["Handles"] for r in selected),
                              "sum_private_bytes_by_sample": distribution(sums.values())}
    raw = [directory / name for name in ("run.json", "collector.json", "owned-tree.csv", "process.csv")]
    raw += [directory / "fixture" / name for name in ("events.csv", "counters-before.csv", "counters-after.csv", "latency.csv", "geometry.csv", "geometry-after.csv", "frame-intervals.csv")]
    hashes = {str(path.relative_to(directory)): digest(path) for path in raw}
    hashes["manifest.json"] = digest(directory.parent / "run-manifest.json")
    return {"meta": meta, "fixture": fixture, "processes": measurements, "raw_sha256": hashes}
