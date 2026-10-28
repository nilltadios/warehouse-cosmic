#!/usr/bin/env -S uv run --python 3.15
"""Vendor Cargo.lock and create an offline, checksummed Flatpak source archive.

Local: uv run --python 3.15 ci/prepare-flatpak.py OUTPUT
CI: Python 3.11+ from the runner is sufficient; no app Python runtime is shipped.
Existing output directories are refused. Downloaded Rust is checked by builder
against the manifest hash; its official checksum is also saved and compared here.
"""
import argparse
import datetime
import gzip
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
APP = "dev.nilltadios.WarehouseCosmic"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((ROOT / "flatpak" / (APP + ".json")).read_text())
    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    rust_source = manifest["modules"][0]["sources"][0]
    assert f"rust-{toolchain}-" in rust_source["url"], "Manifest/toolchain pin mismatch"
    checksum_url = rust_source["url"] + ".sha256"
    with urllib.request.urlopen(checksum_url, timeout=60) as response:
        checksum = response.read().decode()
    (output / "rust-official.sha256").write_text(checksum)
    assert checksum.split()[0] == rust_source["sha256"], "Official Rust archive checksum differs"
    with tempfile.TemporaryDirectory(prefix="warehouse-inputs-", dir=output) as work:
        stage = pathlib.Path(work)
        for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "LICENSE", "NOTICE", "llms.txt", "src", "resources", "docs", "examples", "schemas"):
            source = ROOT / name
            if source.is_dir():
                shutil.copytree(source, stage / name)
            else:
                shutil.copy2(source, stage / name)
        vendor = subprocess.run(["cargo", "vendor", "--locked", "--versioned-dirs", str(stage / "vendor")], cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        (output / "cargo-vendor.log").write_text(vendor.stderr)
        vendor.check_returncode()
        config = re.sub(r'^directory = .*$', 'directory = "vendor"', vendor.stdout, flags=re.M)
        (stage / ".cargo").mkdir()
        (stage / ".cargo/config.toml").write_text(config)
        archive = output / "warehouse-cosmic-source.tar.gz"
        def normalize(info):
            info.uid = info.gid = 0
            info.uname = info.gname = "root"
            info.mtime = 0
            return info
        with archive.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", compresslevel=3, mtime=0) as zipped, tarfile.open(fileobj=zipped, mode="w|") as tar:
            for child in sorted(stage.iterdir()):
                tar.add(child, arcname=child.name, filter=normalize)
    manifest["modules"][1]["sources"][0]["sha256"] = digest(archive)
    (output / (APP + ".json")).write_text(json.dumps(manifest, indent=2) + "\n")
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True)
    inputs = {"app_id": APP, "version": tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"],
              "runtime": manifest["runtime"], "runtime_version": manifest["runtime-version"],
              "rust_version": toolchain, "rust_url": rust_source["url"], "rust_sha256": rust_source["sha256"],
              "rust_checksum_url": checksum_url, "source_sha256": digest(archive), "lock_sha256": digest(ROOT / "Cargo.lock"),
              "prepared_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "source_commit": commit.stdout.strip() if commit.returncode == 0 else None}
    (output / "inputs.json").write_text(json.dumps(inputs, indent=2) + "\n")
    print(output / (APP + ".json"))


if __name__ == "__main__":
    main()
