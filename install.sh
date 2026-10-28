#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
install_prefix="${HOME}/.local"
app_id=dev.nilltadios.WarehouseCosmic
files=("$install_prefix/bin/warehouse-cosmic" "$install_prefix/bin/warehouse-cosmic-cli" "$install_prefix/share/applications/$app_id.desktop" "$install_prefix/share/icons/hicolor/scalable/apps/$app_id.svg" "$install_prefix/share/icons/WarehouseCosmic" "$install_prefix/share/metainfo/$app_id.metainfo.xml")
if [[ "${1:-}" == --dry-run ]]; then printf 'Install destinations:\n'; printf '%s\n' "${files[@]}"; exit 0; fi
if [[ $# -ne 0 ]]; then printf 'usage: ./install.sh [--dry-run]\n' >&2; exit 2; fi
cargo build --release --locked --bins -j 2
stamp="$(date +%Y%m%d-%H%M%S)"
for path in "${files[@]}"; do
  if [[ -d "$path" && ! -L "$path" ]]; then mv -- "$path" "$path.bak-$stamp"
  elif [[ -e "$path" || -L "$path" ]]; then cp -a -- "$path" "$path.bak-$stamp"; fi
done
install -Dm755 target/release/warehouse-cosmic "${files[0]}"
install -Dm755 target/release/warehouse-cosmic-cli "${files[1]}"
install -Dm644 resources/$app_id.svg "${files[3]}"
cp -a resources/icons/WarehouseCosmic "${files[4]}"
install -Dm644 resources/$app_id.metainfo.xml "${files[5]}"
mkdir -p "$install_prefix/share/applications"
sed "s@^Exec=.*@Exec=\"$install_prefix/bin/warehouse-cosmic\" %F@" resources/$app_id.desktop > "${files[2]}"
if command -v desktop-file-validate > /dev/null; then desktop-file-validate "${files[2]}"; fi
if command -v update-desktop-database > /dev/null; then update-desktop-database "$install_prefix/share/applications"; fi
if command -v kbuildsycoca6 > /dev/null; then kbuildsycoca6 --noincremental; fi
printf 'Installed Warehouse COSMIC and its standalone CLI.\n'
printf 'Run: %s\nCLI: %s --help\n' "${files[0]}" "${files[1]}"
