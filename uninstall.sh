#!/usr/bin/env bash
set -euo pipefail
install_prefix="${HOME}/.local"
app_id=dev.nilltadios.WarehouseCosmic
files=("$install_prefix/bin/warehouse-cosmic" "$install_prefix/bin/warehouse-cosmic-cli" "$install_prefix/share/applications/$app_id.desktop" "$install_prefix/share/icons/hicolor/scalable/apps/$app_id.svg" "$install_prefix/share/icons/WarehouseCosmic" "$install_prefix/share/metainfo/$app_id.metainfo.xml")
printf 'Remove these installed application files:\n'
printf '%s\n' "${files[@]}"
if [[ "${1:-}" != --apply ]]; then printf 'Dry run. Add --apply to remove them. App data and backups remain.\n'; exit 0; fi
stamp="$(date +%Y%m%d-%H%M%S)"
for path in "${files[@]}"; do if [[ -e "$path" || -L "$path" ]]; then mv -- "$path" "$path.uninstalled-$stamp"; fi; done
if command -v update-desktop-database > /dev/null; then update-desktop-database "$install_prefix/share/applications"; fi
if command -v kbuildsycoca6 > /dev/null; then kbuildsycoca6 --noincremental; fi
printf 'Removed from the launcher/PATH. Restore the .uninstalled-%s files to undo.\n' "$stamp"
