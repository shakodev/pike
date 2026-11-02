"""Finite Unix/native-graphics workflow checks, explicitly not Windows cost data."""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

from renderer_comparison_support import digest, read_csv, validate_fixture, write_json


def check(args):
    if sys.platform == "win32":
        raise ValueError("use the native PowerShell collector on Windows")
    manifest = json.loads(args.manifest.read_text(encoding="utf-8-sig"))
    if not 0 < len(manifest["runs"]) <= 10000:
        raise ValueError("invalid bounded manifest")
    output = args.output.resolve()
    output.mkdir()
    write_json(output / "scope.json", {"platform": sys.platform, "scope": "actual App/native graphics/Unix PTY, synthetic App focus, short workflow validation only",
                                      "private_bytes": None, "working_set": None, "one_core_cpu": None, "gpu": None,
                                      "missing_counters": "native Windows collector and physical input/display checks unexecuted"})
    outcomes = []
    for index, run in enumerate(manifest["runs"][args.first_run:args.first_run+args.run_count], args.first_run):
        if not run.get("dry_run") or run["seconds"] > 5 or run["settle"] > 2:
            raise ValueError("local dry runs require a short explicitly marked manifest")
        directory = output / f"run-{index:04d}"
        directory.mkdir()
        write_json(directory / "run.json", run)
        build = run["build"]
        exe = Path(build["executable"]).resolve()
        if digest(exe) != build["fixture_sha256"]:
            raise ValueError("binary identity changed")
        command = [str(exe), "--output", str(directory / "fixture"), "--visible", str(run["visible"]), "--hidden", str(run["hidden"]),
                   "--history", run["history"], "--workload", run["workload"], "--seconds", str(run["seconds"]),
                   "--settle", str(run["settle"]), "--probes", str(run["probes"]), "--synthetic-focus", "true"]
        env = os.environ.copy()
        env.update(RUST_LOG="off", ICED_BACKEND=build["renderer"])
        process = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)
        prefix = bytearray()
        def drain():
            for data in iter(lambda: process.stderr.read(4096), b""):
                prefix.extend(data[:max(0, 4096-len(prefix))])
        reader = threading.Thread(target=drain, daemon=True)
        reader.start()
        start = time.monotonic()
        error = None
        ready_pids = set()
        success = False
        try:
            while process.poll() is None and time.monotonic()-start < run["seconds"]+run["settle"]+40:
                events = directory / "fixture" / "events.csv"
                if events.exists():
                    ready_pids.update(int(row["a"]) for row in read_csv(events) if row["event"] == "ready")
                time.sleep(.05)  # Finite external watcher, never product polling.
            if process.poll() is None:
                raise ValueError("external dry-run deadline")
            if process.returncode != 0:
                raise ValueError(f"fixture exited {process.returncode}")
            summary = validate_fixture(directory / "fixture", run["visible"], run["hidden"], run["history"] == "saturated", run["workload"] == "flood", run["probes"], run["seconds"])
            write_json(directory / "fixture-summary.json", summary)
            success = True
        except Exception as failure:
            error = str(failure)
        finally:
            # Popen retains the exact child identity until reaped; no name/PID
            # search is used for termination. Closing the owned PTY masters
            # also ends controlled Unix children; verify their absence below.
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            reader.join(timeout=5)
            process.stderr.close()
            (directory / "stderr.txt").write_bytes(prefix)
            survivors = []
            if ready_pids:
                observed = subprocess.run(["/bin/ps", "-p", ",".join(map(str, sorted(ready_pids))), "-o", "pid=,ppid=,lstart=,stat=,comm="], capture_output=True, text=True)
                survivors = observed.stdout.splitlines()
            if survivors or reader.is_alive():
                success = False
                error = error or "owned child absence or bounded reader completion not established"
            write_json(directory / "collector.json", {"success": success, "failure": error, "app_pid": process.pid, "ready_child_pids": sorted(ready_pids),
                         "child_processes_observed_after_app_exit": survivors, "stderr_reader_finished": not reader.is_alive(),
                         "exit_code": process.returncode, "external_wall_seconds": time.monotonic()-start, "scope": "local synthetic workflow check only"})
        outcomes.append({"index": index, "success": success, "failure": error})
        write_json(output / "outcomes.json", outcomes)
        if not success and not getattr(args, "keep_going", False):
            raise ValueError(f"dry run {index} failed: {error}; raw evidence retained")
    if any(not row["success"] for row in outcomes):
        raise ValueError("dry-run matrix contains invalid runs; all raw results and cleanup outcomes retained")
