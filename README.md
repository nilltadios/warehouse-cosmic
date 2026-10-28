# Warehouse COSMIC

A native Rust/libcosmic Flatpak toolbox inspired by [flattool/Warehouse](https://github.com/flattool/warehouse). It has the same five main work areas: Packages, Remotes, User Data, Snapshots, and Install Packages. The red-and-black warehouse icon is original SVG artwork in `resources/`.

The application code is Rust. There is no Python, GTK, web view, or JavaScript application runtime. Flatpak handles package transactions; libcosmic and the Linux graphics stack provide the desktop integration. Compression uses the Rust zstd wrapper and its native zstd dependency.

## Run

The distribution package uses **Freedesktop Platform 26.08**, built with **Freedesktop SDK 26.08** and a checksummed Rust 1.99.1 build toolchain. The build toolchain is removed from the shipped app. Both the GUI and the standalone CLI are included.

Build a Flatpak locally with `flatpak/build.sh`. It creates `flatpak/out/dev.nilltadios.WarehouseCosmic.flatpak` without installing it. See [Flatpak distribution](docs/flatpak.md) for installation, signed GitHub Pages publishing, updates, permissions, and packaged CLI commands. The repository publishing workflow is prepared locally; this does not mean a public repository has been deployed.

For native development:

```sh
cargo run --locked --bin warehouse-cosmic
```

The default build enables native Wayland and wgpu. X11 support is excluded from the libcosmic feature selection. wgpu tries the available graphics adapters; iced includes its software renderer as a fallback. A software-only GUI build is available with `cargo build --locked --no-default-features --features gui --bin warehouse-cosmic`.

Rust 1.99.1 is pinned in `rust-toolchain.toml`. The libcosmic revision and all crate versions are recorded in `Cargo.lock`. Native prerequisites are a C compiler, pkg-config, fontconfig/freetype, xkbcommon, Wayland, Flatpak, and a working Vulkan or OpenGL driver for GPU rendering. KDE/COSMIC provides file chooser portals. System package changes use PolicyKit through `pkexec`.

The private icon theme bundles COSMIC icons, including the sidebar toggle, so controls remain visible on KDE and inside the Freedesktop runtime. The original red/black app icon has separate header padding. Scrollbars reserve a content gutter, and Remotes/Snapshots toolbars stay outside their scrolling lists.

The Settings gear opens a side panel. Appearance → Theme offers Match desktop, Dark and Light. The choice is saved automatically and loaded on the next launch. Ctrl+, opens Settings; Escape closes the panel.

## Features

- Search installed applications and runtimes; filter by remote, runtime, installation and end-of-life state.
- Copy package fields, launch applications, check/apply updates from the installed origin, change sources, block updates, pin runtimes, and change versions using remote commit history.
- Uninstall packages individually or in batches, with an option to move shared app data to Trash.
- Add remotes from URLs or `.flatpakrepo` files; enable, disable, and remove remotes.
- Inspect active and leftover user data, open folders, copy IDs, make snapshots, and move data to Trash.
- Create, import, export, rename, restore, and trash Warehouse-compatible `.tar.zst` snapshots. Batch restores require one snapshot per app.
- Search an enabled remote, queue packages, and install multiple `.flatpak` or `.flatpakref` files.

Snapshots created here live under `$XDG_DATA_HOME/warehouse-cosmic/Snapshots` (normally `~/.local/share/warehouse-cosmic/Snapshots`). The official Warehouse snapshot library is also visible; import copies to edit them here. Restores validate and extract the archive before replacing data. The previous data is retained in a sibling `.warehouse-recovery-*` folder under `~/.var/app`; the result names its exact path. Close the affected application yourself before snapshotting, restoring, or removing its data.

Operations are serialized across GUI and CLI invocations with a file lock. Results, commands, errors and recent output are saved under `$XDG_DATA_HOME/warehouse-cosmic/operations`. Cancellation preserves completed steps and reports partial results. Flatpak operations are not a multi-command transaction.

## Headless CLI for people and agents

```sh
warehouse-cosmic-cli commands --json
warehouse-cosmic-cli describe package.uninstall
warehouse-cosmic-cli run --cmd 'package.list={"kind":"applications"}'
warehouse-cosmic-cli run --script examples/audit.json
```

Backend commands start no GUI, MCP server, daemon, or listening socket. Mutations produce plans by default. Add `--apply` to perform them. System mutations also require `--allow-system`; this can show a PolicyKit authorization dialog. For a CLI build without libcosmic or a graphics renderer:

```sh
cargo build --locked --no-default-features --bin warehouse-cosmic-cli
```

Custom channels, including the Parsec autobuild distribution, are covered in [custom remotes](docs/custom-remotes.md).

See [the CLI guide](docs/cli.md), [agent workflow](docs/agents.md), [command catalog](schemas/commands.json), and [request schema](schemas/request.json). The discovery and scripting conventions were informed by Craft's [GridCraft CLI](https://github.com/storytold/gridcraft/blob/11bb67386c9bb19b0717e21e98adc608162ae2e5/docs/cli.md) and [DeckCraft command documentation](https://github.com/storytold/deckcraft/blob/55e5ee56b4b2c64973b78fda4182a78f9766fa62/apps/deckcraft-cli/src/help.rs).

The [project brief](docs/project-brief.md) records the intended product and interface requirements. The [testing guide](docs/verification.md) describes reproducible checks and their limits. For live GUI inspection, navigation, file review, theme/window changes, hover/scroll tests and renderer captures, use the [GUI CLI](docs/ui-cli.md).

See [deferred issues](docs/known-issues.md) for reported problems, saved evidence and future reproduction steps. These reports do not authorize implementation until requested.

## Install and remove

`install.sh` builds and installs both binaries, the desktop entry, and the icon for the current user. `install.sh --dry-run` lists destinations. Replaced files are backed up next to themselves with a timestamp. `uninstall.sh` removes only this application's installed files after showing the plan; `uninstall.sh --apply` performs it. User data, snapshots, and operation reports remain.

## Verify

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
./verification/backend-smoke.sh
./verification/verify-cli.sh
```

The backend smoke test generates a signed local Flatpak repository and isolates HOME and every XDG folder before installing anything. It never installs fixtures into the real user installation. Desktop captures come from the application's own renderer with `--preview PAGE --capture /absolute/output.png`; that process exits after saving the image. See [verification](docs/verification.md) for results and limitations and [feature mapping](docs/parity.md) for UI differences.

GPL-3.0-or-later. This is an independent application and is not maintained by the Warehouse developers.
