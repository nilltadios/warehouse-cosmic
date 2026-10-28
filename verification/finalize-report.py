#!/usr/bin/env -S uv run --python 3.15
"""Collect saved checks and repeat read-only installation/cleanup comparisons.

Run from any directory. Exit 0 means the build/CLI/install/cleanup checks passed.
The separate host-state comparison can report changes made during the session;
these are recorded, never silently repaired or counted as an unchanged host.
"""
import datetime
import hashlib
import json
import pathlib
import re
import subprocess

root = pathlib.Path(__file__).resolve().parents[1]
out = root / "verification"
home = pathlib.Path.home()


def read_json(path):
    return json.loads(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(*args):
    return subprocess.run(args, cwd=root, capture_output=True, text=True)


checks_dir = pathlib.Path((out / "last-checks.txt").read_text().strip())
captures = pathlib.Path((out / "last-captures.txt").read_text().strip())
checks = read_json(checks_dir / "results.json")
catalog = run(str(home / ".local/bin/warehouse-cosmic-cli"), "commands", "--json")
(out / "installed-catalog.json").write_text(catalog.stdout)
catalog_data = json.loads(catalog.stdout)
installed = {}
for name in ("warehouse-cosmic", "warehouse-cosmic-cli"):
    destination = home / ".local/bin" / name
    installed[name] = {
        "path": str(destination),
        "sha256": sha(destination),
        "matches_release": sha(destination) == sha(root / "target/release" / name),
    }

host = run("./verification/check-host-state.sh")
(out / "host-state-final.log").write_text(host.stdout + host.stderr)
before = read_json(out / "host-inventory.json")
after = read_json(out / "host-inventory-final.json")["results"][0]["data"]
key = lambda p: (p["installation"], p["reference"])
previous = {key(p): p for p in before["packages"]}
current = {key(p): p for p in after["packages"]}
removed = sorted(set(previous) - set(current))
added = sorted(set(current) - set(previous))
diff = run("diff", "-u", "verification/host-before-state.json", "verification/host-after-state.json")
(out / "host-state-diff.txt").write_text(diff.stdout)
changes = []
for p in after["packages"]:
    old = previous.get(key(p))
    if old:
        for field in ("masked", "pinned"):
            if old[field] != p[field]:
                changes.append({"installation": p["installation"], "reference": p["reference"],
                                "field": field, "before": old[field], "after": p[field]})

def package_state(inv):
    return [{k: v for k, v in p.items() if k not in ("pins", "masks", "pinned", "masked")}
            for p in inv["packages"]]

pins = run("flatpak", "pin", "--user")
(out / "host-pins-final.txt").write_text(pins.stdout + pins.stderr)
processes = run("pgrep", "-a", "-x", "warehouse-cosmi")
flatpaks = run("flatpak", "ps", "--columns=application")
(out / "remaining-test-processes.txt").write_text(processes.stdout)
(out / "running-flatpaks-final.txt").write_text(flatpaks.stdout)
closed = processes.returncode == 1 and "io.github.flattool.Warehouse" not in flatpaks.stdout
desktop_log = (captures / "packages.log").read_text()
adapter_match = re.search(r"Selected: AdapterInfo \{(.*?)\n    \}", desktop_log, re.S)
adapter = adapter_match.group(0) if adapter_match else None
wayland = 'set_app_id("dev.nilltadios.WarehouseCosmic")' in desktop_log
pngs = sorted(captures.glob("*.png"))
capture_hash = (captures / "binary.sha256").read_text().split()[0]
deps = read_json(out / "dependencies.json")
all_current = all(d.get("current", True) for d in deps["direct_dependencies"].values())
icon = "dev.nilltadios.WarehouseCosmic.svg"
icon_matches = sha(root / "resources" / icon) == sha(home / ".local/share/icons/hicolor/scalable/apps" / icon)
launcher_log = (out / "launcher-probe-final.log").read_text()
test_log = (checks_dir / "tests.log").read_text()
test_count = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", test_log)))
report = {
    "schema": "warehouse-cosmic-final/1",
    "collected_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "reproduce": "uv run --python 3.15 verification/finalize-report.py",
    "automated_checks": checks,
    "tests_passed": test_count,
    "installed_commands": len(catalog_data["commands"]),
    "installed_binaries": installed,
    "installed_icon_matches": icon_matches,
    "launcher_probe_passed": "PASS:" in launcher_log,
    "launcher_evidence": str(out / "launcher"),
    "desktop": {"native_wayland": wayland, "selected_adapter": adapter,
                "capture_directory": str(captures), "capture_count": len(pngs),
                "captured_release_matches_installed": capture_hash == installed["warehouse-cosmic"]["sha256"],
                "screenshots": [{"path": str(p), "sha256": sha(p)} for p in pngs]},
    "dependencies": {"evidence": str(out / "dependencies.json"), "direct_crates_current": all_current,
                     "rust_matches_stable": deps["rustc"].removeprefix("rustc ") == deps["rust_latest_stable"],
                     "libcosmic_matches_head": deps["libcosmic_pinned"] == deps["libcosmic_head"]},
    "host_comparison": {"exit_status": host.returncode,
                        "packages_unchanged": package_state(before) == package_state(after),
                        "remotes_unchanged": before["remotes"] == after["remotes"],
                        "removed_refs": removed, "added_refs": added,
                        "mask_or_pin_differences": changes,
                        "note": "The host changed during final read-only checks, including runtime pin changes followed by runtime removals. No correction was applied; cause is not established.",
                        "diff": str(out / "host-state-diff.txt"),
                        "evidence": str(out / "host-state-final.log")},
    "test_apps_closed": closed,
    "limitations": ["Native chooser and clipboard mouse interactions were not manually exercised.",
                    "Interactive system PolicyKit authorization was not exercised; user-level transactions used isolated real repositories.",
                    "The real Parsec package was inspected read-only. Its extra-data install was not repeated.",
                    "Drag/drop installation, translations, persistent UI preferences and popular-remote shortcuts are absent; see docs/parity.md."],
}
passed = (checks["all_passed"] and catalog.returncode == 0 and closed and wayland and bool(adapter)
          and icon_matches and report["launcher_probe_passed"] and len(pngs) == 15
          and all(v["matches_release"] for v in installed.values()))
report["build_install_and_cleanup_passed"] = passed
(out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print(f"Build/install/cleanup: {'PASS' if passed else 'FAIL'}; {test_count} tests; {report['installed_commands']} commands; {len(pngs)} captures")
print(f"Host comparison exit {host.returncode}; {len(changes)} recorded mask/pin differences")
print(out / "results.json")
raise SystemExit(0 if passed else 1)
