#!/usr/bin/env -S uv run --python 3.15
"""Install into an isolated Flatpak directory; test the real packaged consumer.

Shares the existing user runtime directory read-only as the test's system
runtime source. No real installation or existing app data is modified. A unique
disposable app-data directory is created under ~/.var/app and moved to the
desktop Trash. Fixture files, install state, and logs remain for inspection.
GUI captures autoexit; an existing user window is never closed.
"""
import configparser
import datetime
import hashlib
import json
import os
import pathlib
import subprocess
import tempfile
import time
import urllib.parse
import uuid

root = pathlib.Path(__file__).resolve().parents[1]
home = pathlib.Path.home()
app = "dev.nilltadios.WarehouseCosmic"
area = root / "verification/flatpak-packaging"
fixture = pathlib.Path(tempfile.mkdtemp(prefix="consumer-", dir=area))
home_area = home / "Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic"
home_area.mkdir(parents=True, exist_ok=True)
data_fixture = pathlib.Path(tempfile.mkdtemp(prefix="flatpak-permissions-", dir=home_area))
identity = "org.example.WarehousePermissionProbe" + fixture.name.split("-")[-1]
app_data = home / ".var/app" / identity
assert not app_data.exists(), "Refusing to reuse any existing app-data directory"
app_data.mkdir(mode=0o700)
(app_data / "probe.txt").write_text("Disposable Warehouse COSMIC permission verification.\n")
env = {**os.environ, "FLATPAK_USER_DIR": str(fixture / "installation"),
       "FLATPAK_SYSTEM_DIR": str(home / ".local/share/flatpak"), "LC_ALL": "C"}
gnupg = fixture / "gnupg"
gnupg.mkdir(mode=0o700)
sign_env = {**os.environ, "GNUPGHOME": str(gnupg), "LC_ALL": "C"}
records = []


