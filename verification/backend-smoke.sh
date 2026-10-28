#!/usr/bin/env bash
# Generate a signed, offline Flatpak repository and test only an isolated user installation.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
project="$PWD"
mkdir -p verification
fixture="$(mktemp -d "$project/verification/fixture.XXXXXX")"
printf '%s\n' "$fixture" > verification/last-fixture.txt
mkdir -p "$fixture"/{home,data,config,cache,gnupg,app/files/bin,app/export/share/applications,runtime/files,runtime/usr,runtime/var,app/var}
chmod 700 "$fixture/gnupg"
touch "$fixture/.warehouse-test-root"
export GNUPGHOME="$fixture/gnupg"
trap 'gpgconf --homedir "$GNUPGHOME" --kill gpg-agent || true' EXIT
gpg --batch --pinentry-mode loopback --passphrase '' --quick-generate-key 'Warehouse Fixture <fixture@example.invalid>' ed25519 sign 0
fixture_key="$(gpg --batch --with-colons --list-keys | awk -F: '$1=="fpr" {print $10;exit}')"
cat > "$fixture/runtime/metadata" <<'META'
[Runtime]
name=org.example.WarehousePlatform
runtime=org.example.WarehousePlatform/x86_64/stable
sdk=org.example.WarehousePlatform/x86_64/stable
META
printf '%s\n' 'Offline integration fixture' > "$fixture/runtime/usr/fixture.txt"
flatpak build-export --disable-sandbox --runtime --gpg-sign="$fixture_key" "$fixture/repo" "$fixture/runtime" stable
cat > "$fixture/app/metadata" <<'META'
[Application]
name=org.example.WarehouseFixture
runtime=org.example.WarehousePlatform/x86_64/stable
sdk=org.example.WarehousePlatform/x86_64/stable
command=fixture
META
cat > "$fixture/app/export/share/applications/org.example.WarehouseFixture.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Warehouse Fixture
Exec=fixture
DESKTOP
printf '%s\n' '#!/bin/sh' 'exit 0' > "$fixture/app/files/bin/fixture"
chmod +x "$fixture/app/files/bin/fixture"
printf '%s\n' 'version one' > "$fixture/app/files/version"
flatpak build-export --disable-sandbox --subject='Version one' --gpg-sign="$fixture_key" "$fixture/repo" "$fixture/app" stable
ostree --repo="$fixture/repo" rev-parse app/org.example.WarehouseFixture/x86_64/stable > "$fixture/first-commit"
printf '%s\n' 'version two' > "$fixture/app/files/version"
flatpak build-export --disable-sandbox --subject='Version two' --gpg-sign="$fixture_key" "$fixture/repo" "$fixture/app" stable
flatpak build-update-repo --no-update-appstream --gpg-sign="$fixture_key" --title='Warehouse test' "$fixture/repo"
{
  printf '[Flatpak Repo]\nTitle=Warehouse test\nUrl=file://%s/repo\nGPGKey=' "$fixture"
  gpg --batch --export "$fixture_key" | base64 -w0
  printf '\n'
} > "$fixture/test.flatpakrepo"
{
  printf '[Flatpak Ref]\nTitle=Warehouse Fixture\nName=org.example.WarehouseFixture\nBranch=stable\nUrl=file://%s/repo\nSuggestRemoteName=warehouse-test\nIsRuntime=false\nGPGKey=' "$fixture"
  gpg --batch --export "$fixture_key" | base64 -w0
  printf '\n'
} > "$fixture/test.flatpakref"
flatpak build-bundle "$fixture/repo" "$fixture/test.flatpak" org.example.WarehouseFixture stable
env HOME="$fixture/home" XDG_DATA_HOME="$fixture/data" XDG_CONFIG_HOME="$fixture/config" XDG_CACHE_HOME="$fixture/cache" "$project/target/debug/warehouse-cosmic" --smoke "$fixture"
printf 'Fixture and operation reports: %s\n' "$fixture"
