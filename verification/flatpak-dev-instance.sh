#!/usr/bin/env bash
# Install the dev Flatpak build into a private Flatpak directory and start one
# isolated GUI instance of it, for checking the packaged app by hand or with
# verification/drag-sweep.sh. The user's installed app, its data and its
# settings are not touched:
#   - FLATPAK_USER_DIR points at DIR/installation; the user's installation is
#     shared read-only as the system source of runtimes (as verify-flatpak.py).
#   - XDG_CONFIG_HOME inside the sandbox is DIR/config (preferences, window
#     and view state), with the COSMIC theme linked in read-only.
#   - WAREHOUSE_COSMIC_UI_ISOLATED=1 skips single-instance activation, so the
#     user's running window is never reused.
#
#   flatpak/build.sh --dev
#   verification/flatpak-dev-instance.sh DIR      # DIR must be under $HOME
#
# Prints the UI instance name for `warehouse-cosmic-cli ui ... --instance`.
# Stop it with `ui close --apply --instance NAME`; remove DIR afterwards.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
app=dev.nilltadios.WarehouseCosmic
dir=$(realpath -m "$1")
case "$dir" in "$HOME"/*) ;; *) echo "DIR must be under \$HOME (sandbox access)" >&2; exit 2 ;; esac
repo=$PWD/flatpak/out-dev/repo
[ -d "$repo" ] || { echo "no $repo; run flatpak/build.sh --dev" >&2; exit 2; }
export FLATPAK_USER_DIR=$dir/installation FLATPAK_SYSTEM_DIR=$HOME/.local/share/flatpak
mkdir -p "$dir/config"
ln -sfn "$HOME/.config/cosmic" "$dir/config/cosmic"
if ! flatpak remote-list --user --columns=name | grep -qx warehouse-cosmic-dev; then
    flatpak remote-add --user --no-gpg-verify warehouse-cosmic-dev "file://$repo"
fi
flatpak install --user --no-deps --noninteractive -y --reinstall warehouse-cosmic-dev "$app" >"$dir/install.log" 2>&1
flatpak info --user "$app" | grep -E '^ *(Ref|Commit|Installed):' >"$dir/installed.txt"
instance=$app.UI.i$(od -An -N8 -tu8 /dev/urandom | tr -d ' ')
# --user: the private installation, not the user's copy in the system source.
setsid -f flatpak run --user \
    --env=WAREHOUSE_COSMIC_UI_INSTANCE="$instance" --env=WAREHOUSE_COSMIC_UI_ISOLATED=1 \
    --command=env "$app" XDG_CONFIG_HOME="$dir/config" /app/bin/warehouse-cosmic \
    >"$dir/gui.log" 2>&1 </dev/null
echo "$instance"
