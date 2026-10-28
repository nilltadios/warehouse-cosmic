#!/usr/bin/env -S uv run --python 3.15
"""Refresh only the prepared manifest's finish permissions, preserving its source.

The application archive/checksum and build modules are unchanged. This lets
flatpak-builder reuse its compiled module cache and re-export the new context.
Fresh CI preparation reads the corrected canonical manifest directly.
"""
import json
import pathlib
import shutil
root = pathlib.Path(__file__).resolve().parents[1]
name = "dev.nilltadios.WarehouseCosmic.json"
path = root / "flatpak/build-inputs" / name
canonical = json.loads((root / "flatpak" / name).read_text())
prepared = json.loads(path.read_text())
assert prepared["modules"][1]["sources"][0]["sha256"]
backup = path.with_suffix(".json.before-icon-permission")
if not backup.exists():
    shutil.copy2(path, backup)
prepared["finish-args"] = canonical["finish-args"]
path.write_text(json.dumps(prepared, indent=2) + "\n")
print(path)
