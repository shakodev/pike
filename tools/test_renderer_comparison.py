"""Meaningful evidence/feature-isolation regressions; no GPU or Windows required."""
import csv
import importlib.util
import json
import shutil
from pathlib import Path
import tempfile
import unittest

from renderer_comparison_support import distribution, feature_guard, read_csv, validate_fixture


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.events = [
            [0, "ready", 1, 100, 0], [1, "ready", 2, 101, 0],
            [2, "scenario_ready", 0, 1, 1], [10, "measure_begin", 0, 1, 1],
            [6010, "measure_end", 0, 6000, 0],
            [6011, "completion", 1, "true", 0], [6012, "completion", 2, "true", 0],
            [6013, "finished", 0, 2, 0],
        ]
        keys = "session,visible,columns,rows,history,history_limit,snapshots,cells_copied,font_measurements,pty_resizes,grid_resizes,cache_invalidations,input_written,input_outstanding,input_queued,input_cancelled".split(",")
        self.before = [dict(zip(keys, [i, visible, 120, 24, 2000, 2000, 4, 11520, 1, 1, 1, 1, 10, 0, 0, 0])) for i, visible in [(1, "true"), (2, "false")]]
        self.after = [row.copy() for row in self.before]
        self.save()

    def save(self):
        with (self.root / "events.csv").open("w", newline="") as stream:
            csv.writer(stream).writerows([["ms", "event", "session", "a", "b"], *self.events])
        for name, rows in [("before", self.before), ("after", self.after)]:
            with (self.root / f"counters-{name}.csv").open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=rows[0].keys())
                writer.writeheader()
                writer.writerows(rows)
        (self.root / "latency.csv").write_text("nonce,session,snapshot_ms,readback_ms,width,height,scale\n")
        (self.root / "completed.txt").write_text("passed\n")
        (self.root / "frame-intervals.csv").write_text("lower_ms,count\n16,10\n2001,1\n")
        (self.root / "geometry.csv").write_text("width_dip,height_dip,scale\n1600,1000,1\n")
        (self.root / "geometry-after.csv").write_text("width_dip,height_dip,scale\n1600,1000,1\n")

    def verify(self, flood=False, probes=0):
        return validate_fixture(self.root, 1, 1, True, flood, probes)

    def test_real_invariants_accept_complete_bounded_metadata(self):
        self.assertEqual(self.verify()["interval_ms"], 6000)

    def test_missing_drained_completion_or_reused_child_identity_fails(self):
        for index, column, value in [(6, 3, "false"), (1, 3, 100), (1, 2, 1)]:
            saved = self.events[index][column]
            self.events[index][column] = value
            self.save()
            with self.assertRaises(ValueError): self.verify()
            self.events[index][column] = saved

    def test_hidden_snapshots_and_history_overflow_fail(self):
        for key, value in [("snapshots", 5), ("history", 2001), ("font_measurements", 2), ("pty_resizes", 2), ("visible", "true")]:
            saved = self.after[1][key]
            self.after[1][key] = value
            self.save()
            with self.assertRaises(ValueError): self.verify()
            self.after[1][key] = saved

    def test_visible_snapshot_copy_is_bounded_by_unchanged_geometry(self):
        self.after[0]["snapshots"] += 1
        self.after[0]["cells_copied"] += 120*24+1
        self.save()
        with self.assertRaises(ValueError): self.verify()

    def test_latency_requires_the_exact_child_and_probe_identity(self):
        self.events.append([6012, "measurement_epoch", 0, 10000, 16000])
        self.save()
        header = "nonce,session,snapshot_ms,readback_ms,width,height,scale,elapsed_ms,child_pid\n"
        for nonce, pid in [(2, 100), (1, 101)]:
            (self.root / "latency.csv").write_text(header+f"{nonce},1,1,2,1600,1000,1,3,{pid}\n")
            with self.assertRaises(ValueError): self.verify(probes=1)
        (self.root / "latency.csv").write_text(header+"1,1,1,2,1600,1000,1,3,100\n")
        self.assertEqual(self.verify(probes=1)["readback_latency_ms"]["count"], 1)

    def test_missing_throughput_and_latency_cannot_be_reported_zero(self):
        with self.assertRaises(ValueError): self.verify(flood=True)
        with self.assertRaises(ValueError): self.verify(probes=1)

    def test_full_sample_count_does_not_validate_a_materially_late_interval(self):
        self.events.append([6012, "measurement_epoch", 0, 10000, 16000])
        self.save()
        validate_fixture(self.root, 1, 1, True, False, 0, 6)
        with self.assertRaises(ValueError): validate_fixture(self.root, 1, 1, True, False, 0, 3)

    def test_wrong_producer_overlap_cannot_qualify_throughput(self):
        self.events.extend([[6012, "measurement_epoch", 0, 10000, 16000],
                            [6012, "producer_epoch", 1, 1000, 7000], [6012, "producer_epoch", 2, 1000, 7000],
                            [6012, "throughput", 1, 12000000, 6000000], [6012, "throughput", 2, 12000000, 6000000]])
        self.save()
        with self.assertRaises(ValueError): validate_fixture(self.root, 1, 1, True, True, 0, 6)

    def test_bounded_metadata_rejects_large_file(self):
        path = self.root / "oversized.csv"
        path.write_bytes(b"x" * (2 * 1024 * 1024 + 1))
        with self.assertRaises(ValueError): read_csv(path)

    def test_percentiles_are_nearest_rank_and_missing_is_null(self):
        self.assertIsNone(distribution([])["median"])
        self.assertEqual(distribution(range(1, 201))["p95"], 190)
        with self.assertRaises(ValueError): distribution([float("nan")])


