#!/usr/bin/env -S uv run --python 3.15
"""Fetch the bundled preset definitions without adding any host remotes.

Every response, URL, time, checksum and failure is retained beside the report.
Re-run from anywhere: uv run --python 3.15 verification/check-popular-remotes.py
"""
import concurrent.futures
import configparser
import datetime
import hashlib
import json
import pathlib
import urllib.request

root = pathlib.Path(__file__).resolve().parents[1]
catalog = root / "resources/popular-remotes.json"
stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
output = root / "verification/flatpak-packaging" / ("popular-remotes-" + stamp)
output.mkdir(parents=True, exist_ok=False)
presets = json.loads(catalog.read_text())

def fetch(preset):
    source = preset["url"]
    url = source.replace("oci+https://", "https://")
    oci = source.startswith("oci+")
    if oci:
        url += "/v2/"
    record = {"name": preset["name"], "source_url": source, "fetch_url": url,
              "fetched_at": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "WarehouseCosmic-PresetCheck/1"})
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read()
            record.update({"http_status": response.status, "final_url": response.url})
        filename = preset["name"] + (".json" if oci else ".flatpakrepo")
        (output / filename).write_bytes(data)
        record.update({"response_file": str(output / filename), "sha256": hashlib.sha256(data).hexdigest()})
        if oci:
            json.loads(data)
        else:
            config = configparser.ConfigParser(interpolation=None)
            config.read_string(data.decode())
            assert config.has_section("Flatpak Repo")
            assert config.get("Flatpak Repo", "Url").startswith(("https://", "http://", "oci+https://")), "Unrecognized Flatpak repository URL"
            record["repository_url"] = config.get("Flatpak Repo", "Url")
            record["contains_signing_key"] = config.has_option("Flatpak Repo", "GPGKey")
        record["passed"] = True
    except Exception as error:
        record.update({"passed": False, "error": str(error)})
    return record

with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
    records = list(pool.map(fetch, presets))
report = {"passed": all(record["passed"] for record in records), "catalog": str(catalog),
          "catalog_sha256": hashlib.sha256(catalog.read_bytes()).hexdigest(),
          "reproduce": "uv run --python 3.15 verification/check-popular-remotes.py",
          "host_remotes_modified": False, "sources": records}
(output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
(root / "verification/flatpak-packaging/popular-remotes-results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"passed": report["passed"], "report": str(output / "results.json")}))
raise SystemExit(0 if report["passed"] else 1)
