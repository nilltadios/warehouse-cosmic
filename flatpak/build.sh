#!/usr/bin/env bash
# Build locally. Nothing is installed unless --dev --install is given.
#
# Usage: flatpak/build.sh [OUTPUT_DIR] [PREPARED_INPUTS_DIR]
#          Release build: signed when FLATPAK_SIGNING_KEY is set, static deltas,
#          repository summary and a .flatpak bundle. Used by CI and releases.
#        flatpak/build.sh --dev [--install]
#          Fast local iteration: regenerates flatpak/build-inputs-dev from the
#          working tree, builds with Cargo's flatpak-dev profile (no LTO,
#          incremental) into flatpak/out-dev, and skips deltas and the bundle.
#          --install also installs the result into the user installation.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
app=dev.nilltadios.WarehouseCosmic
dev=0
install=0
positional=()
for arg in "$@"; do
  case "$arg" in
    --dev) dev=1 ;;
    --install) install=1 ;;
    *) positional+=("$arg") ;;
  esac
done
if [[ $install == 1 && $dev == 0 ]]; then
  echo "--install is only available with --dev; install release bundles with flatpak install." >&2
  exit 2
fi
# Leave two cores for the desktop on larger machines.
cores="$(nproc)"
jobs="${BUILD_JOBS:-$(( cores > 4 ? cores - 2 : 2 ))}"

if [[ $dev == 1 ]]; then
  output="${positional[0]:-$PWD/flatpak/out-dev}"
  inputs="$PWD/flatpak/build-inputs-dev"
  # Dev inputs are always regenerated from the working tree; this directory
  # belongs to this script. Vendored crates come from flatpak/cache.
  rm -rf -- "$inputs"
  uv run --python 3.15 ci/prepare-flatpak.py --profile dev "$inputs" >/dev/null
  mkdir -p "$output"
  output="$(realpath "$output")"
  install_args=()
  if [[ $install == 1 ]]; then install_args=(--install); fi
  flatpak-builder --user --ccache --jobs="$jobs" --disable-rofiles-fuse --force-clean --default-branch=stable \
    --state-dir="$output/builder-state" --repo="$output/repo" "${install_args[@]}" \
    "$output/build" "$inputs/$app.json"
  if [[ $install == 1 ]]; then
    printf 'Built and installed a dev build for the user installation.\n'
  else
    printf 'Built %s (dev profile). Nothing was installed.\n' "$output/repo"
  fi
  exit 0
fi

output="${positional[0]:-$PWD/flatpak/out}"
inputs="${positional[1]:-$PWD/flatpak/build-inputs}"
mkdir -p "$output"
output="$(realpath "$output")"
if [[ ! -f "$inputs/$app.json" ]]; then
  uv run --python 3.15 ci/prepare-flatpak.py "$inputs"
fi
inputs="$(realpath "$inputs")"
sign=()
if [[ -n "${FLATPAK_SIGNING_KEY:-}" ]]; then sign=("--gpg-sign=$FLATPAK_SIGNING_KEY"); fi
flatpak-builder --user --ccache --jobs="$jobs" --disable-rofiles-fuse --force-clean --default-branch=stable \
  --state-dir="$output/builder-state" --repo="$output/repo" "${sign[@]}" \
  "$output/build" "$inputs/$app.json"
flatpak build-update-repo --generate-static-deltas --title='Warehouse COSMIC' "${sign[@]}" "$output/repo"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
  "$output/repo" "$output/$app.flatpak" "$app" stable
printf 'Built %s\nNothing was installed.\n' "$output/$app.flatpak"
