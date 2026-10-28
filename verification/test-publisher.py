#!/usr/bin/env -S uv run --python 3.15
"""Exercise the Pages publisher with an isolated signed Flatpak repository.

Saves commands, logs and outputs below verification/flatpak-packaging. Does not
deploy, install into the real account, or use the user's signing keys.
"""
import base64
import configparser
import json
import os
import pathlib
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
area = root / "verification/flatpak-packaging"
area.mkdir(exist_ok=True)
fixture = pathlib.Path(tempfile.mkdtemp(prefix="publisher-", dir=area))
app = "dev.nilltadios.WarehouseCosmic"
gnupg = fixture / "gnupg"
gnupg.mkdir(mode=0o700)
env = {**os.environ, "GNUPGHOME": str(gnupg), "LC_ALL": "C"}
log = (fixture / "commands.log").open("w")


def run(*args, expected=0, environment=env):
    result = subprocess.run(args, cwd=root, env=environment, text=True, capture_output=True)
    log.write(json.dumps({"command": args, "exit_status": result.returncode}) + "\n" + result.stdout + result.stderr + "\n")
    log.flush()
    assert result.returncode == expected, f"{args}: expected {expected}, got {result.returncode}; see {fixture}"
    return result.stdout


try:
    source = fixture / "app"
    (source / "files").mkdir(parents=True)
    (source / "export").mkdir()
    (source / "files/test.txt").write_text("Publisher fixture, not the application binary.\n")
    (source / "metadata").write_text(f"[Application]\nname={app}\nruntime=org.freedesktop.Platform/x86_64/26.08\nsdk=org.freedesktop.Sdk/x86_64/26.08\ncommand=true\n")
    repo = fixture / "repo"
    run("flatpak", "build-export", "--disable-sandbox", str(repo), str(source), "stable")
    run("gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "--quick-generate-key", "Warehouse Publisher Fixture <fixture@example.invalid>", "ed25519", "sign", "0")
    keys = run("gpg", "--batch", "--with-colons", "--list-secret-keys")
    key = next(line.split(":")[9] for line in keys.splitlines() if line.startswith("fpr:"))
    inputs = fixture / "inputs.json"
    inputs.write_text(json.dumps({"app_id": app, "version": "0.1.0", "runtime_version": "26.08"}))
    def publish(site, expected=0):
        return run("uv", "run", "--python", "3.15", "ci/publish.py", str(repo), str(site), "https://example.invalid/warehouse/", key, str(inputs), "fixture-source", expected=expected)
    publish(fixture / "unsigned-site", expected=1)
    assert not (fixture / "unsigned-site").exists()
    commit = run("ostree", f"--repo={repo}", "rev-parse", f"app/{app}/x86_64/stable").strip()
    run("ostree", f"--repo={repo}", "gpg-sign", commit, key)
    run("flatpak", "build-update-repo", f"--gpg-sign={key}", str(repo))
    site = fixture / "site"
    publish(site)
    state = json.loads((site / "state.json").read_text())
    assert state["ostree_commit"] == commit and state["runtime_version"] == "26.08"
    ref = configparser.ConfigParser(interpolation=None)
    ref.read(site / f"{app}.flatpakref")
    assert ref["Flatpak Ref"]["SuggestRemoteName"] == "warehouse-cosmic"
    assert ref["Flatpak Ref"]["RuntimeRepo"] == "https://dl.flathub.org/repo/flathub.flatpakrepo"
    pub = subprocess.check_output(["gpg", "--batch", "--export", key], env=env)
    assert base64.b64decode(ref["Flatpak Ref"]["GPGKey"]) == pub
    publish(site, expected=1)
    isolated = fixture / "home"
    isolated.mkdir()
    client_env = {**env, "HOME": str(isolated), "XDG_DATA_HOME": str(fixture / "data"), "XDG_CONFIG_HOME": str(fixture / "config"), "XDG_CACHE_HOME": str(fixture / "cache")}
    repo_file = fixture / "test.flatpakrepo"
    repo_file.write_text("[Flatpak Repo]\nTitle=Warehouse fixture\nUrl=" + repo.as_uri() + "\nGPGKey=" + base64.b64encode(pub).decode() + "\n")
    run("flatpak", "remote-add", "--user", "warehouse-fixture", str(repo_file), environment=client_env)
    listed = run("flatpak", "remote-ls", "--user", "--columns=ref", "warehouse-fixture", environment=client_env)
    assert app in listed
    result = {"passed": True, "fixture": str(fixture), "checks": ["unsigned commit refused", "signed commit verified", "public key and runtime ref correct", "existing site preserved", "isolated client trusts signed summary and sees app"]}
    (area / "publisher-results.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))
finally:
    subprocess.run(["gpgconf", "--homedir", str(gnupg), "--kill", "gpg-agent"], capture_output=True)
    log.close()
