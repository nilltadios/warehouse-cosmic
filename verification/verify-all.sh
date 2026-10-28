#!/usr/bin/env bash
# Run reproducible checks and keep exact commands, exit statuses and logs.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
run_dir="$PWD/verification/checks-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$run_dir"
printf '%s\n' "$run_dir" > verification/last-checks.txt
record="$run_dir/checks.jsonl"
touch "$record"
finish() {
  final_status=$?
  jq -s --argjson exit "$final_status" '{schema:"warehouse-cosmic-verification/1",all_passed:($exit==0),exit_status:$exit,checks:.}' "$record" > "$run_dir/results.json"
  printf 'Verification report: %s/results.json\n' "$run_dir"
}
trap finish EXIT
run_check() {
  check_name="$1"; shift
  check_args="$(jq -cn --args '$ARGS.positional' -- "$@")"
  if "$@" > "$run_dir/$check_name.log" 2>&1; then check_status=0; else check_status=$?; fi
  jq -cn --arg name "$check_name" --argjson command "$check_args" --argjson exit "$check_status" --arg log "$run_dir/$check_name.log" '{name:$name,command:$command,exit_status:$exit,log:$log}' >> "$record"
  printf '%s exit=%s\n' "$check_name" "$check_status"
  return "$check_status"
}
run_check format cargo fmt --all --check
run_check tests cargo test --locked
run_check clippy cargo clippy --all-targets --locked -- -D warnings
run_check build cargo build --locked --bins
run_check headless-build cargo check --locked --no-default-features --bin warehouse-cosmic-cli
run_check software-build cargo check --locked --no-default-features --features gui --bin warehouse-cosmic
run_check offline-flatpak ./verification/backend-smoke.sh
run_check standalone-cli ./verification/verify-cli.sh
run_check cli-reference ./verification/update-cli-reference.sh
if [[ -d /home/nilllinux22/Documents/parsec-flatpak-autobuild ]]; then
  run_check parsec-readonly ./verification/check-parsec.sh
fi
if [[ "${1:-}" == --desktop ]]; then
  run_check desktop ./verification/capture-ui.sh "${2:-DP-1}"
fi
