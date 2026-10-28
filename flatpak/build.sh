#!/usr/bin/env bash
# Build locally without installing into the user's Flatpak installation.
# Usage: flatpak/build.sh [OUTPUT_DIR] [PREPARED_INPUTS_DIR]
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
app=dev.nilltadios.WarehouseCosmic
output="${1:-$PWD/flatpak/out}"
inputs="${2:-$PWD/flatpak/build-inputs}"
mkdir -p "$output"
output="$(realpath "$output")"
if [[ ! -f "$inputs/$app.json" ]]; then
  uv run --python 3.15 ci/prepare-flatpak.py "$inputs"
fi
inputs="$(realpath "$inputs")"
sign=()
if [[ -n "${FLATPAK_SIGNING_KEY:-}" ]]; then sign=("--gpg-sign=$FLATPAK_SIGNING_KEY"); fi
flatpak-builder --user --ccache --jobs="${BUILD_JOBS:-2}" --disable-rofiles-fuse --force-clean --default-branch=stable \
  --state-dir="$output/builder-state" --repo="$output/repo" "${sign[@]}" \
  "$output/build" "$inputs/$app.json"
flatpak build-update-repo --generate-static-deltas --title='Warehouse COSMIC' "${sign[@]}" "$output/repo"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
  "$output/repo" "$output/$app.flatpak" "$app" stable
printf 'Built %s\nNothing was installed.\n' "$output/$app.flatpak"
