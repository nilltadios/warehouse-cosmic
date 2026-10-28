#!/usr/bin/env bash
# Compare invariant package/remote fields with the initial read-only inventory.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
./target/debug/warehouse-cosmic-cli inspect > verification/host-inventory-final.json
# The new per-pattern mask/pin arrays were added after the initial snapshot.
# Existing masked/pinned flags are still compared.
jq -S '[(.packages | map(del(.masks,.pins))), .remotes]' verification/host-inventory.json > verification/host-before-state.json
jq -S '[(.results[0].data.packages | map(del(.masks,.pins))), .results[0].data.remotes]' verification/host-inventory-final.json > verification/host-after-state.json
cmp verification/host-before-state.json verification/host-after-state.json
printf 'PASS: installed packages, masks/pins and remote state match the initial inventory\n'