class FeatureTests(unittest.TestCase):
    GRAPH = "iced v0.14.0|crisp,tiny-skia,tokio\niced_futures v0.14.0|tokio\niced_renderer v0.14.0|tiny-skia\niced_term v0.8.0|\ntokio v1.54.1|rt,rt-multi-thread,sync,time\n"

    def test_repeated_versions_and_renderer_runtime_leaks_fail(self):
        feature_guard(self.GRAPH, "tiny-skia")
        for suffix in ["wgpu v27.0.1|dx12", "iced v0.14.0|default", "tokio v1.54.1|full", "iced_futures v0.14.0|thread-pool", "iced_term v0.8.0|compatibility-defaults"]:
            with self.assertRaises(ValueError): feature_guard(self.GRAPH + suffix, "tiny-skia")


class ManifestTests(unittest.TestCase):
    def test_matrix_is_finite_and_counterbalances_build_order(self):
        spec = importlib.util.spec_from_file_location("comparison_cli", Path(__file__).with_name("renderer-comparison.py"))
        cli = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cli)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            builds = root / "builds.json"
            builds.write_text(json.dumps([{"renderer": "tiny-skia"}, {"renderer": "wgpu"}]))
            class Args:
                repeats = 2
                dry_run = False
                startup = False
            args = Args()
            args.builds, args.output = builds, root / "plan.json"
            cli.plan(args)
            runs = json.loads(args.output.read_text())["runs"]
            self.assertEqual(len(runs), 96)
            self.assertEqual(runs[0]["build"]["renderer"], "tiny-skia")
            self.assertEqual(runs[48]["build"]["renderer"], "wgpu")
            self.assertEqual({(r["visible"], r["hidden"]) for r in runs}, {(v, h) for v in (1, 4, 8) for h in (0, 8)})
            self.assertTrue(all(r["seconds"] <= 600 and r["probes"] <= 200 for r in runs))
            self.assertTrue(all(r["probes"] == 0 for r in runs), "resource counters must exclude readback observers")
            args.latency = True
            args.output = root / "latency-plan.json"
            cli.plan(args)
            latency = json.loads(args.output.read_text())["runs"]
            self.assertEqual(len(latency), 4)
            self.assertTrue(all(r["visible"] == 4 and r["hidden"] == 0 and r["seconds"] == 30 and r["probes"] == 200 for r in latency))


