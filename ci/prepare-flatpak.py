#!/usr/bin/env -S uv run --python 3.15
"""Vendor Cargo.lock and create an offline, checksummed Flatpak source archive.

Local: uv run --python 3.15 ci/prepare-flatpak.py OUTPUT [--profile dev]
CI: Python 3.11+ from the runner is sufficient; no app Python runtime is shipped.
Existing output directories are refused. Downloaded Rust is checked by builder
against the manifest hash; its official checksum is also saved and compared here.

Vendored crates go in their own archive, cached under flatpak/cache by the
Cargo.lock hash, so a source-only change re-archives only the app (a few MB).
`--profile dev` builds with Cargo's flatpak-dev profile (no LTO, incremental)
for local iteration; published builds always use the release profile.
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


def write_archive(path, directory):
    """A reproducible tar.gz of DIRECTORY's children, owned by root with zero mtimes."""
    def normalize(info):
        info.uid = info.gid = 0
        info.uname = info.gname = "root"
        info.mtime = 0
        return info
    with path.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", compresslevel=3, mtime=0) as zipped, tarfile.open(fileobj=zipped, mode="w|") as tar:
        for child in sorted(directory.iterdir()):
            tar.add(child, arcname=child.name, filter=normalize)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--profile", choices=("release", "dev"), default="release")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((ROOT / "flatpak" / (APP + ".json")).read_text())
    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    rust_source = manifest["modules"][0]["sources"][0]
    assert f"rust-{toolchain}-" in rust_source["url"], "Manifest/toolchain pin mismatch"
    checksum_url = rust_source["url"] + ".sha256"
    cache = ROOT / "flatpak" / "cache"
    cache.mkdir(parents=True, exist_ok=True)
    cached_checksum = cache / (pathlib.Path(rust_source["url"]).name + ".sha256")
    if cached_checksum.is_file():
        checksum = cached_checksum.read_text()
    else:
        with urllib.request.urlopen(checksum_url, timeout=60) as response:
            checksum = response.read().decode()
        cached_checksum.write_text(checksum)
    (output / "rust-official.sha256").write_text(checksum)
    assert checksum.split()[0] == rust_source["sha256"], "Official Rust archive checksum differs"
    lock_sha = digest(ROOT / "Cargo.lock")
    vendored = cache / f"vendor-{lock_sha[:16]}.tar.gz"
    with tempfile.TemporaryDirectory(prefix="warehouse-inputs-", dir=output) as work:
        stage = pathlib.Path(work)
        if not vendored.is_file():
            vendor_stage = stage / "vendor-stage"
            vendor = subprocess.run(["cargo", "vendor", "--locked", "--versioned-dirs", str(vendor_stage / "vendor")], cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            (output / "cargo-vendor.log").write_text(vendor.stderr)
            vendor.check_returncode()
            config = re.sub(r'^directory = .*$', 'directory = "vendor"', vendor.stdout, flags=re.M)
            (vendor_stage / ".cargo").mkdir()
            (vendor_stage / ".cargo/config.toml").write_text(config)
            partial = vendored.with_suffix(".partial")
            write_archive(partial, vendor_stage)
            partial.replace(vendored)
        app_stage = stage / "app"
        app_stage.mkdir()
        for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "LICENSE", "NOTICE", "llms.txt", "src", "resources", "docs", "examples", "schemas"):
            source = ROOT / name
            if source.is_dir():
                shutil.copytree(source, app_stage / name)
            else:
                shutil.copy2(source, app_stage / name)
        archive = output / "warehouse-cosmic-source.tar.gz"
        write_archive(archive, app_stage)
    # Keep the inputs directory self-contained (CI uploads it as one artifact).
    vendor_archive = output / "warehouse-cosmic-vendor.tar.gz"
    try:
        vendor_archive.hardlink_to(vendored)
    except OSError:
        shutil.copy2(vendored, vendor_archive)
    module = manifest["modules"][1]
    module["sources"] = [
        {"type": "archive", "path": archive.name, "strip-components": 0, "sha256": digest(archive)},
        {"type": "archive", "path": vendor_archive.name, "strip-components": 0, "sha256": digest(vendor_archive)},
    ]
    if args.profile == "dev":
        module["build-commands"] = [c.replace("--release", "--profile flatpak-dev").replace("/release/", "/flatpak-dev/") for c in module["build-commands"]]
        module["build-options"]["env"]["CARGO_INCREMENTAL"] = "1"
    (output / (APP + ".json")).write_text(json.dumps(manifest, indent=2) + "\n")
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True)
    inputs = {"app_id": APP, "version": tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"],
              "runtime": manifest["runtime"], "runtime_version": manifest["runtime-version"],
              "rust_version": toolchain, "rust_url": rust_source["url"], "rust_sha256": rust_source["sha256"],
              "rust_checksum_url": checksum_url, "source_sha256": digest(archive), "vendor_sha256": digest(vendor_archive),
              "lock_sha256": lock_sha, "cargo_profile": args.profile,
              "prepared_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "source_commit": commit.stdout.strip() if commit.returncode == 0 else None}
    (output / "inputs.json").write_text(json.dumps(inputs, indent=2) + "\n")
    print(output / (APP + ".json"))


if __name__ == "__main__":
    main()