def run(name, args, environment=env, expected=0):
    result = subprocess.run(args, cwd=root, env=environment, text=True, capture_output=True)
    (fixture / (name + ".log")).write_text(result.stdout + result.stderr)
    records.append({"name": name, "command": args, "exit_status": result.returncode,
                    "expected_status": expected, "log": str(fixture / (name + ".log"))})
    (fixture / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    assert result.returncode == expected, f"{name}: exit {result.returncode}; see {fixture}"
    return result.stdout


def cli(name, *args, data=False):
    command = ["flatpak", "run", "--nosocket=wayland", "--nosocket=x11", "--nosocket=fallback-x11"]
    if data:
        command += ["--command=env", app, "XDG_DATA_HOME=" + str(data_fixture / "data"), "/app/bin/warehouse-cosmic-cli"]
    else:
        command += ["--command=warehouse-cosmic-cli", app]
    return json.loads(run(name, command + list(args)))


def request(name, command, params, apply=False, data=False):
    args = ["run", "--cmd", json.dumps({"command": command, "params": params})]
    if apply:
        args += ["--apply"]
    return cli(name, *args, data=data)


def desktop_trashed(original):
    # Inspect only metadata for the disposable paths created by this script.
    trash = home / ".local/share/Trash"
    for info in (trash / "info").glob("*.trashinfo"):
        text = info.read_text(errors="replace")
        if any(line.startswith("Path=") and urllib.parse.unquote(line[5:]) == str(original) for line in text.splitlines()):
            target = trash / "files" / info.name.removesuffix(".trashinfo")
            if target.exists():
                return str(target)
    raise AssertionError(f"No desktop Trash entry for {original}")


def app_instances(output):
    return {line.split()[1] for line in output.splitlines()
            if len(line.split()) == 2 and line.split()[0] == app}


try:
    existing_instances = app_instances(run("existing-instances", ["flatpak", "ps", "--columns=application,instance"]))
    repo = root / "flatpak/out/repo"
    commit = run("commit", ["ostree", f"--repo={repo}", "rev-parse", f"app/{app}/x86_64/stable"], sign_env).strip()
    run("generate-fixture-key", ["gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "--quick-generate-key", "Warehouse Package Fixture <fixture@example.invalid>", "ed25519", "sign", "0"], sign_env)
    keys = run("fixture-key", ["gpg", "--batch", "--with-colons", "--list-secret-keys"], sign_env)
    key = next(line.split(":")[9] for line in keys.splitlines() if line.startswith("fpr:"))
    run("sign-app", ["ostree", f"--repo={repo}", "gpg-sign", commit, key], sign_env)
    run("sign-summary", ["flatpak", "build-update-repo", f"--gpg-sign={key}", str(repo)], sign_env)
    site = fixture / "site"
    run("prepare-site", ["uv", "run", "--python", "3.15", "ci/publish.py", str(repo), str(site), "https://example.invalid/warehouse/", key, str(root / "flatpak/build-inputs/inputs.json"), "local-verification"], sign_env)
    config = configparser.ConfigParser(interpolation=None)
    config.optionxform = str
    config.read(site / "warehouse-cosmic.flatpakrepo")
    config["Flatpak Repo"]["Url"] = repo.as_uri()
    local_repo = fixture / "local.flatpakrepo"
    with local_repo.open("w") as file:
        config.write(file)
    run("add-isolated-remote", ["flatpak", "remote-add", "--user", "warehouse-cosmic-fixture", str(local_repo)])
    run("install-package", ["flatpak", "install", "--user", "--no-deps", "--noninteractive", "-y", "warehouse-cosmic-fixture", app])
    permissions = run("permissions", ["flatpak", "info", "--user", "--show-permissions", app])
    assert "sockets=wayland;" in permissions and "x11" not in permissions
    assert "org.freedesktop.Flatpak=talk" in permissions and "~/.var/app" in permissions
    catalog = cli("headless-discovery", "commands", "--json")
    assert len(catalog["commands"]) == 27
    denied = json.loads(run("denied-host-permission", ["flatpak", "run", "--no-talk-name=org.freedesktop.Flatpak", "--nosocket=wayland", "--command=warehouse-cosmic-cli", app, "inspect"], expected=1))
    assert denied["ok"] is False and app_data.is_dir()
    info = request("real-parsec-info", "package.info", {"installation": "user", "reference": "app/com.parsecgaming.parsec/x86_64/stable"})
    assert info["results"][0]["data"]["package"]["origin"] == "parsec-autobuild"
    plan = request("update-plan", "package.update", {"installation": "user", "references": ["app/com.parsecgaming.parsec/x86_64/stable"]})
    assert plan["results"][0]["mode"] == "dry_run"
    request("snapshot-create", "snapshot.create", {"ids": [identity], "name": "Permission fixture"}, apply=True, data=True)
    snapshots = sorted((data_fixture / "data/warehouse-cosmic/Snapshots").rglob("*.tar.zst"))
    assert len(snapshots) == 1
    request("data-trash-plan", "data.trash", {"ids": [identity]}, data=True)
    assert app_data.is_dir()
    request("data-trash-apply", "data.trash", {"ids": [identity]}, apply=True, data=True)
    assert not app_data.exists()
    trashed_data = desktop_trashed(app_data)
    listing = request("data-readback", "data.list", {}, data=True)
    assert identity not in json.dumps(listing)
    request("snapshot-trash", "snapshot.trash", {"paths": [str(snapshots[0])]}, apply=True, data=True)
    assert not snapshots[0].exists()
    trashed_snapshot = desktop_trashed(snapshots[0])
    helper = "on-screen.sh"
    capture_names = []
    for page in ("packages", "remotes", "leftovers", "appearance"):
        capture = data_fixture / (page + ".png")
        instance = app + ".UI.capture" + uuid.uuid4().hex
        capture_names.append(instance)
        command = [helper, "DP-1", app, "--", "flatpak", "run", "--env=WAREHOUSE_COSMIC_UI_INSTANCE="+instance, "--env=RUST_LOG=warn,iced_wgpu=info", "--env=WAYLAND_DEBUG=1", app, "--preview", page, "--capture", str(capture)]
        run("capture-" + page, command)
        assert capture.stat().st_size > 1000
    denied_capture = data_fixture / "permission-error.png"
    instance = app + ".UI.capture" + uuid.uuid4().hex
    capture_names.append(instance)
    run("capture-permission-error", [helper, "DP-1", app, "--", "flatpak", "run", "--env=WAREHOUSE_COSMIC_UI_INSTANCE="+instance, "--no-talk-name=org.freedesktop.Flatpak", app, "--preview", "packages", "--capture", str(denied_capture)])
    assert denied_capture.stat().st_size > 1000
    # Check only our named GUIs. A user may open another normal window while
    # these captures run; it must never be mistaken for a leaked test window.
    for instance in capture_names:
        owner = json.loads(run("capture-owner",["busctl","--user","--json=short","call","org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus","NameHasOwner","s",instance]))
        assert owner["data"] == [False], "A package test GUI remained open"
    remaining = app_instances(run("remaining-instances", ["flatpak", "ps", "--columns=application,instance"]))
    result = {"passed": True, "checked_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "fixture": str(fixture), "home_fixture": str(data_fixture), "disposable_app_id": identity,
              "runtime": "org.freedesktop.Platform/x86_64/26.08", "commands": records,
              "source_archive": str(root / "flatpak/build-inputs/warehouse-cosmic-source.tar.gz"),
              "source_sha256": hashlib.sha256((root / "flatpak/build-inputs/warehouse-cosmic-source.tar.gz").read_bytes()).hexdigest(),
              "existing_instances_preserved": sorted(existing_instances & remaining),
              "capture_instances_closed":capture_names,"new_user_instances_untouched":sorted(remaining-existing_instances),
              "desktop_trash": {"app_data": trashed_data, "snapshot": trashed_snapshot},
              "notes": ["Real Parsec operations were read-only/dry-run.", "All package test windows autoexited. Existing user windows were preserved.", "Interactive PolicyKit authorization was not exercised."]}
    (area / "consumer-results.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": True, "fixture": str(fixture), "captures": str(data_fixture)}))
finally:
    subprocess.run(["gpgconf", "--homedir", str(gnupg), "--kill", "gpg-agent"], capture_output=True)
    marker = app_data / "probe.txt"
    if marker.is_file() and marker.read_text() == "Disposable Warehouse COSMIC permission verification.\n":
        cleanup = subprocess.run(["gio", "trash", "--", str(app_data)], text=True, capture_output=True)
        (fixture / "disposable-cleanup.log").write_text(cleanup.stdout + cleanup.stderr)
