#!/usr/bin/env -S uv run --python 3.15
"""Read-only checks of the NORMAL installed Flatpak, without runtime overrides.

This complements the isolated fixture tests. In particular, a custom test
FLATPAK_SYSTEM_DIR can expose icon paths that a normal installation hides.
"""
import datetime
import json
import os
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[1]
out = root / "verification/flatpak-packaging"
app = "dev.nilltadios.WarehouseCosmic"
env = {k: v for k, v in os.environ.items() if k not in ("FLATPAK_USER_DIR", "FLATPAK_SYSTEM_DIR")}
checks = []
def run(name, *args):
    result = subprocess.run(args, cwd=root, env=env, capture_output=True, text=True)
    (out / (name + ".log")).write_text(result.stdout + result.stderr)
    checks.append({"name": name, "command": args, "exit_status": result.returncode})
    assert result.returncode == 0, f"{name} failed; see {out}"
    return result.stdout

permissions = run("normal-permissions", "flatpak", "info", "--user", "--show-permissions", app)
assert "~/.local/share/flatpak:ro" in permissions
assert "sockets=wayland;" in permissions
assert "org.freedesktop.Flatpak=talk" in permissions
assert "xdg-config/cosmic:ro" in permissions
run("normal-cosmic-theme-access", "flatpak", "run", "--command=sh", app, "-c",
    'set -eu; test -r "$HOME/.config/cosmic"; test ! -w "$HOME/.config/cosmic"; test -r "$XDG_CONFIG_HOME/cosmic"; test ! -w "$XDG_CONFIG_HOME/cosmic"; test -w "$XDG_CONFIG_HOME"; printf "PASS: shared COSMIC theme is read-only; private app config is writable\\n"')
run("normal-icon-access", "flatpak", "run", "--command=sh", app, "-c",
    'set -eu; icons="$HOME/.local/share/flatpak/exports/share/icons/hicolor/128x128/apps"; test -r "$icons/ai.storyteller.artcraft-launcher.png"; test -r "$icons/ai.storyteller.deckcraft.png"; printf "PASS: real exported icons are readable\\n"')
run("normal-host-trash-helper", "flatpak", "run", "--command=flatpak-spawn", app, "--host", "gio", "--version")
catalog = json.loads(run("normal-catalog", "flatpak", "run", "--command=warehouse-cosmic-cli", app, "commands", "--json"))
assert len(catalog["commands"]) == 27
commit = run("normal-installed-commit", "flatpak", "info", "--user", "--show-commit", app).strip()
runtime = run("normal-runtime", "flatpak", "info", "--user", "--show-runtime", app).strip()
assert runtime == "org.freedesktop.Platform/x86_64/26.08"
(out / "normal-install-results.json").write_text(json.dumps({"passed": True, "checked_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "commit": commit, "runtime": runtime, "checks": checks}, indent=2) + "\n")
print(f"PASS: normal installed Flatpak {commit[:12]}, icon permissions, host Trash helper, and CLI")
