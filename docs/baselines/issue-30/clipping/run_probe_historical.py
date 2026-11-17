"""Run the unchanged reviewer pixel oracle against this worktree's source."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

receipts = Path(__file__).resolve().parent
root = receipts.parents[3]
os.chdir(root)
env = dict(os.environ, CARGO_BUILD_JOBS="4", CARGO_TARGET_DIR=str(root / "target"))
commands = []


def run(args, name):
    result = subprocess.run(args, env=env, capture_output=True, text=True)
    (receipts / (name + ".log")).write_text(result.stdout + result.stderr)
    commands.append({"argv": args, "exit_code": result.returncode})
    (receipts / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
    if result.returncode:
        raise RuntimeError(f"{name} failed; inspect its retained log")
    return result.stdout


messages = run([
    "cargo", "build", "--offline", "--locked", "-p", "pike", "--lib",
    "--message-format=json",
], "library-build")
libraries = {}
for line in messages.splitlines():
    artifact = json.loads(line)
    if artifact.get("reason") == "compiler-artifact":
        for filename in artifact["filenames"]:
            if filename.endswith(".rlib"):
                libraries[artifact["target"]["name"]] = filename

original = (receipts / "reviewer-original-probe.rs").read_text()
old_output = '"docs/baselines/issue-30-reviewer2"'
assert original.count(old_output) == 1
source = receipts / "author-green-probe.rs"
source.write_text(original.replace(old_output, '"docs/baselines/issue-30/clipping"'))
binary = root / "target" / "r30-clipping-probe"
run([
    "rustc", "--edition=2024", "--crate-name", "r30_clipping_probe", str(source),
    "-L", "dependency=" + str(root / "target/debug/deps"),
    "--extern", "iced=" + libraries["iced"],
    "--extern", "pike=" + libraries["pike"],
    "-o", str(binary),
], "probe-build")
output = run([str(binary)], "native-green")

# The remaining finite actual-App driver is not wired before the R26 handoff.
# Compile just the newly prepared helpers against these exact Cargo artifacts.
harness = root / "target" / "r30-fixture-helpers.rs"
harness.write_text("\n".join(
    '#[allow(dead_code)]\n#[path = ' + json.dumps(str(root / "examples/support" / filename))
    + ']\nmod ' + module + ';'
    for module, filename in [
        ("child", "tab_management_child.rs"),
        ("widget", "tab_management_widget.rs"),
        ("snapshot", "tab_management_snapshot.rs"),
    ]
))
run([
    "rustc", "--edition=2024", "--crate-name", "r30_fixture_helpers", "--crate-type", "lib",
    str(harness), "-L", "dependency=" + str(root / "target/debug/deps"),
    "--extern", "iced=" + libraries["iced"],
    "--extern", "iced_term=" + libraries["iced_term"],
    "--extern", "pike=" + libraries["pike"],
    "-o", str(root / "target/libr30_fixture_helpers.rlib"),
], "fixture-helper-build")
provenance = {
    "source": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
    "renderer": "native macOS/default wgpu; synthetic standalone pixel probe",
    "reviewer_red_source": "ca4b9aaa0449e7758be9c41eece45b2fb0f47687",
    "production_sha256": {
        path: hashlib.sha256((root / path).read_bytes()).hexdigest()
        for path in ["src/tab_strip.rs", "src/tab_strip/reveal.rs", "Cargo.toml", "Cargo.lock"]
    },
    "artifact_sha256": {
        path.name: hashlib.sha256(path.read_bytes()).hexdigest()
        for path in receipts.iterdir() if path.is_file() and path.name != "provenance.json"
    },
    "native_windows": False,
    "physical_keyboard_monitor": False,
    "release_measurements": False,
    "remote_ci": False,
}
(receipts / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
print(output.strip())
print("Prepared child, actual-App widget and stable-owner snapshot helpers compile.")