class CollectorBoundaryTests(unittest.TestCase):
    setUp = EvidenceTests.setUp
    save = EvidenceTests.save
    def test_supported_full_process_table_has_an_explicit_finite_byte_bound(self):
        from renderer_comparison_native import PROCESS_BYTES, PROCESS_ROWS, number
        path = self.root / "full-synthetic-process-table.csv"
        with path.open("w", newline="") as stream:
            writer = csv.writer(stream, quoting=csv.QUOTE_ALL)
            writer.writerow("Sample,Seconds,Phase,Role,Pid,CreatedTicks,CpuSeconds,PrivateBytes,WorkingSetBytes,Threads,Handles".split(","))
            for index in range(19833):
                writer.writerow([index, 600.123456789, "measure", "fixture-child", 12345, 638000000000000001, 123.456789, 1000000000, 1000000000, 10, 1000])
        self.assertGreater(path.stat().st_size, 2*1024*1024)
        self.assertEqual(len(read_csv(path, PROCESS_ROWS, max_bytes=PROCESS_BYTES)), 19833)
        with self.assertRaises(ValueError): read_csv(path, PROCESS_ROWS, max_bytes=1024)
        self.assertNotEqual(number('638000000000000001', 'creation', integer=True), number('638000000000000002', 'creation', integer=True))
    def test_powershell_bom_and_owned_cost_aggregation_are_preserved(self):
        spec = importlib.util.spec_from_file_location("comparison_cli", Path(__file__).with_name("renderer-comparison.py"))
        cli = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cli)
        self.events.append([6012, "measurement_epoch", 0, 10000, 16000])
        self.save()
        evidence = self.root / "native" / "run-0000"
        evidence.mkdir(parents=True)
        shutil.copytree(self.root, evidence / "fixture", ignore=shutil.ignore_patterns("native"))
        # Invented unit-test observations only, explicitly kept in a temporary
        # test directory; never mixed with native diagnostics or evidence.
        meta = {"visible": 1, "hidden": 1, "history": "saturated", "workload": "idle", "probes": 0, "seconds": 6, "dry_run": True,
                "build": {"renderer": "tiny-skia", "variant": "baseline", "source_commit": "synthetic-unit-test", "target": "synthetic-unit-test",
                          "app_bytes": 100, "executable_archive_bytes": 50}}
        (evidence / "run.json").write_text(json.dumps(meta), encoding="utf-8-sig")
        (evidence.parent / "run-manifest.json").write_text(json.dumps({"schema": 1, "runs": [meta]}), encoding="utf-8-sig")
        (evidence / "collector.json").write_text(json.dumps({"Success": True, "Survivors": [], "AppExitCode": 0, "StderrReaderFinished": True,
            "AppPid": 1, "AppCreatedTicks": 100, "ExternalBeginObservationMs": 0, "ExternalDurationMs": 7000, "ExternalSceneSnapshotReadyMs": 0}), encoding="utf-8-sig")
        (evidence / "owned-tree.csv").write_text("ProcessId,ParentProcessId,Name,CreatedTicks,Role\n1,0,fixture,100,app\n100,1,fixture,101,fixture-child\n101,1,fixture,102,fixture-child\n")
        with (evidence / "process.csv").open("w", newline="") as stream:
            writer = csv.writer(stream)
            writer.writerow("Sample,Seconds,Phase,Role,Pid,CreatedTicks,CpuSeconds,PrivateBytes,WorkingSetBytes,Threads,Handles".split(","))
            for index, second in enumerate([0, 6]):
                writer.writerow([index, second, "measure", "app", 1, 100, second*.01, 1000, 2000, 3, 10])
                writer.writerow([index, second, "measure", "fixture-child", 100, 101, second*.01, 3000, 4000, 1, 5])
                writer.writerow([index, second, "measure", "fixture-child", 101, 102, second*.01, 3000, 4000, 1, 5])
        class Args: pass
        args = Args()
        args.directory = evidence
        cli.summarize(args)
        summary = json.loads((evidence / "summary.json").read_text())
        self.assertAlmostEqual(summary["processes"]["app"]["one_core_cpu_percent_sum"], 1)
        self.assertAlmostEqual(summary["processes"]["fixture-child"]["one_core_cpu_percent_sum"], 2)
        self.assertIsNone(summary["processes"]["conpty-host"]["sum_private_bytes_by_sample"]["median"])
        args.roots, args.output = [evidence.parent], self.root / "comparison.json"
        cli.compare(args)
        result = json.loads(args.output.read_text())
        self.assertEqual(result["selection"], "PENDING")
        self.assertTrue(any("dry runs" in value for value in result["blockers"]))
        (evidence / "collector.json").write_text(json.dumps({"Success": True, "Survivors": [999], "AppExitCode": 0, "StderrReaderFinished": True}))
        with self.assertRaises(ValueError): cli.summarize(args)


if __name__ == "__main__": unittest.main()
