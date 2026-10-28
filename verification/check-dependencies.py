#!/usr/bin/env -S uv run --python 3.15
"""Record dependency provenance and compare direct crates with current stable releases."""
import datetime, json, pathlib, subprocess, sys, tomllib, urllib.request
root = pathlib.Path(__file__).resolve().parents[1]
out = root / 'verification' / 'dependency-sources'
out.mkdir(exist_ok=True)
def command(*args):
    return subprocess.check_output(args, cwd=root, text=True).strip()
def fetch(url, name):
    request = urllib.request.Request(url, headers={'User-Agent': 'warehouse-cosmic-dependency-check/1'})
    with urllib.request.urlopen(request, timeout=30) as response:
        body = response.read()
    (out / name).write_bytes(body)
    return body
metadata = json.loads(command('cargo', 'metadata', '--locked', '--format-version=1'))
packages = {p['id']: p for p in metadata['packages']}
node = next(n for n in metadata['resolve']['nodes'] if n['id'] == metadata['resolve']['root'])
manifest = tomllib.loads((root / 'Cargo.toml').read_text())
versions = {}
for dep in node['deps']:
    package = packages[dep['pkg']]
    name = package['name']
    if name not in manifest['dependencies']:
        continue
    info = {'resolved': package['version'], 'source': package['source']}
    if package['source'] and package['source'].startswith('registry+'):
        url = f'https://crates.io/api/v1/crates/{name}'
        data = json.loads(fetch(url, name + '.json'))
        latest = data['crate'].get('max_stable_version') or data['crate']['max_version']
        info.update(latest_stable=latest, current=latest == package['version'], source_url=url)
    versions[name] = info
stable_url = 'https://static.rust-lang.org/dist/channel-rust-stable.toml'
stable = tomllib.loads(fetch(stable_url, 'rust-stable.toml').decode())
update = subprocess.run(['cargo', 'update', '--dry-run', '--verbose'], cwd=root, text=True, capture_output=True)
(out / 'cargo-update.txt').write_text(update.stdout + update.stderr)
result = {
    'checked_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'python': sys.executable,
    'rustc': command('rustc', '--version'),
    'cargo': command('cargo', '--version'),
    'rust_latest_stable': stable['pkg']['rust']['version'],
    'rust_source_url': stable_url,
    'libcosmic_pinned': versions['libcosmic']['source'].rsplit('#', 1)[1],
    'libcosmic_head': command('git', 'ls-remote', 'https://github.com/pop-os/libcosmic.git', 'HEAD').split()[0],
    'direct_dependencies': versions,
    'compatible_update_check_exit': update.returncode,
    'compatible_update_check_output': str(out / 'cargo-update.txt'),
}
(root / 'verification' / 'dependencies.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
if update.returncode:
    raise SystemExit(update.returncode)
