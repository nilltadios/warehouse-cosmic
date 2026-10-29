#!/usr/bin/env -S uv run --python 3.15
"""Collect the current Flatpak build, normal-install checks and fixture evidence."""
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import tarfile

root = pathlib.Path(__file__).resolve().parents[1]
out = root / "verification/flatpak-packaging"
app = "dev.nilltadios.WarehouseCosmic"
checks = []
def run(name, args):
    result = subprocess.run(args, cwd=root, text=True, capture_output=True)
    (out / (name + ".log")).write_text(result.stdout + result.stderr)
    checks.append({"name": name, "command": args, "exit_status": result.returncode})
    return result.stdout
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

run("final-format", ["cargo", "fmt", "--all", "--check"])
run("final-shellcheck", ["shellcheck", "flatpak/build.sh", "flatpak/install.sh", "install.sh", "uninstall.sh", "verification/capture-ui.sh"])
run("final-workflow-lint", [str(out / "actionlint/actionlint"), ".github/workflows/flatpak.yml"])
run("final-appstream", ["appstreamcli", "validate", "--no-net", "resources/" + app + ".metainfo.xml"])
run("final-normal-install", ["uv", "run", "--python", "3.15", "verification/verify-installed-flatpak.py"])
run("final-build-tools-removed", ["flatpak", "run", "--command=sh", app, "-c", "test ! -d /app/lib/rust"])
normal = json.loads((out / "normal-install-results.json").read_text())
consumer = json.loads((out / "consumer-results.json").read_text())
publisher = json.loads((out / "publisher-results.json").read_text())
settings = json.loads((out / "settings-results.json").read_text())
desktop_checks = {name: json.loads((out / (name + "-results.json")).read_text())
                  for name in ("remotes", "install-review", "window-state", "single-instance", "ui-cli")}
presets = json.loads((out / "popular-remotes-results.json").read_text())
tests = (out / "tests-ui-cli.log").read_text()
count = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", tests)))
clippy = (out / "clippy-ui-cli.log").read_text()
location = run("final-installed-location", ["flatpak", "info", "--user", "--show-location", app]).strip()
exported = run("final-exported-commit", ["ostree", "--repo=flatpak/out/repo", "rev-parse", "app/" + app + "/x86_64/stable"]).strip()
instances = run("final-running-instances", ["flatpak", "ps", "--columns=application,commit"])
bundle = root / "flatpak/out" / (app + ".flatpak")
core = {}
baseline = pathlib.Path(consumer["source_archive"])
source_matches = digest(baseline) == consumer["source_sha256"]
with tarfile.open(baseline) as archive:
    paths = [path for folder in ("src", "resources", "docs", "schemas") for path in (root / folder).rglob("*") if path.is_file()]
    paths += [root / name for name in ("Cargo.toml", "Cargo.lock", "LICENSE", "NOTICE", "llms.txt")]
    for path in paths:
        name = str(path.relative_to(root))
        core[name] = hashlib.sha256(archive.extractfile(name).read()).hexdigest() == digest(path)
passed = (all(c["exit_status"] == 0 for c in checks) and normal["passed"] and consumer["passed"]
          and publisher["passed"] and count >= 32 and "test result: FAILED" not in tests and "Finished" in clippy and all(core.values())
          and normal["commit"] == exported and source_matches and settings["passed"]
          and settings["installed_commit"] == exported
          and all(value["passed"] and value["installed_commit"] == exported for value in desktop_checks.values()))
