#!/usr/bin/env -S uv run --python 3.15
"""Capture the normal installed remote manager; preserve existing user windows.

Previews navigate real UI state without applying remote/package changes. Inspect
the saved renders as well as the automated dimension and unchanged-state checks.
"""
import hashlib
import json
import pathlib
import subprocess
import tempfile
import time
import uuid
from PIL import Image

root = pathlib.Path(__file__).resolve().parents[1]
area = root / "verification/flatpak-packaging"
app = "dev.nilltadios.WarehouseCosmic"
task = pathlib.Path.home() / "Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic"
output = pathlib.Path(tempfile.mkdtemp(prefix="remotes-", dir=task))
records = []

def run(name, args):
    result = subprocess.run(args, cwd=root, text=True, capture_output=True)
    (output / (name + ".log")).write_text(result.stdout + result.stderr)
    records.append({"name": name, "command": args, "exit_status": result.returncode})
    (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    assert result.returncode == 0, f"{name} failed; see {output}"
    return result.stdout

def instances():
    return {line.split()[1] for line in run("instances", ["flatpak", "ps", "--columns=application,instance"]).splitlines()
            if len(line.split()) == 2 and line.split()[0] == app}

def inventory(name):
    return json.loads(run(name, ["flatpak", "run", "--command=warehouse-cosmic-cli", app, "inspect"]))["results"][0]["data"]

existing = instances()
before = inventory("inventory-before")
commit = run("installed-commit", ["flatpak", "info", "--user", "--show-commit", app]).strip()
helper = "on-screen.sh"
captures = {}
capture_names = []
for page in ("remotes", "remotes-narrow", "remotes-menu", "remotes-disabled", "remotes-search", "remotes-no-results", "remotes-details", "remotes-preset", "remote-filter"):
    path = output / (page + ".png")
    instance = app + ".UI.capture" + uuid.uuid4().hex
    capture_names.append(instance)
    run(page, [helper, "DP-1", app, "--", "flatpak", "run", "--env=WAREHOUSE_COSMIC_UI_INSTANCE="+instance, app, "--preview", page, "--capture", str(path)])
    with Image.open(path) as image:
        size = image.size
    assert min(size) > 500 and path.stat().st_size > 1000
    captures[page] = {"path": str(path), "size": size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
assert captures["remotes-narrow"]["size"][0] < captures["remotes"]["size"][0], "Narrow preview did not resize"
after = inventory("inventory-after")
assert before["remotes"] == after["remotes"], "Host remotes changed during UI review"
for instance in capture_names:
    owner = json.loads(run("capture-owner",["busctl","--user","--json=short","call","org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus","NameHasOwner","s",instance]))
    assert owner["data"] == [False], "A capture GUI remained open"
remaining = instances()
report = {"passed": True, "installed_commit": commit, "output": str(output),
          "captures": captures, "host_remotes_unchanged": True,
          "existing_instances_preserved": sorted(existing & remaining), "commands": records,
          "capture_instances_closed":capture_names,"new_user_instances_untouched":sorted(remaining-existing),
          "limitations": "Previews exercise view states and action handlers. Mouse, clipboard and native chooser interactions still need manual review."}
(area / "remotes-results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"passed": True, "output": str(output)}))
