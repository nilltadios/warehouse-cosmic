#!/usr/bin/env -S uv run --python 3.15
"""Launch the installed desktop entry, read KWin's identity/icon, and close only that new window."""
import json, pathlib, subprocess, time
from PIL import Image
root = pathlib.Path(__file__).resolve().parents[1]
out = root / 'verification' / 'launcher'
out.mkdir(exist_ok=True)
app_id = 'dev.nilltadios.WarehouseCosmic'
def command(*args):
    return subprocess.check_output(args, text=True)
def matches():
    data = json.loads(command('busctl', '--user', '--json=short', 'call', 'org.kde.KWin', '/WindowsRunner', 'org.kde.krunner1', 'Match', 's', 'Warehouse COSMIC'))
    return [entry for entry in data['data'][0] if entry[1] == 'Warehouse COSMIC']
before = {entry[0] for entry in matches()}
subprocess.run(['gtk-launch', app_id], check=True)
window = None
try:
    for attempt in range(100):
        current = [entry for entry in matches() if entry[0] not in before]
        if current:
            window = current[0]
            break
        time.sleep(0.1)
    if window is None:
        raise RuntimeError('No new installed application window appeared')
    (out / 'window-runner.json').write_text(json.dumps(window) + '\n')
    uuid = window[0].removeprefix('0_')
    info = command('qdbus-qt6', 'org.kde.KWin', '/KWin', 'org.kde.KWin.getWindowInfo', uuid)
    (out / 'window-info.txt').write_text(info)
    if f'resourceClass: {app_id}' not in info or f'desktopFile: {app_id}' not in info:
        raise RuntimeError('Installed window identity does not match its desktop entry')
    if window[2] == app_id:
        # KWin returns a theme icon name when the desktop entry resolves normally.
        icon = pathlib.Path.home() / '.local/share/icons/hicolor/scalable/apps' / (app_id + '.svg')
        expected = root / 'resources' / (app_id + '.svg')
        if icon.read_bytes() != expected.read_bytes():
            raise RuntimeError('Installed named icon differs from the project icon')
        subprocess.run(['inkscape', str(icon), '--export-filename=' + str(out / 'window-icon.png'), '--export-width=256', '--export-height=256'], check=True)
        with Image.open(out / 'window-icon.png') as image:
            width, height = image.size
        (out / 'icon-resolution.txt').write_text(f'KWin icon name: {window[2]}\nInstalled artwork: {icon}\n')
    else:
        variant = window[5]['icon-data']
        data = variant.get('data', variant.get('value'))
        width, height, stride, alpha, bits, channels, pixels = data
        if bits != 8 or channels != 4:
            raise RuntimeError(f'Unexpected icon format: {data[:6]}')
        image = Image.frombytes('RGBA', (width, height), bytes(v % 256 for v in pixels), 'raw', 'RGBA', stride)
        image.save(out / 'window-icon.png')
    print(f'PASS: installed desktop entry launched; KWin app identity and {width}x{height} icon recorded')
finally:
    if window:
        uuid = window[0].removeprefix('0_')
        script = out / 'close-probe.js'
        script.write_text('for (const w of workspace.windowList()) { if (String(w.internalId) === ' + json.dumps(uuid) + ' && String(w.resourceClass) === ' + json.dumps(app_id) + ') w.closeWindow(); }\n')
        name = 'warehouse-launcher-probe-close'
        subprocess.run(['qdbus-qt6', 'org.kde.KWin', '/Scripting', 'org.kde.kwin.Scripting.loadScript', str(script), name], check=True, stdout=subprocess.DEVNULL)
        subprocess.run(['qdbus-qt6', 'org.kde.KWin', '/Scripting', 'org.kde.kwin.Scripting.start'], check=True, stdout=subprocess.DEVNULL)
        time.sleep(0.2)
        subprocess.run(['qdbus-qt6', 'org.kde.KWin', '/Scripting', 'org.kde.kwin.Scripting.unloadScript', name], check=True, stdout=subprocess.DEVNULL)
