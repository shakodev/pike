"""Prepare an isolated local manifest, reusing the original pinned probe lock."""
from pathlib import Path
import json
import shutil

root = Path(__file__).resolve().parents[4]
original = root / "docs/baselines/issue-25/focus-probe"
probe = root / "target/issue-25-integration-probe"
probe.mkdir(parents=True, exist_ok=True)
manifest = (original / "Cargo.toml").read_text().split("\n[[bin]]", 1)[0]
manifest = manifest.replace("../../../..", str(root))
source = Path(__file__).with_name("pointer-integration.rs")
manifest += '\n[[bin]]\nname = "pointer-integration"\npath = ' + json.dumps(str(source)) + "\n"
(probe / "Cargo.toml").write_text(manifest)
shutil.copy2(original / "Cargo.lock", probe / "Cargo.lock")
print(probe / "Cargo.toml")
