#!/usr/bin/env -S uv run --python 3.15
"""Open files through the installed desktop launcher and retain one window.

Refuses to start if the normal app already owns its desktop bus name. Only the
private-config GUI created here is closed. No installation is confirmed.
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time

root = pathlib.Path(__file__).resolve().parents[1]
app = "dev.nilltadios.WarehouseCosmic"
area = root / "verification/flatpak-packaging"
task = pathlib.Path.home() / "Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic"
output = pathlib.Path(tempfile.mkdtemp(prefix="single-instance-", dir=task))
config = output / "config"
(config / "warehouse-cosmic").mkdir(parents=True)
(config / "cosmic").symlink_to(pathlib.Path.home() / ".config/cosmic", target_is_directory=True)
(config / "warehouse-cosmic/window-state.json").write_text(json.dumps({"width":1100,"height":700,"mode":"windowed"}))
instance = app + ".UI.test" + str(time.time_ns())
base = ["flatpak","run","--command=warehouse-cosmic-cli",app]
records = []
process = None
monitor = None
owned = False

def run(name, args):
    result = subprocess.run(args, cwd=root, text=True, capture_output=True, timeout=40)
    log = output / (str(len(records)) + "-" + name + ".log")
    log.write_text(result.stdout + result.stderr)
    records.append({"name":name,"command":args,"exit_status":result.returncode,"log":str(log)})
    (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    assert result.returncode == 0, f"{name} failed; see {log}"
    return result.stdout

def ui(name, *args):
    return json.loads(run(name,base + ["ui","--instance",instance] + list(args)))

def names():
    return json.loads(run("instances",base + ["ui","instances"]))["data"]["instances"]

def inspect(): return ui("inspect","inspect")["data"]["state"]

def windows():
    listing = json.loads(run("windows",["busctl","--user","--json=short","call","org.kde.KWin","/WindowsRunner","org.kde.krunner1","Match","s","Warehouse COSMIC"]))
    result = {}
    for entry in listing["data"][0]:
        identity = entry[0].removeprefix("0_")
        text = run("window-info",["qdbus-qt6","org.kde.KWin","/KWin","org.kde.KWin.getWindowInfo",identity])
        data = dict(line.split(": ",1) for line in text.splitlines() if ": " in line)
        if data.get("resourceClass") == app:
            result[identity] = data
    return result

def host_state(name):
    value = json.loads(run(name,base + ["inspect"]))["results"][0]["data"]
    return {key:value[key] for key in ("packages","remotes")}

primary = json.loads(run("primary-owner",["busctl","--user","--json=short","call","org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus","NameHasOwner","s",app]))
assert primary["data"] == [False], "Normal app is already open. Leave it untouched and rerun after it is closed."
before = host_state("host-before")
existing = names()
initial_windows = windows()
bundle = output / "Warehouse-COSMIC-review-only.flatpak"
shutil.copy2(root / "flatpak/out" / (app + ".flatpak"),bundle)
desktop = pathlib.Path.home() / ".local/share/flatpak/exports/share/applications" / (app + ".desktop")
try:
    command = ["on-screen.sh","DP-1",app,"--","flatpak","run","--env=WAREHOUSE_COSMIC_UI_ISOLATED=0","--env=WAREHOUSE_COSMIC_UI_INSTANCE="+instance,"--command=env",app,"XDG_CONFIG_HOME="+str(config),"/app/bin/warehouse-cosmic"]
    with (output / "primary.log").open("w") as log:
        process = subprocess.Popen(command,cwd=root,stdout=log,stderr=subprocess.STDOUT)
    records.append({"launch_command":command,"pid":process.pid})
    for attempt in range(100):
        if instance in names():
            owned = True
            break
        assert process.poll() is None, "Primary exited before its GUI interface appeared"
        time.sleep(.1)
    assert owned, "No test GUI appeared"
    with (output / "activation-messages.log").open("w") as log:
        monitor = subprocess.Popen(["dbus-monitor","--session","type='method_call',interface='org.freedesktop.DbusActivation',destination='"+app+"'"],stdout=log,stderr=subprocess.STDOUT)
    ui("wait","wait")
    time.sleep(.4)
    opened = windows()
    added = set(opened) - set(initial_windows)
    assert len(added) == 1, opened
    identity = added.pop()
    environment = (pathlib.Path("/proc") / opened[identity]["pid"] / "environ").read_bytes()
    assert ("XDG_CONFIG_HOME=" + str(config)).encode() in environment.split(b"\0")
    script = output / "minimize-own-window.js"
    script.write_text("for (const w of workspace.windowList()) { if (String(w.internalId) === " + json.dumps(identity) + ") w.minimized = true; }\n")
    script_name = "warehouse-single-instance-" + output.name
    run("load-minimize",["qdbus-qt6","org.kde.KWin","/Scripting","org.kde.kwin.Scripting.loadScript",str(script),script_name])
    try:
        run("minimize",["qdbus-qt6","org.kde.KWin","/Scripting","org.kde.kwin.Scripting.start"])
        time.sleep(.2)
        assert windows()[identity]["minimized"] == "true"
    finally:
        run("unload-minimize",["qdbus-qt6","org.kde.KWin","/Scripting","org.kde.kwin.Scripting.unloadScript",script_name])
    for number in range(2):
        run("desktop-file-open-"+str(number),["gio","launch",str(desktop),str(bundle)])
        for attempt in range(100):
            state = inspect()
            if state["modal"] == "install-files": break
            time.sleep(.1)
        assert state["modal"] == "install-files" and state["busy"] is False, state
        assert state["install_files"] == [str(bundle)], state
        assert set(names()) == set(existing) | {instance}, "File activation created another GUI instance"
        current = windows()
        assert set(current) == set(opened), "File activation created another window"
        for attempt in range(50):
            current = windows()
            if current[identity]["minimized"] == "false": break
            time.sleep(.1)
        if os.environ.get("WAREHOUSE_TEST_KWIN_PRESENT_PROBE") == "1" and current[identity]["minimized"] == "true":
            run("probe-native-presentation",["qdbus-qt6","org.kde.KWin","/WindowsRunner","org.kde.krunner1.Run","0_"+identity,""])
            time.sleep(.2)
            current = windows()
        assert current[identity]["minimized"] == "false", "Existing window was not presented"
    reused = json.loads(run("cli-start-reuses-primary",base + ["ui","start"]))
    assert reused["data"]["instance"] == instance and reused["data"]["reused"] is True
    ui("capture-review","capture",str(output / "same-window-review.png"))
    assert before == host_state("host-after")
    ui("close-own-primary","close","--apply")
    process.wait(timeout=20)
    assert process.returncode == 0
    owned = False
    for attempt in range(50):
        if instance not in names(): break
        time.sleep(.1)
    assert set(names()) == set(existing)
    report = {"passed":True,"installed_commit":run("commit",["flatpak","info","--user","--show-commit",app]).strip(),"output":str(output),"window_identity":identity,"gui_instance":instance,"desktop_file_opens":2,"minimized_window_presented":True,"cli_start_reused_primary":True,"host_state_unchanged":True,"all_test_windows_closed":True,"bundle_sha256":hashlib.sha256(bundle.read_bytes()).hexdigest(),"commands":records}
    probe = os.environ.get("WAREHOUSE_TEST_KWIN_PRESENT_PROBE") == "1"
    report["external_presentation_probe"] = probe
    report_name = "single-instance-helper-probe-results.json" if probe else "single-instance-results.json"
    (area / report_name).write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"passed":True,"output":str(output)}))
finally:
    if monitor:
        monitor.terminate()
        monitor.wait(timeout=5)
    if owned and instance in names():
        ui("cleanup-own-primary","close","--apply")
        process.wait(timeout=20)
