#!/usr/bin/env bash
# Read-only compatibility check. No Parsec install/update/reinstall is applied.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
output="$PWD/verification/parsec-compatibility"
mkdir -p "$output"
base=https://nilltadios.github.io/com.parsecgaming.parsec
repo=/home/nilllinux22/Documents/parsec-flatpak-autobuild
cli="$PWD/target/debug/warehouse-cosmic-cli"
date --iso-8601=seconds > "$output/checked-at.txt"
git -C "$repo" rev-parse HEAD > "$output/source-commit.txt"
git -C "$repo" status --short > "$output/source-status-before.txt"
sha256sum "$repo/README.md" "$repo/ci/publish.sh" "$repo/.github/workflows/build.yml" "$repo/com.parsecgaming.parsec.yml" > "$output/source-files.sha256"
curl -fsSL "$base/state.json" -o "$output/state.json"
curl -fsSL "$base/parsec.flatpakrepo" -o "$output/parsec.flatpakrepo"
curl -fsSL "$base/com.parsecgaming.parsec.flatpakref" -o "$output/parsec.flatpakref"
printf '%s\n' "$base/state.json" "$base/parsec.flatpakrepo" "$base/com.parsecgaming.parsec.flatpakref" > "$output/source-urls.txt"
"$cli" run --cmd 'package.info={"installation":"user","reference":"app/com.parsecgaming.parsec/x86_64/stable"}' > "$output/installed.json"
"$cli" run --cmd 'package.update-check={"installation":"user","reference":"app/com.parsecgaming.parsec/x86_64/stable"}' > "$output/update-check.json"
"$cli" run --cmd 'remote.packages={"installation":"user","name":"parsec-autobuild","search":"parsec"}' > "$output/remote-packages.json"
"$cli" run --cmd 'package.reinstall={"installation":"user","reference":"app/com.parsecgaming.parsec/x86_64/stable","remote":"parsec-autobuild"}' > "$output/source-plan.json"
"$cli" run --cmd 'package.update={"installation":"user","references":["app/com.parsecgaming.parsec/x86_64/stable"]}' > "$output/update-plan.json"
file_request="$(jq -cn --arg path "$output/parsec.flatpakref" '{command:"package.install-files",params:{installation:"user",files:[$path]}}')"
"$cli" run --cmd "$file_request" > "$output/flatpakref-plan.json"
jq -e '.ok and .results[0].data.package.origin == "parsec-autobuild"' "$output/installed.json" > /dev/null
jq -e '.ok and .results[0].mode == "dry_run"' "$output/source-plan.json" > /dev/null
jq -e '.ok and .results[0].mode == "dry_run"' "$output/update-plan.json" > /dev/null
jq -e '.ok and .results[0].mode == "dry_run"' "$output/flatpakref-plan.json" > /dev/null
published_commit="$(jq -r .ostree_commit "$output/state.json")"
jq -e --arg commit "$published_commit" '.ok and .results[0].data.available_commit == $commit' "$output/update-check.json" > /dev/null
sha256sum -c "$output/source-files.sha256" > "$output/source-readback.txt"
git -C "$repo" status --short > "$output/source-status-after.txt"
cmp "$output/source-status-before.txt" "$output/source-status-after.txt"
printf 'PASS: custom signed remote, published commit, search, update/reinstall plans, .flatpakref plan, unchanged source project\n'
