#!/usr/bin/env -S uv run --python 3.15
"""Fetch the official actionlint release/checksum and lint the publishing job."""
import datetime
import hashlib
import json
import pathlib
import subprocess
import tarfile
import urllib.request

root = pathlib.Path(__file__).resolve().parents[1]
out = root / "verification/flatpak-packaging/actionlint"
out.mkdir(parents=True, exist_ok=True)
url = "https://api.github.com/repos/rhysd/actionlint/releases/latest"
def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "warehouse-cosmic-verification"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()
release_raw = fetch(url)
(out / "release.json").write_bytes(release_raw)
release = json.loads(release_raw)
archive = next(asset for asset in release["assets"] if asset["name"].endswith("linux_amd64.tar.gz"))
checksum = next(asset for asset in release["assets"] if asset["name"].endswith("checksums.txt"))
checksums = fetch(checksum["browser_download_url"])
(out / "checksums.txt").write_bytes(checksums)
body = fetch(archive["browser_download_url"])
expected = next(line.split()[0] for line in checksums.decode().splitlines() if line.split()[-1] == archive["name"])
assert hashlib.sha256(body).hexdigest() == expected, "actionlint checksum mismatch"
path = out / archive["name"]
path.write_bytes(body)
with tarfile.open(path) as tar:
    binary = tar.extractfile("actionlint").read()
(out / "actionlint").write_bytes(binary)
(out / "actionlint").chmod(0o755)
result = subprocess.run([str(out / "actionlint"), ".github/workflows/flatpak.yml"], cwd=root, text=True, capture_output=True)
(out / "lint.log").write_text(result.stdout + result.stderr)
(out / "result.json").write_text(json.dumps({"checked_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "source": url, "archive_url": archive["browser_download_url"], "checksum_url": checksum["browser_download_url"], "sha256": expected, "exit_status": result.returncode}, indent=2) + "\n")
print(result.stdout + result.stderr)
raise SystemExit(result.returncode)
