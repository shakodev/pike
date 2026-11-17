"""Synthetic rejection probes only: these values are not machine measurements."""
import copy
import csv
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))
import test_renderer_comparison as seed_module
from renderer_comparison_support import validate_fixture

spec = importlib.util.spec_from_file_location("reviewed_cli", ROOT / "tools/renderer-comparison.py")
cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cli)


class IndependentRejectionTests(unittest.TestCase):
    def setUp(self):
        seed = seed_module.EvidenceTests()
        seed.setUp()
        self.addCleanup(seed.doCleanups)
        seed.events.append([6012, "measurement_epoch", 0, 10000, 16000])
        seed.save()
        self.temp = tempfile.TemporaryDirectory(prefix="pike-r38-synthetic-unit-only-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write_json(self.root / "scope.json", {"scope": "SYNTHETIC UNIT TEST ONLY; invalid rows and summaries are never native measurements"})
        self.run = self.root / "run-0000"
        self.run.mkdir()
        import shutil
        shutil.copytree(seed.root, self.run / "fixture")
        self.meta = {"visible": 1, "hidden": 1, "history": "saturated", "workload": "idle", "probes": 0,
                     "seconds": 6, "dry_run": True, "build": {"renderer": "tiny-skia", "variant": "baseline",
                     "source_commit": "SYNTHETIC-UNIT-TEST-NOT-A-NATIVE-RESULT", "target": "x86_64-pc-windows-msvc",
                     "app_bytes": 100, "executable_archive_bytes": 50, "portable_archive_bytes": 100}}
        self.write_json(self.run / "run.json", self.meta)
        self.write_json(self.root / "run-manifest.json", {"schema": 1, "runs": [copy.deepcopy(self.meta)]})
        self.write_json(self.root / "machine.json", {"OS": {"BuildNumber": 26100}, "scope": "SYNTHETIC UNIT TEST"})
        self.write_json(self.run / "gpu-availability.json", {"Available": False, "Failure": "SYNTHETIC unavailable"})
        self.write_json(self.run / "collector.json", {"Success": True, "Survivors": [], "AppExitCode": 0, "AppPid": 99})
        self.rows = [
            [0, 0, "measure", "app", 99, 1000, 0, 1000, 2000, 3, 10],
            [0, 0, "measure", "fixture-child", 100, 1001, 0, 3000, 4000, 1, 5],
            [0, 0, "measure", "fixture-child", 101, 1002, 0, 3000, 4000, 1, 5],
            [1, 6, "measure", "app", 99, 1000, .06, 1000, 2000, 3, 10],
            [1, 6, "measure", "fixture-child", 100, 1001, .12, 3000, 4000, 1, 5],
            [1, 6, "measure", "fixture-child", 101, 1002, .18, 3000, 4000, 1, 5],
        ]
        self.save_rows()
        self.args = type("Args", (), {"directory": self.run, "roots": [self.root], "output": self.root / "comparison.json"})()

    def write_json(self, path, value):
        path.write_text(json.dumps(value))

    def save_rows(self):
        with (self.run / "process.csv").open("w", newline="") as stream:
            writer = csv.writer(stream)
            writer.writerow("Sample,Seconds,Phase,Role,Pid,CreatedTicks,CpuSeconds,PrivateBytes,WorkingSetBytes,Threads,Handles".split(","))
            writer.writerows(self.rows)

    def tearDown(self):
        import shutil
        archive = Path(__file__).resolve().parent / "synthetic-rejection-evidence-54525c6" / self._testMethodName
        archive.parent.mkdir(exist_ok=True)
        shutil.copytree(self.root, archive)

    def test_control_complete_synthetic_cost_data_is_accepted(self):
        cli.summarize(self.args)
        summary = json.loads((self.run / "summary.json").read_text())
        self.assertAlmostEqual(summary["processes"]["app"]["one_core_cpu_percent_sum"], 1.0)
        self.assertAlmostEqual(summary["processes"]["fixture-child"]["one_core_cpu_percent_sum"], 5.0)

    def test_failed_cleanup_cannot_be_a_valid_cost_summary(self):
        self.write_json(self.run / "collector.json", {"Success": False, "Survivors": [2], "AppExitCode": -1})
        with self.assertRaises(ValueError):
            cli.summarize(self.args)

    def test_negative_cpu_delta_is_invalid_data(self):
        next(row for row in self.rows if row[3] == "app" and row[0] == 1)[6] = -.1
        self.save_rows()
        with self.assertRaises(ValueError):
            cli.summarize(self.args)

    def test_missing_child_cost_cannot_be_silently_accepted(self):
        self.rows = [r for r in self.rows if r[3] == "app"]
        self.save_rows()
        with self.assertRaises(ValueError):
            cli.summarize(self.args)

    def test_compare_revalidates_raw_cost_after_summary_was_written(self):
        cli.summarize(self.args)
        next(row for row in self.rows if row[3] == "app" and row[0] == 1)[6] = -.1
        self.save_rows()
        with self.assertRaises(ValueError):
            cli.compare(self.args)

    def test_run_identity_must_match_its_planned_manifest_row(self):
        cli.summarize(self.args)
        self.meta["build"]["renderer"] = "wgpu"
        self.meta["build"]["source_commit"] = "DIFFERENT-SYNTHETIC-UNIT-SOURCE"
        self.write_json(self.run / "run.json", self.meta)
        with self.assertRaises(ValueError):
            cli.compare(self.args)

    def test_nonfinite_native_geometry_is_invalid(self):
        (self.run / "fixture/geometry.csv").write_text("width_dip,height_dip,scale\nnan,1000,1\n")
        with self.assertRaises(ValueError):
            validate_fixture(self.run / "fixture", 1, 1, True, False, 0, 6)


if __name__ == "__main__":
    unittest.main(verbosity=2)
