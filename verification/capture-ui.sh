#!/usr/bin/env bash
# Render exactly this app, then exit each test instance. No desktop pixels are used.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
monitor="${1:-DP-1}"
output="${CAPTURE_OUTPUT:-$HOME/Documents/warehouse-cosmic-tests/$(date +%Y-%m-%d)/warehouse-cosmic/captures-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$output"
printf '%s\n' "$output" > verification/last-captures.txt
helper=on-screen.sh
app=dev.nilltadios.WarehouseCosmic
location="$(flatpak info --user --show-location "$app")"
sha256sum "$location/files/bin/warehouse-cosmic" > "$output/binary.sha256"
flatpak info --user "$app" > "$output/installed-info.txt"
(pgrep -x warehouse-cosmi || true) | sort > "$output/existing-pids.txt"
kscreen-doctor -j > "$output/monitors.json"
for page in packages remotes data leftovers snapshots install filters selection remote-dialog new-snapshot import about appearance parsec source updates; do
  if [[ "$page" == packages ]]; then
    "$helper" "$monitor" "$app" -- flatpak run --env=RUST_LOG=warn,iced_wgpu=info --env=WAYLAND_DEBUG=1 "$app" --preview "$page" --capture "$output/$page.png" > "$output/$page.log" 2>&1
  else
    "$helper" "$monitor" "$app" -- flatpak run "$app" --preview "$page" --capture "$output/$page.png" > "$output/$page.log" 2>&1
  fi
  test -s "$output/$page.png"
done
printf 'Saved previews: %s\n' "$output"
for ((attempt=0; attempt<30; attempt++)); do
  (pgrep -x warehouse-cosmi || true) | sort > "$output/final-pids.txt"
  comm -13 "$output/existing-pids.txt" "$output/final-pids.txt" > "$output/leftover-pids.txt"
  [[ ! -s "$output/leftover-pids.txt" ]] && break
  sleep 0.1
done
if [[ -s "$output/leftover-pids.txt" ]]; then
  printf 'A new Warehouse process remains; see %s\n' "$output/leftover-pids.txt" >&2
  exit 1
fi
printf 'PASS: preview processes exited; pre-existing user windows were preserved\n'
