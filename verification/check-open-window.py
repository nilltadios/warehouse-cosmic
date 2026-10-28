#!/usr/bin/env -S uv run --python 3.15
"""Read-only delivery check of the open normal Flatpak and KWin window."""
import json
import configparser
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[1]
area = root / "verification/flatpak-packaging"
app = "dev.nilltadios.WarehouseCosmic"
records = []
def run(args):
    result = subprocess.run(args,cwd=root,text=True,capture_output=True,timeout=30)
    records.append({"command":args,"exit_status":result.returncode,"stdout":result.stdout,"stderr":result.stderr})
    (area / "open-window-commands.json").write_text(json.dumps(records,indent=2)+"\n")
    assert result.returncode == 0, result.stderr
    return result.stdout
state = json.loads(run(["flatpak","run","--command=warehouse-cosmic-cli",app,"ui","inspect"]))["data"]["state"]
commit = run(["flatpak","info","--user","--show-commit",app]).strip()
running = run(["flatpak","ps","--columns=instance,application,commit"])
assert any(len(fields := line.split()) == 3 and fields[:2] == [state["flatpak_instance"],app] and commit.startswith(fields[2]) for line in running.splitlines())
matches = json.loads(run(["busctl","--user","--json=short","call","org.kde.KWin","/WindowsRunner","org.kde.krunner1","Match","s","Warehouse COSMIC"]))["data"][0]
windows = []
for match in matches:
    identity = match[0].removeprefix("0_")
    text = run(["qdbus-qt6","org.kde.KWin","/KWin","org.kde.KWin.getWindowInfo",identity])
    data = dict(line.split(": ",1) for line in text.splitlines() if ": " in line)
    if data.get("resourceClass") == app:
        windows.append({"identity":identity,"kwin":data})
assert len(windows) == 1, windows
data = windows[0]["kwin"]
metadata = configparser.ConfigParser(interpolation=None)
metadata.read(pathlib.Path("/proc") / data["pid"] / "root/.flatpak-info")
assert metadata["Instance"]["app-commit"] == commit
assert metadata["Instance"]["instance-id"] == state["flatpak_instance"]
assert data["minimized"] == "false"
assert int(data["maximizeHorizontal"]) and int(data["maximizeVertical"]), data
report = {"passed":True,"installed_commit":commit,"instance":state["instance"],"page":state["page"],"data_filter":state["data_filter"],"window":windows[0],"commands":str(area / "open-window-commands.json")}
(area / "open-window-results.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps({"passed":True,"page":state["page"],"mode":"maximized","output":str(area / "open-window-results.json")}))
