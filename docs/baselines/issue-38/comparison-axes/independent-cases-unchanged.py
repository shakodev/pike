"""Synthetic comparison-axis probes; never native build/cost measurements."""
import csv
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import unittest

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "tools"))
spec = importlib.util.spec_from_file_location("comparison_axes_cli", ROOT / "tools/renderer-comparison.py")
cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cli)
SEED = OUT / "independent-adapter-7221910/test_control_complete_synthetic_cost_data_is_accepted/synthetic-unit-data"


class ComparisonAxes(unittest.TestCase):
    def setUp(self):
        self.archive = OUT / "synthetic-comparison-axes-7221910-refined" / self._testMethodName
        self.archive.mkdir(parents=True)
        self.roots = []
        for repeat, name in enumerate(["left", "right"]):
            root = self.archive / name
            shutil.copytree(SEED, root)
            self.roots.append(root)
            run = root / "run-0000"
            meta = json.loads((run / "run.json").read_text())
            meta["repeat"] = repeat
            meta["build"].update(profile={"opt-level": 3}, app_sha256="a"*64, fixture_sha256="b"*64)
            self.set_meta(root, meta)
            if repeat:
                collector = json.loads((run / "collector.json").read_text())
                collector["AppCreatedTicks"] += 1000
                (run / "collector.json").write_text(json.dumps(collector))
                for filename in ["owned-tree.csv", "process.csv"]:
                    path = run / filename
                    with path.open() as stream:
                        rows = list(csv.DictReader(stream))
                    for row in rows:
                        row["CreatedTicks"] = str(int(row["CreatedTicks"]) + 1000)
                    with path.open("w", newline="") as stream:
                        writer = csv.DictWriter(stream, fieldnames=rows[0].keys())
                        writer.writeheader()
                        writer.writerows(rows)

    def set_meta(self, root, meta):
        (root / "run-0000/run.json").write_text(json.dumps(meta))
        (root / "run-manifest.json").write_text(json.dumps({"schema": 1, "runs": [meta]}))

    def compare(self):
        for root in self.roots:
            cli.summarize(type("Args", (), {"directory": root / "run-0000"})())
        args = type("Args", (), {"roots": self.roots, "output": self.archive / "comparison.json"})()
        cli.compare(args)
        return json.loads(args.output.read_text())

    def test_control_matching_build_and_terminal_geometry_can_pool(self):
        result = self.compare()
        self.assertEqual(len(result["groups"]), 1)
        self.assertEqual(result["groups"][0]["run_count"], 2)

    def test_different_binary_profile_and_size_cannot_be_silently_pooled(self):
        root = self.roots[1]
        meta = json.loads((root / "run-0000/run.json").read_text())
        meta["build"].update(profile={"opt-level": 1}, app_sha256="c"*64,
                             fixture_sha256="d"*64, app_bytes=200)
        self.set_meta(root, meta)
        try:
            result = self.compare()
        except ValueError:
            return
        self.assertTrue(len(result["groups"]) > 1 or any("build" in reason and "ident" in reason for reason in result["blockers"]),
                        "different binary/config identities pooled with the first artifact size")

    def test_equal_window_dip_does_not_hide_different_terminal_cell_geometry(self):
        root = self.roots[1]
        meta = json.loads((root / "run-0000/run.json").read_text())
        meta["build"]["renderer"] = "wgpu"
        self.set_meta(root, meta)
        for name in ["before", "after"]:
            path = root / f"run-0000/fixture/counters-{name}.csv"
            with path.open() as stream:
                rows = list(csv.DictReader(stream))
            for row in rows:
                row["columns"] = "80"
            with path.open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=rows[0].keys())
                writer.writeheader()
                writer.writerows(rows)
        try:
            result = self.compare()
        except ValueError:
            return
        self.assertTrue(any("geometry" in reason for reason in result["blockers"]),
                        "120-column and 80-column terminal states compare without a geometry blocker")


if __name__ == "__main__":
    unittest.main(verbosity=2)
