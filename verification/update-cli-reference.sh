#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
mkdir -p schemas
./target/debug/warehouse-cosmic-cli commands --json > schemas/commands.json
./target/debug/warehouse-cosmic-cli schema > schemas/request.json
./target/debug/warehouse-cosmic-cli ui commands > schemas/ui-commands.json
./target/debug/warehouse-cosmic-cli ui schema > schemas/ui-request.json