report = {
    "schema": "warehouse-cosmic-flatpak-verification/1",
    "collected_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "local_build_install_and_checks_passed": passed,
    "reproduce": "uv run --python 3.15 verification/finalize-flatpak.py",
    "tests_passed": count, "tests_log": str(out / "tests-ui-cli.log"),
    "clippy_log": str(out / "clippy-ui-cli.log"), "checks": checks,
    "installed": normal,
    "installed_matches_latest_export": normal["commit"] == exported,
    "binary_sha256": digest(pathlib.Path(location) / "files/bin/warehouse-cosmic"),
    "bundle": {"path": str(bundle), "sha256": digest(bundle), "bytes": bundle.stat().st_size},
    "consumer_fixture": consumer,
    "settings": settings,
    "desktop_checks": desktop_checks,
    "popular_remote_sources": presets,
    "header_tooltips": {"implementation": "src/app.rs::header_tooltip", "position": "Bottom", "gap_logical_pixels": 8,
                        "buttons": ["Warehouse COSMIC", "Toggle sidebar", "Refresh", "Settings", "About Warehouse COSMIC"],
                        "manual_check": "Hover each named header button at normal and narrow window widths. The label should sit below the button with a clear gap."},
    "consumer_archive_matches": source_matches,
    "consumer_backend_sources_match_current": core,
    "signed_publisher_fixture": publisher,
    "runtime_evidence": [str(out / "runtime.txt"), str(out / "runtime-remote.txt"), str(out / "sdk.txt"), str(out / "sdk-remote.txt")],
    "toolchain_and_crates": str(root / "verification/dependencies.json"),
    "normal_icon_evidence": settings["captures"][0]["capture"],
    "source_backups": [str(out / "inputs-before-about-data-filter"), str(out / "inputs-before-narrow-capture"), str(out / "inputs-before-kde-presentation"), str(out / "inputs-before-header-logo"), str(out / "inputs-before-single-instance"), "/home/nilllinux22/Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic/backups"],
    "rollback": "Restore the saved source files, then rebuild and reinstall the Flatpak with the documented packaging scripts. The previous prepared inputs are retained under verification/flatpak-packaging/inputs-before-theme-fix.",
    "running_instances": instances.strip().splitlines(),
    "publication": {"workflow_prepared": True, "public_deployment_performed": False,
                    "remaining_setup": "Choose the GitHub repository, configure its stable signing key and enable Pages."},
    "resolved_check_failures": [
        {"issue": "Window restoration overrode the narrow preview size", "fix": "Use the requested capture width as the initial normal window size", "log": str(out / "remotes-final.log")},
        {"issue": "A new user window was mistaken for a capture leak", "fix": "Track and verify only explicitly named capture instances", "log": str(out / "remotes-final.log")},
        {"issue": "Wayland ignored the generic unminimize request", "fix": "Use KWin's default window-runner action for the exact GUI process on explicit activation", "log": str(out / "single-instance-final.log")},
        {"issue": "GUI launch helper kept the CLI sandbox waiting", "fix": "Detach the GUI launch through host setsid and keep exact-instance replies", "log": str(out / "live-ui-cli-final.log")},
        {"issue": "Builder stripped the temporary Rust toolchain", "fix": "Disable stripping/debug extraction for that module", "log": str(out / "build.log")},
        {"issue": "Source archive was extracted with one path component stripped", "fix": "Set strip-components=0 for the app archive", "log": str(out / "build-final.log")},
        {"issue": "Instance cleanup query ran before Flatpak retired the renderer", "fix": "Bounded wait after captures; never close a user window", "log": str(out / "consumer-test.log")},
        {"issue": "Isolated runtime path masked missing normal-install icon access", "fix": "Explicit read-only user Flatpak directory grant and a normal-install regression check", "log": str(out / "installed-icon-access.log")},
        {"issue": "Stored Light choice rendered a dark palette when COSMIC Light config was missing", "fix": "Validate the loaded palette, build the requested fallback and keep explicit choices independent of desktop mode", "evidence": "/home/nilllinux22/Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic/settings-y88kp1qk"},
        {"issue": "Debug capture target outside permitted home was unavailable", "fix": "Save debug captures inside the home task bundle", "log": str(out / "installed-icons-capture.log")}
    ],
    "limitations": ["Interactive system PolicyKit authorization and native file chooser/clipboard mouse interaction were not automatically exercised.",
                    "Public GitHub Pages deployment has not been performed.",
                    "Remaining parity differences are listed in docs/parity.md."]
}
(out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print(f"{'PASS' if passed else 'FAIL'}: {count} tests, normal Flatpak installation, desktop Trash, signed publisher, and static checks")
print(out / "results.json")
raise SystemExit(0 if passed else 1)
