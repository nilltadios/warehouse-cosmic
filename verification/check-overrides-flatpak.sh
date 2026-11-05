#!/usr/bin/env bash
# Check launcher/icon override detection (issue #1) in the packaged dev build.
#
#   flatpak/build.sh --dev
#   verification/check-overrides-flatpak.sh DIR     # DIR must be under $HOME and new
#
# The dev build is installed into DIR/installation (FLATPAK_USER_DIR), not the
# user's installation. The sandbox sees a fixture data folder through
# HOST_XDG_DATA_HOME=DIR/share and keeps its own data/config in DIR, so the
# user's ~/.local/share, the installed app's data and its settings are not
# touched. Installed packages are read from the host as-is (read-only).
# Changes: the fixture launcher is moved to the desktop Trash by the packaged
# CLI (disposable; it is named after a real app ID), and update-desktop-database
# rewrites DIR/share/applications/mimeinfo.cache. Exits non-zero on a failed check.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
app=dev.nilltadios.WarehouseCosmic
dir=$(realpath -m "$1")
case "$dir" in "$HOME"/*) ;; *) echo "DIR must be under \$HOME (sandbox access)" >&2; exit 2 ;; esac
[ ! -e "$dir" ] || { echo "$dir exists; pick a new DIR" >&2; exit 2; }
repo=$PWD/flatpak/out-dev/repo
[ -d "$repo" ] || { echo "no $repo; run flatpak/build.sh --dev" >&2; exit 2; }
mkdir -p "$dir"/{share/applications,appdata,config}
ln -sfn "$HOME/.config/cosmic" "$dir/config/cosmic"
fail() { echo "FAIL: $*" >&2; exit 1; }

# An installed app from the host to build the fixture around (read-only).
read -r id branch < <(flatpak list --app --columns=application,branch | grep -v "^$app" | head -1)
[ -n "${id:-}" ] || fail "no installed app to use as a fixture"
echo "fixture app: $id (installed branch $branch)"
launcher=$dir/share/applications/$id.desktop
printf '[Desktop Entry]\nType=Application\nName=Fixture\nExec=/usr/bin/flatpak run --branch=fixture-missing %s\nX-Flatpak=%s\n' "$id" "$id" > "$launcher"
icon=$dir/share/icons/hicolor/scalable/apps/$id.svg
mkdir -p "$(dirname "$icon")"
echo '<svg xmlns="http://www.w3.org/2000/svg"/>' > "$icon"
printf '[Desktop Entry]\nExec=flatpak run org.example.LeftoverFixture\nX-Flatpak=org.example.LeftoverFixture\n' > "$dir/share/applications/org.example.LeftoverFixture.desktop"
: > "$dir/share/applications/mimeinfo.cache"
touch -d '2000-01-01' "$dir/share/applications/mimeinfo.cache"

export FLATPAK_USER_DIR=$dir/installation FLATPAK_SYSTEM_DIR=$HOME/.local/share/flatpak
flatpak remote-add --user --no-gpg-verify warehouse-cosmic-dev "file://$repo"
flatpak install --user --no-deps --noninteractive -y warehouse-cosmic-dev "$app" > "$dir/install.log" 2>&1
flatpak info --user "$app" | grep -E '^ *(Ref|Commit):' | tee "$dir/installed.txt"
sandbox=(--user --env=HOST_XDG_DATA_HOME="$dir/share" --env=XDG_DATA_HOME="$dir/appdata" --env=XDG_CONFIG_HOME="$dir/config")
cli() { flatpak run "${sandbox[@]}" --command=warehouse-cosmic-cli "$app" "$@"; }

cli run --cmd 'override.list={}' > "$dir/list.json"
uv run --python 3.15 python -I - "$dir/list.json" "$launcher" "$icon" <<'EOF' || fail "override.list"
import json, sys
data = json.load(open(sys.argv[1]))["results"][0]["data"]
by = {o["path"]: o for o in data}
l, i = by.get(sys.argv[2]), by.get(sys.argv[3])
left = [o for o in data if o["id"] == "org.example.LeftoverFixture"]
assert l and l["broken"] and "branch fixture-missing, which is not installed" in l["message"], l
assert i and not i["broken"] and i["kind"] == "icon" and i["installed"], i
assert left and left[0]["broken"] and not left[0]["installed"], left
print("PASS override.list:", l["message"], "|", i["message"])
EOF
# Renders of the packaged GUI while the broken fixture launcher exists:
# packages = the start-up prompt and list mark from the real scan; overrides =
# the display-only sample prompt; leftovers = the real Leftover Data list.
# Each capture instance exits by itself.
for page in packages overrides overrides-narrow leftovers; do
    flatpak run "${sandbox[@]}" --env=WAREHOUSE_COSMIC_UI_ISOLATED=1 "$app" --preview "$page" --capture "$dir/$page.png" > "$dir/$page.log" 2>&1
    [ -s "$dir/$page.png" ] || fail "capture $page"
done
echo "PASS captures in $dir"
cli run --cmd "override.trash={\"paths\":[\"$HOME/.bashrc\"]}" > "$dir/refused.json" 2>&1 && fail "a path outside the scan was accepted"
grep -q 'not a launcher or icon reported by override.list' "$dir/refused.json" || fail "refusal text"
echo "PASS refuses paths override.list did not report"
cli run --cmd "override.trash={\"paths\":[\"$launcher\"]}" > "$dir/plan.json"
[ -e "$launcher" ] || fail "dry run moved the file"
echo "PASS dry run leaves the launcher"
cli run --apply --cmd "override.trash={\"paths\":[\"$launcher\"]}" > "$dir/apply.json"
[ ! -e "$launcher" ] || fail "launcher still present after override.trash --apply"
grep -q 'Refreshed with update-desktop-database' "$dir/apply.json" || fail "menu cache was not refreshed"
[ "$(stat -c %Y "$dir/share/applications/mimeinfo.cache")" -gt 946684800 ] || fail "mimeinfo.cache not rewritten"
echo "PASS launcher moved to Trash through host gio; menu cache refreshed"

echo "Remove the fixture later with: gio trash $dir   (the trashed fixture launcher is $id.desktop in the desktop Trash)"
