#!/usr/bin/env bash
# Install the local package and make launchers use Flatpak. Default: dry run.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
app=dev.nilltadios.WarehouseCosmic
bundle="$PWD/flatpak/out/$app.flatpak"
prefix="$HOME/.local"
printf 'Install local Flatpak: %s\n' "$bundle"
printf 'Replace GUI and CLI commands with Flatpak launch wrappers, preserving timestamped backups.\n'
printf 'Retire native desktop/icon metadata so Flatpak exports are used. App data is retained.\n'
if [[ "${1:-}" != --apply ]]; then printf 'Dry run. Use --apply to install.\n'; exit 0; fi
test -f "$bundle"
flatpak install --user --bundle --noninteractive -y "$bundle"
stamp="$(date +%Y%m%d-%H%M%S)"
mkdir -p "$prefix/bin"
for name in warehouse-cosmic warehouse-cosmic-cli; do
  dest="$prefix/bin/$name"
  if [[ -e "$dest" || -L "$dest" ]]; then mv -- "$dest" "$dest.native-backup-$stamp"; fi
  if [[ "$name" == warehouse-cosmic ]]; then
    printf '#!/bin/sh\nexec flatpak run %s "$@"\n' "$app" > "$dest"
  else
    printf '#!/bin/sh\nexec flatpak run --command=warehouse-cosmic-cli %s "$@"\n' "$app" > "$dest"
  fi
  chmod 755 "$dest"
done
for path in "$prefix/share/applications/$app.desktop" "$prefix/share/icons/hicolor/scalable/apps/$app.svg" "$prefix/share/metainfo/$app.metainfo.xml"; do
  if [[ -e "$path" || -L "$path" ]]; then mv -- "$path" "$path.native-backup-$stamp"; fi
done
if command -v update-desktop-database >/dev/null; then update-desktop-database "$prefix/share/applications"; fi
if command -v kbuildsycoca6 >/dev/null; then kbuildsycoca6 --noincremental; fi
printf 'Installed. Launch: flatpak run %s\n' "$app"
printf 'Rollback: uninstall the Flatpak without --delete-data, remove the two wrappers, and restore .native-backup-%s files.\n' "$stamp"
