# Development

## Prerequisites

The Rust toolchain version is pinned in `rust-toolchain.toml`, and every crate and the libcosmic revision are locked in `Cargo.lock`. Native builds also need a C compiler, pkg-config, fontconfig/freetype, xkbcommon, Wayland, Flatpak, and a Vulkan or OpenGL driver for GPU rendering. A desktop file-chooser portal (COSMIC or KDE) is used for Open Files. System package changes use PolicyKit through `pkexec`.

## Run and build

```sh
cargo run --locked --bin warehouse-cosmic
```

The default build uses native Wayland and wgpu; X11 is not enabled. A software-rendered GUI build:

```sh
cargo build --locked --no-default-features --features gui --bin warehouse-cosmic
```

A CLI-only build without libcosmic or a renderer:

```sh
cargo build --locked --no-default-features --bin warehouse-cosmic-cli
```

Optional desktop test helpers for real pointer input, window activation and placement are never part of the app. On COSMIC, build `cargo build --features cosmic-tools --bin warehouse-cosmic-cosmic` (see [GUI CLI](ui-cli.md#if-youre-on-cosmic)); on KDE Plasma, `cargo build --features kde-tools --bin warehouse-cosmic-kde` (see [GUI CLI](ui-cli.md#if-youre-on-kde-plasma)).

For the Flatpak, see [Flatpak distribution](flatpak.md): `flatpak/build.sh --dev` for quick local builds, `flatpak/build.sh` for a release bundle.

## Native install

`install.sh` builds and installs both binaries, the desktop entry and the icon for the current user; `install.sh --dry-run` lists the destinations. Replaced files are backed up next to themselves with a timestamp. `uninstall.sh` shows what it would remove, and `uninstall.sh --apply` removes only this app's installed files. User data, snapshots and operation reports remain.

## Where data lives

Snapshots and operation reports are under `$XDG_DATA_HOME/warehouse-cosmic/` (for the Flatpak, `~/.var/app/dev.nilltadios.WarehouseCosmic/data/warehouse-cosmic/`). Operations are serialized across GUI and CLI with a file lock, and each one writes a report with its commands, errors and output. The official Warehouse snapshot library is listed read-only; import a copy to rename or trash it here.

## Tests

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
./verification/backend-smoke.sh
./verification/verify-cli.sh
```

The backend smoke test builds a signed local Flatpak repository and isolates HOME and every XDG folder, so it never touches your real installation. See [verification](verification.md) for the packaged GUI checks and their limits.

## Credits

The CLI's discovery and scripting conventions were informed by Craft's [GridCraft CLI](https://github.com/storytold/gridcraft/blob/11bb67386c9bb19b0717e21e98adc608162ae2e5/docs/cli.md) and [DeckCraft command documentation](https://github.com/storytold/deckcraft/blob/55e5ee56b4b2c64973b78fda4182a78f9766fa62/apps/deckcraft-cli/src/help.rs). The bundled fallback icons are from [COSMIC Icons](https://github.com/pop-os/cosmic-icons) (CC-BY-SA-4.0).
