#!/usr/bin/env python3
"""Build isolated release comparisons, produce finite run manifests, validate evidence.

Python 3.11+, standard library only. Run from the repository root.
"""
import argparse
import csv
import json
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import zipfile

from renderer_comparison_support import digest, distribution, feature_guard, read_csv, validate_fixture, write_json
from renderer_comparison_dry_run import check as dry_run
from renderer_comparison_native import validate_native_run, read_json

VARIANTS = {
    "baseline": (3, False, 16, "none"),
    "strip": (3, False, 16, "symbols"),
    "codegen1": (3, False, 1, "symbols"),
    "thin": (3, "thin", 1, "symbols"),
    "fat": (3, "fat", 1, "symbols"),
    "size-s": ("s", "thin", 1, "symbols"),
    "size-z": ("z", "thin", 1, "symbols"),
}


def build_identity(build):
    immutable = {key: value for key, value in build.items() if key not in ("directory", "executable", "invocation")}
    invocation = []
    relocate = False
    for argument in build.get("invocation", []):
        invocation.append("<relocated-path>" if relocate else argument)
        relocate = argument in ("--target-dir", "--manifest-path")
    immutable["invocation"] = invocation
    return hashlib.sha256(json.dumps(immutable, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def command(args, **kw):
    return subprocess.check_output(args, text=True, **kw).strip()


def build(args):
    output = args.output.resolve()
    output.mkdir()  # Never overwrite previous reds/identities.
    env = os.environ.copy()
    forbidden = [key for key in env if key.startswith("CARGO_PROFILE_RELEASE_") or key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS") or (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS"))]
    if forbidden:
        raise ValueError("remove ambient build overrides before comparison: " + ",".join(forbidden))
    env["CARGO_BUILD_JOBS"] = "4"
    sha = command(["git", "rev-parse", "HEAD"])
    source = sorted(set(command(["git", "ls-files", "*.rs", "*Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]).splitlines()) |
                    {str(p) for p in Path("examples/renderer_comparison").glob("*.rs")} | {"examples/renderer_comparison.rs"})
    identities = {name: digest(name) for name in source}
    write_json(output / "source.json", {"commit": sha, "tracked_dirty": bool(command(["git", "diff", "--stat"])), "files_sha256": identities})
    (output / "rustc.txt").write_text(command(["rustc", "-Vv"]) + "\n")
    builds = []
    for renderer in args.renderers:
        base = ["--locked", "--target", args.target, "--no-default-features", "--features", "renderer-" + renderer]
        graph = command(["cargo", "tree", *base, "--color", "never", "--charset", "ascii", "-e", "normal,build", "--prefix", "none", "--format", "{p}|{f}"])
        feature_guard(graph, renderer)
        (output / (renderer + "-resolved.txt")).write_text(graph.replace(str(Path.cwd()), "<repository>") + "\n")
        fixture_graph = command(["cargo", "tree", *base, "--color", "never", "--charset", "ascii", "-e", "normal,build,dev", "--prefix", "none", "--format", "{p}|{f}"])
        feature_guard(fixture_graph, renderer)
        (output / (renderer + "-fixture-resolved.txt")).write_text(fixture_graph.replace(str(Path.cwd()), "<repository>") + "\n")
        (output / (renderer + "-features.txt")).write_text(command(["cargo", "tree", *base, "--color", "never", "--charset", "ascii", "-e", "features"]).replace(str(Path.cwd()), "<repository>") + "\n")
        for name in args.variants:
            opt, lto, units, strip = VARIANTS[name]
            directory = output / (renderer + "-" + name)
            directory.mkdir()
            config = {"opt-level": opt, "lto": lto, "codegen-units": units, "strip": strip, "debug": False, "incremental": False}
            invocation = ["cargo", "build", *base, "--release", "-p", "pike", "--bin", "pike", "--example", "renderer_comparison", "--target-dir", str(args.target_directory)]
            for key, value in config.items():
                invocation.extend(["--config", "profile.release." + key + "=" + json.dumps(value)])
            with (directory / "build.log").open("w") as log:
                subprocess.run(invocation, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            if any(not Path(path).exists() or digest(path) != sha256 for path, sha256 in identities.items()):
                raise ValueError("source changed during comparison build; raw build retained without a false identity")
            extension = ".exe" if "windows" in args.target else ""
            built = args.target_directory / args.target / "release"
            for origin, destination in [(built / ("pike" + extension), directory / ("pike" + extension)),
                                        (built / "examples" / ("renderer_comparison" + extension), directory / ("renderer_comparison" + extension))]:
                shutil.copy2(origin, destination)
            archive = directory / "pike-executable-only.zip"
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as package:
                package.write(directory / ("pike" + extension), "pike" + extension)
            row = {"renderer": renderer, "variant": name, "profile": config, "source_commit": sha,
                   "target": args.target, "invocation": invocation, "directory": str(directory),
                   "executable": str(directory / ("renderer_comparison" + extension)),
                   "app_bytes": (directory / ("pike" + extension)).stat().st_size,
                   "app_sha256": digest(directory / ("pike" + extension)),
                   "fixture_sha256": digest(directory / ("renderer_comparison" + extension)),
                   "executable_archive_bytes": archive.stat().st_size, "archive_sha256": digest(archive),
                   "archive_scope": "executable only; portable ConPTY archive is separately required"}
            if platform.system() == "Windows":
                powershell = shutil.which("pwsh") or shutil.which("powershell")
                if not powershell:
                    raise ValueError("PowerShell required for existing ConPTY package verification")
                package_dir = directory / "portable"
                with (directory / "package.log").open("w") as log:
                    subprocess.run([powershell, "-NoProfile", "-File", "tools/package-windows.ps1", "-Executable", str(directory / "pike.exe"),
                                    "-OutputDirectory", str(package_dir), "-TargetDirectory", str(args.target_directory.resolve())], stdout=log, stderr=subprocess.STDOUT, check=True)
                    subprocess.run([powershell, "-NoProfile", "-File", "tools/prepare-conpty.ps1", "-TargetDirectory", str(args.target_directory.resolve()),
                                    "-DestinationDirectories", str(directory)], stdout=log, stderr=subprocess.STDOUT, check=True)
                portable_zip = Path(str(package_dir) + ".zip")
                row.update(portable_archive_bytes=portable_zip.stat().st_size, portable_archive_sha256=digest(portable_zip),
                           conpty_files={entry["name"]: digest(directory / entry["name"]) for entry in json.loads(Path("tools/conpty-runtime.json").read_text())["files"]})
            write_json(directory / "identity.json", row)
            builds.append(row)
            write_json(output / "builds.json", builds)


def plan(args):
    builds = json.loads(args.builds.read_text())
    latency = getattr(args, "latency", False)
    if latency and args.startup:
        raise ValueError("startup and instrumented latency are separate manifests")
    scenarios = []
    # Fixed sequence is counterbalanced by reversing build order every repeat.
    for repeat in range(20 if args.startup else args.repeats):
        for build in builds if repeat % 2 == 0 else reversed(builds):
            for visible in ((1,) if args.startup else ((4,) if latency else (1, 4, 8))):
                for hidden in ((0,) if args.startup or latency else (0, 8)):
                    for history in (("empty",) if args.startup else (("saturated",) if latency else ("empty", "saturated"))):
                        for workload in (("idle",) if args.startup else (("flood",) if latency else ("idle", "flood"))):
                            scenarios.append({"build": build, "visible": visible, "hidden": hidden,
                                              "history": history, "workload": workload, "repeat": repeat,
                                              "seconds": 1 if args.startup else (3 if args.dry_run else (30 if latency else (62 if workload == "idle" else 600))),
                                              "settle": 0 if args.dry_run or args.startup else 10,
                                              "probes": (2 if args.dry_run else 200) if latency else 0,
                                              "dry_run": args.dry_run, "startup_repetition": args.startup, "latency_repetition": latency})
    write_json(args.output, {"schema": 1, "geometry_dip": [1600, 1000], "font": "monospace 14 DIP (Consolas on Windows); record installed font hash separately",
                             "history_limit": 2000, "producer_bytes_per_second_per_session": 2097152,
                             "required_host": "existing native Windows 11 24H2+ x64, interactive desktop, no debugger",
                             "selection_status": "PENDING: independent correctness/text-quality and native cost review required",
                             "runs": scenarios})


def summarize(args):
    validated = validate_native_run(args.directory)
    write_json(args.directory / "summary.json", {"fixture": validated["fixture"], "processes": validated["processes"], "raw_sha256": validated["raw_sha256"],
        "limitations": ["synthetic commands, snapshot and screenshot callback latency; physical input/display latency unmeasured",
                        "native redraw-request intervals are not GPU execution or presentation timestamps",
                        "separate GPU counters/metadata and correctness review are required before selection"], "selection": "PENDING"})


def compare(args):
    """Compare valid runs; never turn incomplete evidence into a default choice."""
    groups = {}
    blockers = {"independent correctness/text-quality and native decision review required",
                "physical input/display and actual GPU presentation timing are separate gates"}
    sources = set()
    expected_runs = set()
    observed_runs = set()
    failed_runs = []
    scenario_geometry = {}
    for root in args.roots:
        machine_path = root / "machine.json"
        machine = json.loads(machine_path.read_text(encoding="utf-8-sig")) if machine_path.exists() else {}
        if int(machine.get("OS", {}).get("BuildNumber", 0)) < 26100:
            blockers.add("native Windows 11 24H2+ machine metadata missing")
        manifest_path = root / "run-manifest.json"
        manifest_id = digest(manifest_path) if manifest_path.exists() else "missing"
        if manifest_path.exists():
            manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
            if not 0 < len(manifest["runs"]) <= 10000:
                raise ValueError("invalid comparison manifest coverage")
            expected_runs.update((manifest_id, index) for index in range(len(manifest["runs"])))
        else:
            blockers.add("run manifest and complete scenario coverage unavailable")
        for directory in root.glob("run-*"):
            if directory.is_dir() and not (directory / "summary.json").exists():
                failed_runs.append(str(directory))
        summaries = sorted(root.glob("run-*/summary.json"))
        if not summaries:
            raise ValueError("no validated run summaries in " + str(root))
        if len(summaries) > 10000:
            raise ValueError("comparison run bound exceeded")
        for path in summaries:
            directory = path.parent
            observed = (manifest_id, int(directory.name.split("-")[-1]))
            if observed in observed_runs:
                raise ValueError("duplicate planned run is not an independent repetition")
            observed_runs.add(observed)
            validated = validate_native_run(directory)
            meta, fixture = validated["meta"], validated["fixture"]
            stored = read_json(path)
            if stored.get("raw_sha256") != validated["raw_sha256"]:
                raise ValueError("summary raw identities are stale")
            summary = {"processes": validated["processes"]}
            build = meta["build"]
            sources.add(build["source_commit"])
            if meta.get("dry_run"):
                blockers.add("dry runs are present; they cannot select a product default")
            if build["target"] != "x86_64-pc-windows-msvc":
                blockers.add("compared build target is not native Windows x64")
            if not build.get("portable_archive_bytes"):
                blockers.add("portable ConPTY archive evidence missing")
            if not (directory / "gpu-availability.json").exists():
                blockers.add("GPU availability evidence missing")
            readiness = directory / "fixture" / "readiness.txt"
            if not readiness.exists() or readiness.read_text().strip() != "App::boot_with_config_path":
                blockers.add("integrated actual34 product boot/readiness evidence pending")
            if meta["workload"] == "flood" and meta["probes"] >= 200 and not fixture["input_30s_200_sample_gate"]:
                blockers.add("200 validated input observations did not fit the 30-second window")
            key = (build["renderer"], build["variant"], meta["visible"], meta["hidden"], meta["history"], meta["workload"], bool(meta.get("startup_repetition")), bool(meta.get("latency_repetition")), build_identity(build))
            shape = json.dumps(fixture["terminal_geometry"], sort_keys=True)
            scenario = key[2:8]
            if scenario in scenario_geometry and scenario_geometry[scenario] != shape:
                raise ValueError("terminal cell geometry differs within the controlled scenario")
            scenario_geometry[scenario] = shape
            group = groups.setdefault(key, {"runs": [], "geometry": set(), "machine": set(), "build": build})
            group["geometry"].add(json.dumps(fixture["native_geometry"], sort_keys=True))
            group["machine"].add(digest(machine_path) if machine_path.exists() else "missing")
            group["runs"].append({"path": str(directory), "meta": meta, "fixture": fixture, "summary": summary})
    if len(sources) != 1:
        blockers.add("compared source commits differ")
    if failed_runs:
        blockers.add("failed or unvalidated runs remain in supplied evidence")
    if expected_runs-observed_runs:
        blockers.add("planned scenarios or repetitions are missing")
    builds_seen = {(key[0], key[1], key[8]) for key in groups}
    resource_cases = {(visible, hidden, history, workload) for visible in (1, 4, 8) for hidden in (0, 8) for history in ("empty", "saturated") for workload in ("idle", "flood")}
    for build_id in builds_seen:
        relevant = [(key, group) for key, group in groups.items() if (key[0], key[1], key[8]) == build_id]
        covered = {key[2:6] for key, _ in relevant if not key[6] and not key[7]}
        if covered != resource_cases:
            blockers.add("complete 1/4/8 visible and hidden resource matrix missing")
        if sum(len(group["runs"]) for key, group in relevant if key[6]) < 20:
            blockers.add("20 warm startup repetitions per build missing")
        if not any(key[7] and any(run["fixture"]["input_30s_200_sample_gate"] for run in group["runs"]) for key, group in relevant):
            blockers.add("dedicated 30-second/200-probe rendered latency evidence missing")
    rows = []
    for key, group in groups.items():
        runs = group["runs"]
        if len(group["geometry"]) != 1 or len(group["machine"]) != 1:
            blockers.add("machine or geometry changed within a scenario")
        def observed(getter):
            return distribution(value for run in runs if (value := getter(run)) is not None)
        row = dict(zip(("renderer", "variant", "visible", "hidden", "history", "workload", "startup", "latency", "build_identity"), key))
        row.update(run_count=len(runs), app_bytes=group["build"]["app_bytes"], executable_archive_bytes=group["build"]["executable_archive_bytes"],
                   portable_archive_bytes=group["build"].get("portable_archive_bytes"),
                   startup_ready_ms=observed(lambda r: r["fixture"]["external_scene_snapshot_ready_ms"]),
                   snapshot_run_p95_ms=observed(lambda r: r["fixture"]["snapshot_latency_ms"]["p95"]),
                   readback_run_p95_ms=observed(lambda r: r["fixture"]["readback_latency_ms"]["p95"]),
                   raw_paths=[r["path"] for r in runs])
        row["parser_observed_bytes_per_second_per_session"] = distribution(t["parser_observed_bytes_per_second"] for run in runs for t in run["fixture"]["throughput"])
        row["writer_bytes_per_second_per_session"] = distribution(t["writer_bytes_per_second"] for run in runs for t in run["fixture"]["throughput"])
        row["frame_redraw_p95_lower_bucket_ms"] = observed(lambda r: (r["fixture"]["frame_redraw_intervals_ms"]["p95_bucket"] or {}).get("lower_ms"))
        row["validated_input_samples"] = sum(run["fixture"]["snapshot_latency_ms"]["count"] for run in runs)
        for role in ("app", "fixture-child", "conpty-host", "owned-other"):
            row[role] = {
                "one_core_cpu_percent": observed(lambda r: r["summary"]["processes"][role]["one_core_cpu_percent_sum"]),
                "private_bytes_run_median": observed(lambda r: r["summary"]["processes"][role]["sum_private_bytes_by_sample"]["median"]),
                "working_set_per_process_run_median": observed(lambda r: r["summary"]["processes"][role]["per_process_working_set_bytes"]["median"]),
                "threads_per_process_run_maximum": observed(lambda r: r["summary"]["processes"][role]["threads"]["maximum"]),
                "handles_per_process_run_maximum": observed(lambda r: r["summary"]["processes"][role]["handles"]["maximum"]),
            }
        rows.append(row)
    # Cross-renderer geometry/machine comparison is mandatory as well.
    if len({g for group in groups.values() for g in group["geometry"]}) != 1 or len({m for group in groups.values() for m in group["machine"]}) != 1:
        blockers.add("the compared profiles do not share one machine and native geometry")
    write_json(args.output, {"schema": 1, "source_commits": sorted(sources), "selection": "PENDING", "blockers": sorted(blockers),
                             "failed_or_unvalidated_runs": failed_runs, "missing_planned_run_count": len(expected_runs-observed_runs),
                             "method": "nearest-rank distributions of run medians/p95s/maxima; raw observations retained separately", "groups": rows})
    csv_path = args.output.with_suffix(".csv")
    with csv_path.open("w", newline="", encoding="utf-8") as stream:
        fields = ["renderer", "variant", "build_identity", "visible", "hidden", "history", "workload", "startup", "latency", "run_count", "app_bytes", "executable_archive_bytes", "portable_archive_bytes", "app_cpu_run_median", "app_private_run_median", "snapshot_run_p95_median", "readback_run_p95_median", "validated_input_samples", "parser_observed_bytes_per_second_per_session"]
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        for row in rows:
            writer.writerow({**{key: row[key] for key in fields if key in row},
                "app_cpu_run_median": row["app"]["one_core_cpu_percent"]["median"], "app_private_run_median": row["app"]["private_bytes_run_median"]["median"],
                "snapshot_run_p95_median": row["snapshot_run_p95_ms"]["median"], "readback_run_p95_median": row["readback_run_p95_ms"]["median"],
                "parser_observed_bytes_per_second_per_session": row["parser_observed_bytes_per_second_per_session"]["median"]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    p = commands.add_parser("build")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--target-directory", type=Path, default=Path("target"))
    p.add_argument("--target", default=command(["rustc", "-vV"]).split("host: ")[1].splitlines()[0])
    p.add_argument("--variants", nargs="+", choices=VARIANTS, default=list(VARIANTS))
    p.add_argument("--renderers", nargs="+", choices=("tiny-skia", "wgpu"), default=["tiny-skia", "wgpu"])
    p.set_defaults(func=build)
    p = commands.add_parser("plan")
    p.add_argument("--builds", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--repeats", type=int, choices=range(1, 6), default=3)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--startup", action="store_true", help="20 warm single-session launches per build")
    p.add_argument("--latency", action="store_true", help="separate 30-second four-producer pixel-readback observations")
    p.set_defaults(func=plan)
    p = commands.add_parser("summarize")
    p.add_argument("directory", type=Path)
    p.set_defaults(func=summarize)
    p = commands.add_parser("compare")
    p.add_argument("roots", nargs="+", type=Path)
    p.add_argument("--output", type=Path, required=True)
    p.set_defaults(func=compare)
    p = commands.add_parser("dry-run")
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--first-run", type=int, choices=range(10000), default=0)
    p.add_argument("--run-count", type=int, choices=range(1, 10001), default=10000)
    p.add_argument("--keep-going", action="store_true", help="retain every finite diagnostic case; still fail the final outcome if any case fails")
    p.set_defaults(func=dry_run)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
