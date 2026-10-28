#!/usr/bin/env -S uv run --python 3.15
"""Exercise real Flatpak close/reopen cycles with private window preferences.

Only windows whose process has this fixture's XDG_CONFIG_HOME are changed or
closed. The user's app preferences and pre-existing windows are preserved.
"""
import json
import os
import pathlib
import subprocess
import tempfile
import time

root = pathlib.Path(__file__).resolve().parents[1]
app = "dev.nilltadios.WarehouseCosmic"
area = root / "verification/flatpak-packaging"
task = pathlib.Path.home() / "Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic"
output = pathlib.Path(tempfile.mkdtemp(prefix="window-state-", dir=task))
config = output / "config"
(config / "warehouse-cosmic").mkdir(parents=True)
(config / "cosmic").symlink_to(pathlib.Path.home() / ".config/cosmic", target_is_directory=True)
state_path = config / "warehouse-cosmic/window-state.json"
state_path.write_text(json.dumps({"width":1100, "height":700, "mode":"windowed"}))
actual = pathlib.Path.home() / (".var/app/" + app + "/config/warehouse-cosmic/window-state.json")
before = actual.read_bytes() if actual.exists() else None
records = []
observations = []
counter = 0
current = None
process = None

def run(args):
    result = subprocess.run(args, text=True, capture_output=True)
    records.append({"command":args, "exit_status":result.returncode, "stdout":result.stdout, "stderr":result.stderr})
    (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    assert result.returncode == 0, f"Command failed; see {output}"
    return result.stdout

def info(identity):
    text = run(["qdbus-qt6", "org.kde.KWin", "/KWin", "org.kde.KWin.getWindowInfo", identity])
    return dict(line.split(": ", 1) for line in text.splitlines() if ": " in line)

def ours(data):
    try:
        environment = pathlib.Path("/proc") / data["pid"] / "environ"
        return ("XDG_CONFIG_HOME=" + str(config)).encode() in environment.read_bytes().split(b"\0")
    except (KeyError, OSError):
        return False

def find_window():
    listing = json.loads(run(["busctl", "--user", "--json=short", "call", "org.kde.KWin", "/WindowsRunner", "org.kde.krunner1", "Match", "s", "Warehouse COSMIC"]))
    for entry in listing["data"][0]:
        identity = entry[0].removeprefix("0_")
        data = info(identity)
        if data.get("resourceClass") == app and ours(data):
            return identity
    return None

def mode(data):
    if data.get("fullscreen") == "true": return "fullscreen"
    if int(data.get("maximizeHorizontal", "0")) and int(data.get("maximizeVertical", "0")): return "maximized"
    return "windowed"

def wait_mode(identity, expected):
    for attempt in range(100):
        data = info(identity)
        if mode(data) == expected: return data
        time.sleep(0.1)
    raise AssertionError(f"Window did not restore {expected}; see {output}")

def action(identity, body):
    global counter
    assert ours(info(identity)), "Refusing to change a window outside this fixture"
    counter += 1
    name = "warehouse-window-state-" + output.name + str(counter)
    script = output / (str(counter) + ".js")
    script.write_text("for (const w of workspace.windowList()) { if (String(w.internalId) === " + json.dumps(identity) + " && String(w.resourceClass) === " + json.dumps(app) + ") { " + body + " } }\n")
    run(["qdbus-qt6", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", str(script), name])
    try:
        run(["qdbus-qt6", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
        time.sleep(0.1)
    finally:
        run(["qdbus-qt6", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", name])

def launch(number):
    global process, current
    command = ["on-screen.sh", "DP-1", app, "--", "flatpak", "run", "--env=WAREHOUSE_COSMIC_UI_ISOLATED=1", "--command=env", app, "XDG_CONFIG_HOME=" + str(config), "/app/bin/warehouse-cosmic", "--preview", "window-state-test"]
    log = (output / ("launch-" + str(number) + ".log")).open("w")
    process = subprocess.Popen(command, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    log.close()
    records.append({"launch_command":command, "launcher_pid":process.pid})
    for attempt in range(100):
        current = find_window()
        if current: return current
        assert process.poll() is None, f"App exited before opening; see {output}"
        time.sleep(0.1)
    raise AssertionError("No fixture window appeared")

def close():
    global current, process
    action(current, "w.closeWindow();")
    process.wait(timeout=20)
    assert process.returncode == 0
    current = None
    process = None

try:
    expected = "windowed"
    for number, target in enumerate(("windowed", "maximized", "fullscreen", "windowed")):
        identity = launch(number)
        restored = wait_mode(identity, expected)
        body = "w.fullScreen = false; w.setMaximize(false, false);"
        if target == "maximized": body = "w.fullScreen = false; w.setMaximize(true, true);"
        if target == "fullscreen": body = "w.fullScreen = true;"
        if target == "windowed": body += "const g = w.frameGeometry; w.frameGeometry = {x:g.x,y:g.y,width:1100,height:700};"
        action(identity, body)
        changed = wait_mode(identity, target)
        time.sleep(0.7)  # Allow the app's native window-state query to finish.
        close()
        saved = json.loads(state_path.read_text())
        assert saved["mode"] == target, f"Saved mode is {saved}; expected {target}"
        assert abs(saved["width"] - 1100) < 8 and abs(saved["height"] - 700) < 8, saved
        observations.append({"restored_mode":expected, "restored_window":restored, "changed_window":changed, "saved_state":saved})
        expected = target
    identity = launch("final")
    final = wait_mode(identity, "windowed")
    assert abs(float(final["width"]) - 1100) < 8 and abs(float(final["height"]) - 700) < 8
    close()
    assert (actual.read_bytes() if actual.exists() else None) == before, "Real window preferences changed"
    commit = run(["flatpak", "info", "--user", "--show-commit", app]).strip()
    report = {"passed":True, "installed_commit":commit, "output":str(output), "cycles":observations, "real_preferences_unchanged":True, "all_test_windows_closed":True}
    (area / "window-state-results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed":True, "output":str(output)}))
finally:
    if current and process and process.poll() is None and ours(info(current)):
        close()
