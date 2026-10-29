#!/usr/bin/env -S uv run --python 3.15
"""Lay out a signed Flatpak repository for GitHub Pages; never overwrite a site.

Usage: publish.py REPO SITE BASE_URL GPG_KEY INPUTS_JSON SOURCE_COMMIT
This only writes local files. It does not deploy or publish anything itself.
"""
import base64
import datetime
import html
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import urllib.parse

root = pathlib.Path(__file__).resolve().parents[1]
repo, site, base, key, inputs_path, commit = sys.argv[1:]
repo, site = pathlib.Path(repo), pathlib.Path(site)
base = base.rstrip("/") + "/"
parsed = urllib.parse.urlsplit(base)
if parsed.scheme != "https" or not parsed.netloc or parsed.query or parsed.fragment:
    raise SystemExit("BASE_URL must be an HTTPS directory URL without a query or fragment")
if site.exists():
    raise SystemExit("SITE already exists. Choose a new output directory.")
inputs = json.loads(pathlib.Path(inputs_path).read_text())
app = inputs["app_id"]
ref = f"app/{app}/x86_64/stable"
ostree_commit = subprocess.check_output(["ostree", f"--repo={repo}", "rev-parse", ref], text=True).strip()
# Refuse an unsigned input instead of advertising a key that does not sign it.
pub = subprocess.check_output(["gpg", "--batch", "--export", key])
if not pub:
    raise SystemExit("Signing public key not found")
with tempfile.TemporaryDirectory(prefix="warehouse-verify-") as keyring:
    os.chmod(keyring, 0o700)
    # OSTree reads an exported legacy keyring, not GnuPG's newer keybox format.
    (pathlib.Path(keyring) / "pubring.gpg").write_bytes(pub)
    verified = subprocess.run(["ostree", f"--repo={repo}", "show", f"--gpg-homedir={keyring}", ostree_commit], env={**os.environ, "LC_ALL": "C"}, capture_output=True, text=True)
    if verified.returncode or "Good signature from" not in verified.stdout:
        raise SystemExit("Repository commit signature verification failed: " + verified.stdout + verified.stderr)
site.mkdir(parents=True)
shutil.copytree(repo, site / "repo")
(site / "repo/.lock").unlink(missing_ok=True)
gpg = base64.b64encode(pub).decode()
fingerprint = subprocess.check_output(["gpg", "--batch", "--with-colons", "--fingerprint", key], text=True).split("fpr:::::::::", 1)[1].split(":", 1)[0]
fingerprint = " ".join(fingerprint[i:i + 4] for i in range(0, len(fingerprint), 4))
common = f"Url={base}repo/\nHomepage={base}\nIcon={base}icon.svg\nGPGKey={gpg}\n"
(site / "warehouse-cosmic.flatpakrepo").write_text("[Flatpak Repo]\nTitle=Warehouse COSMIC\nComment=Native Wayland Flatpak toolbox\nDescription=Signed Warehouse COSMIC releases\n" + common)
(site / f"{app}.flatpakref").write_text(f"[Flatpak Ref]\nTitle=Warehouse COSMIC\nName={app}\nBranch=stable\nSuggestRemoteName=warehouse-cosmic\nIsRuntime=false\nRuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo\n" + common)
shutil.copy2(root / "resources" / f"{app}.svg", site / "icon.svg")
(site / ".nojekyll").touch()
state = {**inputs, "source_commit": commit, "ostree_commit": ostree_commit,
         "published_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "base_url": base}
(site / "state.json").write_text(json.dumps(state, indent=2) + "\n")
title = "Warehouse COSMIC"
(site / "index.html").write_text(f'''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>{title}</title><style>
body{{background:#151517;color:#eee;font:18px/1.6 system-ui;max-width:780px;margin:60px auto;padding:24px}}
a{{color:#ff5361}}img{{width:96px}}code,pre{{background:#252529;padding:8px;border-radius:8px}}pre{{overflow:auto}}
.install{{display:inline-block;background:#e51d38;color:white;padding:12px 20px;border-radius:12px;text-decoration:none}}
</style><main><img src="icon.svg" alt=""><h1>{title}</h1>
<p>A Rust and libcosmic Flatpak toolbox with native Wayland and GPU rendering.</p>
<p><a class="install" href="{app}.flatpakref">Install Warehouse COSMIC</a></p>
<p>Open the downloaded install file in Discover or GNOME Software. It adds the signed <code>warehouse-cosmic</code> remote. Updates arrive through your normal Flatpak updater.</p>
<p>Version {html.escape(str(inputs['version']))}. Freedesktop Platform {html.escape(inputs['runtime_version'])}. x86_64. Built from commit <code>{html.escape(commit[:12])}</code>.</p>
<h2>Terminal install</h2><pre>flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists warehouse-cosmic {html.escape(base)}warehouse-cosmic.flatpakrepo
flatpak install --user warehouse-cosmic {app}</pre>
<h2>Installed from a downloaded file?</h2><p>Add the remote above, then switch the installed app to it. Settings and data are kept.</p><pre>flatpak install --user --reinstall warehouse-cosmic {app}</pre>
<h2>Signing key</h2><p>Fingerprint <code>{html.escape(fingerprint)}</code>. Compare it with the key in <a href="warehouse-cosmic.flatpakrepo">warehouse-cosmic.flatpakrepo</a> and in the project README.</p>
<h2>Standalone CLI</h2><pre>flatpak run --command=warehouse-cosmic-cli {app} commands --json</pre>
<p>Host command and home-directory access are required to manage packages and app data.</p>
<p><a href="state.json">Published build information</a> · GPL-3.0-or-later</p></main></html>''')
print(site)
