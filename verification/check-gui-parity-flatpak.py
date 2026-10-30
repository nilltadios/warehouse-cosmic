#!/usr/bin/env -S uv run --python 3.15
# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow"]
# ///
"""Exercise the Warehouse parity work in the installed Flatpak without applying changes.

Portable across Wayland desktops: it uses only semantic `ui` controls, renderer
captures and widget bounds, never native pointer input. It starts one isolated
GUI with private preferences, refuses every confirmation without --apply,
compares host packages/remotes/data/snapshots before and after, and closes only
the instance it started. Remote searching contacts Flathub when it is configured.
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import time
from PIL import Image

root = pathlib.Path(__file__).resolve().parents[1]
app = "dev.nilltadios.WarehouseCosmic"
base = ["flatpak", "run", "--command=warehouse-cosmic-cli", app]
# Development iterations may point at another CLI; reports record which was used.
client = [os.environ["WAREHOUSE_TEST_UI_CLIENT"]] if os.environ.get("WAREHOUSE_TEST_UI_CLIENT") else base
output = root / "verification" / time.strftime("checks-gui-%Y%m%d-%H%M%S")
output.mkdir(parents=True)
config = output / "config"
(config / "warehouse-cosmic").mkdir(parents=True)
(config / "cosmic").symlink_to(pathlib.Path.home() / ".config/cosmic", target_is_directory=True)
(config / "warehouse-cosmic/window-state.json").write_text(json.dumps({"width": 1180, "height": 760, "mode": "windowed"}))
records, captures, checks, owned = [], {}, [], []


def run(name, args, expected=0, parse=True):
    result = subprocess.run(args, cwd=root, capture_output=True, text=True)
    log = output / f"{len(records) + 1:03d}-{name}.log"
    log.write_text(result.stdout + result.stderr)
    records.append({"name": name, "command": args, "exit_status": result.returncode, "expected_status": expected, "log": str(log)})
    (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    assert result.returncode == expected, f"{name}: exit {result.returncode}; see {log}"
    return json.loads(result.stdout) if parse and result.stdout.strip() else None


def ui(instance, name, *args, expected=0):
    return run(name, client + ["ui"] + (["--instance", instance] if instance else []) + list(args), expected)


def state(instance):
    return ui(instance, "inspect", "inspect")["data"]["state"]


def tree(instance, name):
    return ui(instance, name, "tree")["data"]["tree"]


def texts(nodes):
    return [node.get("text") or "" for node in nodes]


def check(name, condition, detail=""):
    checks.append({"check": name, "passed": bool(condition), "detail": detail})
    assert condition, f"{name}: {detail}"


def capture(instance, name):
    path = output / f"{name}.png"
    ui(instance, f"capture-{name}", "capture", str(path))
    with Image.open(path) as image:
        captures[name] = {"path": str(path), "size": image.size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    return path


def wait_for(instance, name, predicate, timeout=90):
    end = time.time() + timeout
    while time.time() < end:
        current = state(instance)
        if predicate(current):
            return current
        time.sleep(0.5)
    raise AssertionError(f"{name} did not happen within {timeout}s")


def controls(current):
    return {c["id"]: c for c in current["controls"]}


def host_state():
    data = run("host-state", base + ["inspect"])["results"][0]["data"]
    # This app's own folder grows by the test window's log under its cache;
    # compare its presence, not its size.
    for folder in data["data"]:
        if folder["id"] == app:
            folder["bytes"] = None
    return {key: data[key] for key in ("packages", "remotes", "data", "snapshots")}


before = host_state()
try:
    started = ui(None, "start-isolated-gui", "start", "--config-dir", str(config))
    instance = started["data"]["instance"]
    owned.append(instance)
    ui(instance, "wait-initial", "wait", "--timeout", "60")
    first = state(instance)
    check("inspect lists labelled controls with requirements", all({"id", "label", "requires"} <= set(c) for c in first["controls"]))
    check("inspect lists fillable fields", {"search", "install.query", "snapshot.name"} <= {f["id"] for f in first["fields"]})

    # WC-001: compact data filters keep the accent background and checkmark.
    ui(instance, "nav-data", "navigate", "data")
    for theme in ("dark", "light"):
        ui(instance, f"theme-{theme}", "theme", theme)
        for choice in ("active", "leftover"):
            ui(instance, f"data-{choice}-{theme}", "activate", f"data.{choice}")
            capture(instance, f"data-{choice}-{theme}")
    nodes = tree(instance, "data-tree")
    bounds = {n["id"]: n["bounds"] for n in nodes if n.get("id") in ("data.active", "data.leftover")}
    active, leftover = bounds["data.active"], bounds["data.leftover"]
    check("data filters are sized to their labels", active[2] < 160 and leftover[2] < 160 and active[2] < leftover[2], str(bounds))
    check("data filters share one height", abs(active[3] - leftover[3]) < 0.5, str(bounds))
    check("data sort controls are present", {"data.sort:name", "data.sort:size", "data.sort-direction"} <= set(controls(state(instance))))
    ui(instance, "data-sort-size", "activate", "data.sort:size")
    ui(instance, "data-sort-desc", "activate", "data.sort-direction")
    sizes = [f["bytes"] for f in state(instance)["view"]["folders"]]
    check("size sort orders folders largest first", sizes == sorted(sizes, reverse=True), str(sizes[:5]))
    ui(instance, "theme-dark-restore", "theme", "dark")

    # WC-003: Settings theme choices are separate radio rows, not a dropdown.
    ui(instance, "open-settings", "activate", "header.settings")
    current = state(instance)
    check("theme choices are activatable rows", {"settings.theme:system", "settings.theme:dark", "settings.theme:light"} <= set(controls(current)))
    rows = [n for n in tree(instance, "settings-tree") if n.get("text") in ("Match desktop", "Dark", "Light")]
    ys = sorted(n["bounds"][1] for n in rows)
    check("theme rows are visible and do not overlap", len(ys) == 3 and all(b - a >= 24 for a, b in zip(ys, ys[1:])), str(ys))
    capture(instance, "settings-theme-rows")
    ui(instance, "theme-light-row", "activate", "settings.theme:light")
    check("theme row applies", state(instance)["theme"] == "light")
    ui(instance, "theme-dark-row", "activate", "settings.theme:dark")
    ui(instance, "close-settings", "activate", "settings.close")

    # Packages: filters panel, uninstall confirmation (WC-004), version history.
    ui(instance, "nav-packages", "navigate", "packages")
    ui(instance, "wait-packages", "wait")
    current = state(instance)
    package = current["view"]["selected"]["package"]
    ui(instance, "open-filters", "activate", "packages.filters")
    ui(instance, "filter-runtimes", "activate", "filter.runtimes")
    check("runtime filter shows runtimes", state(instance)["view"]["filters"]["runtimes"] is True)
    capture(instance, "packages-filters")
    ui(instance, "reset-filters", "activate", "filters.reset")
    check("reset restores default filters", state(instance)["view"]["filters"]["runtimes"] is False)
    ui(instance, "close-filters", "activate", "packages.filters")
    ui(instance, "open-uninstall", "activate", "package.uninstall")
    current = state(instance)
    check("uninstall dialog keeps data by default", current["modal"] == "uninstall" and current["dialog"]["app_data"] == "keep", json.dumps(current["dialog"]))
    words = texts(tree(instance, "uninstall-tree"))
    check("uninstall dialog names the app and offers data choices", f"Uninstall {package['name']}?" in words and "App Settings & Content" in words and "Keep" in words and "Trash" in words, str(words[:30]))
    capture(instance, "uninstall-keep")
    ui(instance, "uninstall-trash-choice", "activate", "uninstall.trash")
    check("trash choice is reflected", state(instance)["dialog"]["app_data"] == "trash")
    capture(instance, "uninstall-trash")
    ui(instance, "uninstall-refused-without-apply", "activate", "dialog.confirm", expected=1)
    ui(instance, "uninstall-cancel", "activate", "dialog.cancel")
    check("cancel leaves no dialog or operation", state(instance)["modal"] is None and state(instance)["busy"] is False)
    if package["origin"] and package["installation"] == "user":
        ui(instance, "open-version", "activate", "package.change-version")
        current = wait_for(instance, "version history", lambda s: s["dialog"] and s["dialog"]["history"] != "loading")
        history = current["dialog"]["history"]
        check("version history is a selectable list", isinstance(history, list) or "error" in history, str(history)[:200])
        capture(instance, "version-history")
        if isinstance(history, list) and history:
            ui(instance, "choose-commit", "activate", f"version.commit:{history[-1]['commit']}")
            check("choosing a commit fills the field", state(instance)["dialog"]["commit"] == history[-1]["commit"])
        ui(instance, "version-cancel", "activate", "dialog.cancel")

    # WC-005: Install Packages sections, per-remote search and pending queue.
    ui(instance, "nav-install", "navigate", "install")
    words = texts(tree(instance, "install-tree"))
    check("install page has remote, file and pending sections", all(w in words for w in ("Search in a Remote", "Add Packages", "Pending Packages", "No Pending Packages")), str(words[:40]))
    check("remote rows use readable titles", not any(w.startswith("- (") for w in words))
    capture(instance, "install-empty")
    current = state(instance)
    remote = next((r for r in current["view"]["remotes"] if r["name"] == "flathub"), None)
    if remote:
        ui(instance, "open-remote-search", "activate", f"install.remote:{remote['installation']}:flathub")
        ui(instance, "fill-install-query", "fill", "install.query", "flatseal")
        ui(instance, "search-remote", "activate", "install.search")
        current = wait_for(instance, "remote search", lambda s: s["view"]["searched"] and not s["view"]["searching"])
        results = current["view"]["results"]
        check("remote search returns results", len(results) > 0, str(results[:3]))
        capture(instance, "install-search")
        candidate = next((r for r in results if not r["installed"]), None)
        installed = next((r for r in results if r["installed"]), None)
        if installed:
            check("installed results cannot be queued", f"install.result:{installed['ref']}" not in controls(current))
        if candidate:
            ui(instance, "queue-result", "activate", f"install.result:{candidate['ref']}")
            ui(instance, "back-to-install", "activate", "install.back")
            current = state(instance)
            check("queued package keeps remote and installation", current["view"]["queue"] == [{"installation": remote["installation"], "remote": "flathub", "ref": candidate["ref"], "name": candidate["name"]}], json.dumps(current["view"]["queue"]))
            capture(instance, "install-pending")
            ui(instance, "review-queue", "activate", "install.queue.review")
            current = state(instance)
            check("queue review shows the exact command", current["modal"] == "confirmation" and candidate["ref"] in current["dialog"]["commands"], json.dumps(current["dialog"]))
            capture(instance, "install-queue-confirm")
            ui(instance, "queue-refused-without-apply", "activate", "dialog.confirm", expected=1)
            ui(instance, "queue-cancel", "activate", "dialog.cancel")
            ui(instance, "queue-clear", "activate", "install.queue.clear")
            check("Remove All empties the queue", state(instance)["view"]["queue"] == [])

    # WC-002: the drop handler, with spaces in the name, reaches install review.
    bundle = output / "Review Only Package.flatpak"
    built = root / "flatpak/out-next" / f"{app}.flatpak"
    shutil.copy2(built if built.exists() else root / "flatpak/out" / f"{app}.flatpak", bundle)
    ui(instance, "drop-bundle", "drop", str(bundle))
    current = state(instance)
    check("dropping a bundle opens review without installing", current["modal"] == "install-files" and current["install_files"] == [str(bundle)] and current["busy"] is False, json.dumps(current["install_files"]))
    capture(instance, "drop-review")
    ui(instance, "drop-cancel", "activate", "dialog.cancel")
    note = output / "notes.txt"
    note.write_text("not a package\n")
    ui(instance, "drop-unsupported", "drop", str(note))
    check("unsupported drops explain the allowed types", state(instance)["dialog"]["title"] == "Unsupported File Type")
    ui(instance, "drop-unsupported-close", "activate", "dialog.cancel")
    repo = output / "example.flatpakrepo"
    repo.write_text("[Flatpak Repo]\nUrl=https://example.invalid/repo\n")
    ui(instance, "drop-mixed", "drop", str(bundle), str(repo))
    check("mixed drops are refused", state(instance)["dialog"]["title"] == "Mixed File Types")
    ui(instance, "drop-mixed-close", "activate", "dialog.cancel")

    # WC-006: Snapshots empty state / grouped list and the visual creation dialog.
    ui(instance, "nav-snapshots", "navigate", "snapshots")
    current = state(instance)
    words = texts(tree(instance, "snapshots-tree"))
    if not current["view"]["groups"]:
        check("empty snapshots page centers New Snapshot", "No Snapshots" in words and "snapshots.new" in controls(current) and "packages.select" not in controls(current))
    capture(instance, "snapshots")
    ui(instance, "new-snapshot", "activate", "snapshots.new")
    current = state(instance)
    apps = current["dialog"]["apps"]
    check("creation dialog lists apps with names and IDs", current["modal"] == "new-snapshot" and all({"id", "name"} <= set(a) for a in apps), str(apps[:3]))
    capture(instance, "new-snapshot-dialog")
    if apps:
        term = apps[0]["id"].split(".")[-1]
        ui(instance, "snapshot-search", "fill", "snapshot.search", term)
        ui(instance, "snapshot-select-all", "activate", "snapshot.select-all")
        current = state(instance)
        check("select all picks the visible apps", set(current["dialog"]["selected"]) == {a["id"] for a in current["dialog"]["apps"]})
        ui(instance, "snapshot-refused-without-apply", "activate", "snapshot.create", expected=1)
        ui(instance, "snapshot-clear-name", "fill", "snapshot.name", "")
        check("Create needs a name", "snapshot.create" not in controls(state(instance)))
    ui(instance, "snapshot-cancel", "activate", "dialog.cancel")

    # Narrow layouts. Live resizing depends on the compositor (a tiling COSMIC
    # session may refuse it), so fresh capture processes start at the minimum
    # width (window_state::MIN_WIDTH).
    gui = [str(pathlib.Path(client[0]).with_name("warehouse-cosmic"))] if client is not base else ["flatpak", "run", app]
    wide = captures["data-active-dark"]["size"][0]
    for page in ("install", "data", "snapshots", "packages"):
        path = output / f"narrow-{page}.png"
        run(f"narrow-{page}", gui + ["--preview", f"{page}-narrow", "--capture", str(path)], parse=False)
        with Image.open(path) as image:
            captures[f"narrow-{page}"] = {"path": str(path), "size": image.size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        check(f"narrow {page} render is narrower than the normal window", captures[f"narrow-{page}"]["size"][0] < wide, str(captures[f"narrow-{page}"]["size"]))

    after = host_state()
    check("host packages, remotes, data and snapshots are unchanged", before == after)
finally:
    for instance in owned:
        if instance in (ui(None, "cleanup-instances", "instances")["data"]["instances"]):
            ui(instance, "close-own-window", "close", "--apply")
    deadline = time.time() + 10
    while time.time() < deadline and set(owned) & set(ui(None, "after-close", "instances")["data"]["instances"]):
        time.sleep(0.2)
commit = subprocess.check_output(["flatpak", "info", "--user", "--show-commit", app], text=True).strip()
report = {"passed": all(c["passed"] for c in checks), "installed_commit": commit, "client": client, "output": str(output), "checks": checks, "captures": captures,
          "all_test_windows_closed": not (set(owned) & set(ui(None, "final-instances", "instances")["data"]["instances"])),
          "scope": "Semantic GUI handlers, rendered layout bounds and captures in the installed Flatpak. No confirmation was applied. A real compositor drag from a file manager and native pointer hover are not exercised by this script."}
(output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"passed": report["passed"], "checks": len(checks), "output": str(output)}))
