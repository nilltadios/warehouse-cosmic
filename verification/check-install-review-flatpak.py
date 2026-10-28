#!/usr/bin/env -S uv run --python 3.15
"""Open real local bundles in the installed app, capture review and exit.

No Install button is activated. Host package/remote state must stay unchanged.
"""
import hashlib
import json
import pathlib
import shutil
import subprocess
import tempfile
from PIL import Image

root = pathlib.Path(__file__).resolve().parents[1]
area = root / "verification/flatpak-packaging"
app = "dev.nilltadios.WarehouseCosmic"
task = pathlib.Path.home() / "Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic"
output = pathlib.Path(tempfile.mkdtemp(prefix="install-review-", dir=task))
source = root / "flatpak/out" / (app + ".flatpak")
bundle = output / "Warehouse-COSMIC.flatpak"
shutil.copy2(source, bundle)
other = output / "A-second-package-with-a-long-filename-to-check-wrapping-in-the-review-dialog.flatpak"
shutil.copy2(source, other)
records = []

def run(name, args):
    result = subprocess.run(args, cwd=root, text=True, capture_output=True)
    (output / (name + ".log")).write_text(result.stdout + result.stderr)
    records.append({"name":name, "command":args, "exit_status":result.returncode})
    (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    assert result.returncode == 0, f"{name} failed; see {output}"
    return result.stdout

def inventory(name):
    data = json.loads(run(name, ["flatpak", "run", "--command=warehouse-cosmic-cli", app, "inspect"]))["results"][0]["data"]
    return {key:data[key] for key in ("packages", "remotes")}

before = inventory("before")
captures = []
for name, files, preview in (("single", [bundle], "install"), ("multiple-narrow", [bundle, other], "remotes-narrow")):
    path = output / (name + ".png")
    command = ["on-screen.sh", "DP-1", app, "--", "flatpak", "run", app, "--preview", preview, "--capture", str(path)] + list(map(str, files))
    run(name, command)
    with Image.open(path) as image: size = image.size
    captures.append({"path":str(path), "size":size, "sha256":hashlib.sha256(path.read_bytes()).hexdigest()})
assert before == inventory("after"), "Opening the review unexpectedly changed host packages/remotes"
report = {"passed":True, "installed_commit":run("commit", ["flatpak", "info", "--user", "--show-commit", app]).strip(),
          "output":str(output), "captures":captures, "host_state_unchanged":True,
          "bundle_source":str(source), "bundle_sha256":hashlib.sha256(bundle.read_bytes()).hexdigest(),
          "limitations":"Install confirmation was not clicked. Backend installation behavior is covered by isolated fixture tests."}
(area / "install-review-results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"passed":True, "output":str(output)}))
