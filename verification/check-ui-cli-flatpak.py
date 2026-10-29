#!/usr/bin/env -S uv run --python 3.15
"""Drive the installed GUI through its CLI and save all requests and renders.

No package/data confirmation is applied. Real hover input must be delivered.
Only instances returned by this script's explicit starts are closed.
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
from PIL import Image, ImageStat

root = pathlib.Path(__file__).resolve().parents[1]
area = root / "verification/flatpak-packaging"
app = "dev.nilltadios.WarehouseCosmic"
base = ["flatpak", "run", "--command=warehouse-cosmic-cli", app]
client_path = os.environ.get("WAREHOUSE_TEST_UI_CLIENT")
client = [client_path] if client_path else base
helper = "on-screen.sh"
task = pathlib.Path.home() / "Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic"
output = pathlib.Path(tempfile.mkdtemp(prefix="ui-cli-", dir=task))
config = output / "config"
(config / "warehouse-cosmic").mkdir(parents=True)
(config / "cosmic").symlink_to(pathlib.Path.home() / ".config/cosmic", target_is_directory=True)
(config / "warehouse-cosmic/window-state.json").write_text(json.dumps({"width":1100,"height":700,"mode":"windowed"}))
bundle = output / "Warehouse-COSMIC-review-only.flatpak"
shutil.copy2(root / "flatpak/out" / (app + ".flatpak"), bundle)
records = []
captures = {}
owned = []
counter = 0

def run(name, args, expected=0):
    global counter
    counter += 1
    result = subprocess.run(args, cwd=root, capture_output=True, text=True)
    log = output / (f"{counter:03d}-" + name + ".log")
    log.write_text(result.stdout + result.stderr)
    record = {"name":name, "command":args, "exit_status":result.returncode, "expected_status":expected, "log":str(log)}
    records.append(record)
    (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    assert result.returncode == expected, f"{name}: exit {result.returncode}; see {log}"
    return json.loads(result.stdout)

def ui(instance, name, *args, expected=0):
    command = client + ["ui"] + (["--instance",instance] if instance else []) + list(args)
    return run(name, command, expected)

def state(instance): return ui(instance, "inspect", "inspect")["data"]["state"]

def start(name):
    result = run(name, [helper,"DP-1",app,"--"] + base + ["ui","start","--config-dir",str(config)])
    instance = result["data"]["instance"]
    owned.append(instance)
    ui(instance,"wait","wait")
    return instance

def capture(instance, name):
    path = output / (name + ".png")
    ui(instance,name,"capture",str(path))
    with Image.open(path) as image: size = image.size
    captures[name] = {"path":str(path), "size":size, "sha256":hashlib.sha256(path.read_bytes()).hexdigest()}
    return path

def close(instance):
    ui(instance,"close-own-window","close","--apply")
    for attempt in range(100):
        names = ui(None,"instances-after-close","instances")["data"]["instances"]
        if instance not in names: return
        time.sleep(0.1)
    raise AssertionError("The test GUI did not close")

def backend_state(name):
    value = run(name, base + ["inspect"])["results"][0]["data"]
    return {key:value[key] for key in ("packages","remotes")}

before = backend_state("host-before")
existing = ui(None,"existing-instances","instances")["data"]["instances"]
try:
    instance = start("start-private-gui")
    tree = ui(instance,"header-tree","tree")["data"]["tree"]
    positions = {node["id"]:node["bounds"][0] for node in tree if node.get("id") in ("header.logo","header.sidebar","header.refresh")}
    assert positions["header.logo"] < positions["header.sidebar"] < positions["header.refresh"], positions
    sidebar = state(instance)["sidebar_visible"]
    ui(instance,"hide-sidebar","activate","header.sidebar")
    assert state(instance)["sidebar_visible"] is not sidebar
    ui(instance,"restore-sidebar","activate","header.sidebar")
    assert state(instance)["sidebar_visible"] is sidebar
    for target in ("header.logo","header.sidebar","header.refresh","header.settings","header.about"):
        name = target.replace('.','-') + "-hover"
        path = output / (name + ".png")
        result = ui(instance,name,"hover",target,"--capture",str(path))
        assert result["data"]["native_input_delivered"] is True
        with Image.open(path) as image: size=image.size
        captures[name] = {"path":str(path),"size":size,"sha256":hashlib.sha256(path.read_bytes()).hexdigest()}
    # Package rows no longer carry widget IDs (see src/app.rs::packages_view),
    # so they cannot be hover targets; select them with `ui activate package:KEY`.
    ui(instance,"open-about","activate","header.about")
    assert state(instance)["modal"] == "about"
    about = ui(instance,"about-tree","tree")["data"]["tree"]
    about_text = "\n".join(node.get("text") or "" for node in about)
    assert "Manage Flatpak apps with a native graphical interface and CLI." in about_text
    assert "Inspired by flattool/Warehouse and built with Rust and libcosmic." in about_text
    capture(instance,"about")
    ui(instance,"close-about","activate","dialog.cancel")
    ui(instance,"open-settings","activate","header.settings")
    assert state(instance)["settings_open"] is True
    ui(instance,"theme-light","theme","light")
    light = capture(instance,"settings-light")
    ui(instance,"theme-dark","theme","dark")
    dark = capture(instance,"settings-dark")
    def brightness(path):
        with Image.open(path) as image: return sum(ImageStat.Stat(image.convert("RGB").crop((20,300,200,400))).mean)/3
    assert brightness(light) > brightness(dark) + 60, "Theme capture did not change palette"
    ui(instance,"close-settings","activate","header.settings")
    ui(instance,"user-data","navigate","data")
    ui(instance,"select-active-data","activate","data.active")
    assert state(instance)["data_filter"] == "active"
    active_data = capture(instance,"data-active")
    ui(instance,"select-leftover-data","activate","data.leftover")
    assert state(instance)["data_filter"] == "leftover"
    leftover_data = capture(instance,"data-leftover")
    data_tree = ui(instance,"data-filter-tree","tree")["data"]["tree"]
    data_bounds = {node["id"]:node["bounds"] for node in data_tree if node.get("id") in ("data.active","data.leftover")}
    logical_width = state(instance)["window"]["width"]
    def filter_color(path, bounds):
        with Image.open(path) as image:
            scale = image.width / logical_width
            x = (bounds[0]+bounds[2]/2)*scale
            y = (bounds[1]+bounds[3]-4)*scale
            return ImageStat.Stat(image.convert("RGB").crop((int(x)-2,int(y)-1,int(x)+2,int(y)+1))).mean
    for bounds in data_bounds.values():
        a,b = filter_color(active_data,bounds),filter_color(leftover_data,bounds)
        assert sum(abs(x-y) for x,y in zip(a,b)) > 80, "Data filter selection did not change its visible background"
    assert set(data_bounds) == {"data.active","data.leftover"}
    ui(instance,"data-theme-light","theme","light")
    capture(instance,"data-leftover-light")
    ui(instance,"data-active-light","activate","data.active")
    capture(instance,"data-active-light")
    ui(instance,"restore-dark-theme","theme","dark")
    ui(instance,"remotes","navigate","remotes")
    enabled = state(instance)["remotes"]
    assert all(not item["disabled"] for item in enabled)
    capture(instance,"remotes")
    ui(instance,"show-disabled","activate","remotes.show-disabled")
    assert state(instance)["show_disabled_remotes"] is True
    capture(instance,"remotes-disabled")
    ui(instance,"hide-disabled","activate","remotes.show-disabled")
    ui(instance,"remote-search","fill","search","flathub")
    assert all("flathub" in item["name"].lower() for item in state(instance)["remotes"])
    capture(instance,"remotes-search")
    ui(instance,"empty-search","fill","search","no-such-remote-fixture")
    assert state(instance)["remotes"] == []
    capture(instance,"remotes-empty")
    ui(instance,"clear-search","fill","search","")
    remote = next(item for item in enabled if item["name"] == "flathub")
    key = "remote:" + remote["installation"] + ":" + remote["name"]
    ui(instance,"open-remote-menu","activate",key+":menu")
    capture(instance,"remotes-menu")
    ui(instance,"remote-details","activate","remote.menu.details")
    assert state(instance)["modal"] == "remote-details"
    capture(instance,"remote-details")
    ui(instance,"dismiss-details","activate","dialog.cancel")
    ui(instance,"preset-review","activate","remotes.preset:appcenter")
    assert state(instance)["modal"] == "add-remote"
    capture(instance,"remote-preset")
    ui(instance,"dismiss-preset","activate","dialog.cancel")
    path = output / "remotes-scrolled.png"
    ui(instance,"scroll-remotes","scroll","--pixels","450","--capture",str(path))
    captures["remotes-scrolled"]={"path":str(path),"sha256":hashlib.sha256(path.read_bytes()).hexdigest()}
    ui(instance,"open-bundle-review","open",str(bundle))
    review = state(instance)
    assert review["modal"] == "install-files" and review["installation"] == "user" and review["busy"] is False
    capture(instance,"install-review")
    ui(instance,"deny-install-without-apply","activate","install.confirm",expected=1)
    ui(instance,"choose-system","activate","install.scope:system")
    ui(instance,"deny-system-without-permission","activate","install.confirm","--apply",expected=1)
    assert state(instance)["busy"] is False
    ui(instance,"dismiss-install","activate","dialog.cancel")
    ui(instance,"normal-size","window","--mode","windowed","--width","1100","--height","700")
    ui(instance,"maximize","window","--mode","maximized")
    capture(instance,"maximized")
    ui(instance,"fullscreen","window","--mode","fullscreen")
    capture(instance,"fullscreen")
    close(instance)
    saved = json.loads((config / "warehouse-cosmic/window-state.json").read_text())
    assert saved["mode"] == "fullscreen" and saved["width"] == 1100 and saved["height"] == 700
    instance = start("reopen-private-gui")
    assert state(instance)["window"]["mode"] == "fullscreen"
    ui(instance,"return-windowed","window","--mode","windowed","--width","1100","--height","700")
    capture(instance,"restored-windowed")
    close(instance)
    assert before == backend_state("host-after"), "GUI checks changed host package/remote state"
    final = ui(None,"final-instances","instances")["data"]["instances"]
    assert not set(final) & set(owned), "A test GUI remains open"
    commit = subprocess.check_output(["flatpak","info","--user","--show-commit",app],text=True).strip()
    report = {"passed":True,"installed_commit":commit,"output":str(output),"captures":captures,"commands":records,"host_state_unchanged":True,"all_test_windows_closed":True,
              "scope":"Live GUI semantic handlers, real native pointer hover/scroll, renderer captures, install safeguards and private window/theme persistence. Native chooser, physical keyboard and physical click hit testing were not automated."}
    report["client"] = client
    report_name = "ui-cli-native-probe-results.json" if client_path else "ui-cli-results.json"
    (area / report_name).write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"passed":True,"output":str(output)}))
finally:
    active = ui(None,"cleanup-instances","instances")["data"]["instances"]
    for instance in owned:
        if instance in active: close(instance)
