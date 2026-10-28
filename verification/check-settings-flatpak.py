#!/usr/bin/env -S uv run --python 3.15
"""Check the installed Settings drawer with isolated persisted theme choices.

Both launches are Flatpak instances. Preserve the shared COSMIC theme directory
when isolating XDG_CONFIG_HOME, and never change the real app preferences.
"""
import hashlib
import json
import os
import pathlib
import subprocess
import tempfile
from PIL import Image, ImageStat

root = pathlib.Path(__file__).resolve().parents[1]
area = root / "verification/flatpak-packaging"
home = pathlib.Path.home()
task = home / "Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic"
fixture = pathlib.Path(tempfile.mkdtemp(prefix="settings-", dir=task))
config = fixture / "config"
(config / "warehouse-cosmic").mkdir(parents=True)
(config / "cosmic").symlink_to(home / ".config/cosmic", target_is_directory=True)
actual = home / ".var/app/dev.nilltadios.WarehouseCosmic/config/warehouse-cosmic/preferences.json"
before = actual.read_bytes() if actual.exists() else None
preferences = config / "warehouse-cosmic/preferences.json"
helper = "on-screen.sh"
records = []
brightness = {}
for theme in ("dark", "light"):
    content = json.dumps({"theme": theme})
    preferences.write_text(content)
    capture = fixture / (theme + ".png")
    command = [helper, "DP-1", "dev.nilltadios.WarehouseCosmic", "--", "flatpak", "run", "--command=env", "dev.nilltadios.WarehouseCosmic", "XDG_CONFIG_HOME=" + str(config), "/app/bin/warehouse-cosmic", "--preview", "settings", "--capture", str(capture)]
    result = subprocess.run(command, cwd=root, text=True, capture_output=True)
    (fixture / (theme + ".log")).write_text(result.stdout + result.stderr)
    records.append({"command": command, "exit_status": result.returncode, "capture": str(capture), "sha256": hashlib.sha256(capture.read_bytes()).hexdigest() if capture.exists() else None})
    assert result.returncode == 0 and capture.exists(), f"Settings capture failed: {fixture}"
    assert preferences.read_text() == content, "Preview unexpectedly changed preferences"
    with Image.open(capture) as image:
        brightness[theme] = ImageStat.Stat(image.convert("RGB").crop((20, 500, 200, 600))).mean
after = actual.read_bytes() if actual.exists() else None
assert before == after, "The real app preferences were changed"
assert sum(brightness["light"]) / 3 > sum(brightness["dark"]) / 3 + 60, "Stored light and dark preferences did not change the rendered theme"
report = {"passed": True, "fixture": str(fixture), "captures": records,
          "installed_commit": subprocess.check_output(["flatpak", "info", "--user", "--show-commit", "dev.nilltadios.WarehouseCosmic"], text=True).strip(),
          "brightness_rgb": brightness, "real_preferences_unchanged": True,
          "verification": "Separate Flatpak launches read stored dark/light themes. Write/backup behavior is covered by Rust preference tests; preview does not write settings."}
(area / "settings-results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"passed": True, "fixture": str(fixture)}))
