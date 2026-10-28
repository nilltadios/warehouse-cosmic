#!/usr/bin/env bash
# Exercises the real headless binary against the offline Flatpak fixture.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
project="$PWD"
cli="$project/target/debug/warehouse-cosmic-cli"
fixture="$(cat verification/last-fixture.txt)"
test -f "$fixture/.warehouse-test-root"
export HOME="$fixture/home" XDG_DATA_HOME="$fixture/data" XDG_CONFIG_HOME="$fixture/config" XDG_CACHE_HOME="$fixture/cache"
unset DISPLAY WAYLAND_DISPLAY
"$cli" commands --json > "$fixture/cli-catalog.json"
jq -e '.schema == "warehouse-cosmic-commands/1" and (.commands | length == 27)' "$fixture/cli-catalog.json" > /dev/null
"$cli" describe package.uninstall > "$fixture/cli-describe.json"
"$cli" schema > "$fixture/cli-schema.json"
"$cli" run --script "$project/examples/audit.json" > "$fixture/cli-audit.json"
jq -e '.ok and (.results | length == 4)' "$fixture/cli-audit.json" > /dev/null
flatpak mask --user > "$fixture/masks-before"
"$cli" run --cmd 'package.mask={"installation":"user","reference":"runtime/org.example.WarehousePlatform/x86_64/stable","enabled":true}' > "$fixture/cli-plan.json"
jq -e '.ok and .results[0].mode == "dry_run" and (.results[0].applied | not)' "$fixture/cli-plan.json" > /dev/null
flatpak mask --user > "$fixture/masks-after-plan"
cmp "$fixture/masks-before" "$fixture/masks-after-plan"
"$cli" run --apply --cmd 'package.mask={"installation":"user","reference":"runtime/org.example.WarehousePlatform/x86_64/stable","enabled":true}' > "$fixture/cli-applied.json"
"$cli" run --cmd 'package.info={"installation":"user","reference":"runtime/org.example.WarehousePlatform/x86_64/stable"}' > "$fixture/cli-readback.json"
jq -e '.results[0].data.package.masked' "$fixture/cli-readback.json" > /dev/null
"$cli" run --apply --cmd 'package.mask={"installation":"user","reference":"runtime/org.example.WarehousePlatform/x86_64/stable","enabled":false}' > "$fixture/cli-unmask.json"
set +e
flock "$XDG_DATA_HOME/warehouse-cosmic/operation.lock" "$cli" run --apply --cmd 'package.mask={"installation":"user","reference":"runtime/org.example.WarehousePlatform/x86_64/stable","enabled":true}' > "$fixture/cli-busy.json" 2> "$fixture/cli-busy.stderr"
busy_status=$?
"$cli" run --cmd 'data.list={"force":true}' > "$fixture/cli-invalid.json" 2> "$fixture/cli-invalid.stderr"
invalid_status=$?
"$cli" run --cmd 'package.info={"installation":"user","reference":"org.example.DoesNotExist"}' > "$fixture/cli-missing.json" 2> "$fixture/cli-missing.stderr"
missing_status=$?
set -e
test "$busy_status" -eq 6
test "$invalid_status" -eq 2
test "$missing_status" -eq 3
jq -e '.error.code == "busy"' "$fixture/cli-busy.json" > /dev/null
flatpak mask --user > "$fixture/masks-final"
cmp "$fixture/masks-before" "$fixture/masks-final"
"$cli" run --cmd snapshot.list > "$fixture/snapshots.json"
snapshot_path="$(jq -r '.results[0].data[0].path' "$fixture/snapshots.json")"
mkdir -p "$fixture/exports"
export_request="$(jq -cn --arg src "$snapshot_path" --arg dst "$fixture/exports/backup.tar.zst" '{command:"snapshot.export",params:{path:$src,destination:$dst}}')"
"$cli" run --apply --automation-write-root "$fixture/exports" --cmd "$export_request" > "$fixture/cli-export.json"
test -s "$fixture/exports/backup.tar.zst"
test -s "$fixture/exports/backup.json"
import_request="$(jq -cn --arg src "$fixture/exports/backup.tar.zst" '{command:"snapshot.import",params:{id:"org.example.WarehouseFixture",file:$src,name:"CLI import"}}')"
"$cli" run --apply --automation-read-root "$fixture/exports" --cmd "$import_request" > "$fixture/cli-import.json"
jq -e '.ok and .results[0].mode == "applied"' "$fixture/cli-import.json" > /dev/null
restore_request="$(jq -cn --arg src "$snapshot_path" '{command:"snapshot.restore",params:{paths:[$src]}}')"
"$cli" run --apply --cmd "$restore_request" > "$fixture/cli-restore.json"
jq -e '.ok and .results[0].mode == "applied"' "$fixture/cli-restore.json" > /dev/null
"$cli" run --apply --cmd 'data.trash={"ids":["org.example.WarehouseFixture"]}' > "$fixture/cli-data-trash.json"
test ! -e "$HOME/.var/app/org.example.WarehouseFixture"
test -d "$XDG_DATA_HOME/Trash/files/org.example.WarehouseFixture"
"$cli" run --cmd snapshot.list > "$fixture/snapshots-before-trash.json"
trash_request="$(jq -c '{command:"snapshot.trash",params:{paths:[.results[0].data[].path]}}' "$fixture/snapshots-before-trash.json")"
"$cli" run --apply --cmd "$trash_request" > "$fixture/cli-snapshot-trash.json"
"$cli" run --cmd snapshot.list > "$fixture/snapshots-after-trash.json"
jq -e '.ok and (.results[0].data | length == 0)' "$fixture/snapshots-after-trash.json" > /dev/null
for file in "$fixture/test.flatpakref" "$fixture/test.flatpak"; do
  request="$(jq -cn --arg file "$file" '{command:"package.install-files",params:{installation:"user",files:[$file]}}')"
  "$cli" run --apply --cmd "$request" > "$fixture/cli-file-install-$(basename "$file").json"
  "$cli" run --cmd 'package.info={"installation":"user","reference":"app/org.example.WarehouseFixture/x86_64/stable"}' > "$fixture/cli-file-readback.json"
  jq -e '.ok and .results[0].data.package.id == "org.example.WarehouseFixture"' "$fixture/cli-file-readback.json" > /dev/null
  "$cli" run --apply --cmd 'package.uninstall={"installation":"user","references":["app/org.example.WarehouseFixture/x86_64/stable"],"trash_data":false}' > "$fixture/cli-file-uninstall.json"
done
printf 'PASS: no-display discovery, schema, scripts, dry-run isolation, apply/read-back, lock conflict, error codes, confined export/import, restore, data/snapshot Trash, signed .flatpakref and bundle installation\n'
printf 'Evidence: %s\n' "$fixture"
