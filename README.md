# Warehouse COSMIC

Manage your Flatpak apps, remotes and app data from one native COSMIC app. Inspired by [Warehouse](https://github.com/flattool/warehouse), rewritten in Rust with libcosmic, and usable by scripts and AI agents through a command-line tool.

## Install

**One click:** open [the install file](https://nilltadios.github.io/warehouse-cosmic/dev.nilltadios.WarehouseCosmic.flatpakref) in Discover or GNOME Software.

**Terminal:**

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists warehouse-cosmic https://nilltadios.github.io/warehouse-cosmic/warehouse-cosmic.flatpakrepo
flatpak install --user warehouse-cosmic dev.nilltadios.WarehouseCosmic
```

Updates arrive through `flatpak update`, Discover or GNOME Software like any other app.

**Installed it from a downloaded `.flatpak` file?** That copy never updates. Add the remote (second command above), then switch to it. Your settings and data are kept:

```sh
flatpak install --user --reinstall warehouse-cosmic dev.nilltadios.WarehouseCosmic
```

Use `--system` instead of `--user` in every command if you installed it system-wide.

**Signing key fingerprint:** `E3CD 796A EED6 8D6D 4747  362A 4174 B10F A66F C268`. To compare it with the key the site serves:

```sh
curl -s https://nilltadios.github.io/warehouse-cosmic/warehouse-cosmic.flatpakrepo | sed -n 's/^GPGKey=//p' | base64 -d | gpg --show-keys
```

A Flatpak manager needs broad access to your Flatpak installations and app data; [here's what each permission is for](docs/flatpak.md#host-access-and-app-data).

## What it does

- **Packages:** search and filter installed apps and runtimes, see their details, open, update, reinstall, change source or version, block updates, pin runtimes, and uninstall one or many at once. Uninstalling asks whether to keep or trash the app's data.
- **Remotes:** add remotes from popular presets, URLs or `.flatpakrepo` files; enable, disable, remove, and browse what's installed from each.
- **User data:** see how much space each app's data uses, find data left behind by apps you uninstalled, and move it to the Trash.
- **Snapshots:** back up an app's settings and content, and restore, export or import them. Warehouse snapshots work here too.
- **Install:** search a remote, collect packages from several remotes into one pending list, and install them together. You can also open or drag `.flatpak`, `.flatpakref` and `.flatpakrepo` files onto the window.

Nothing is deleted permanently: removed data goes to your desktop Trash, and restoring a snapshot keeps your previous data in a recovery folder.

## Command line and AI agents

The app ships with `warehouse-cosmic-cli`, which can do everything without opening a window, and can also inspect and click through a running window.

```sh
flatpak run --command=warehouse-cosmic-cli dev.nilltadios.WarehouseCosmic guide
```

`guide` is the starting point. Changes are shown as a plan first and only happen with `--apply`. See the [CLI guide](docs/cli.md), the [agent workflow](docs/agents.md) and the [live GUI controls](docs/ui-cli.md).

## Build from source

```sh
cargo run --locked --bin warehouse-cosmic
```

To build the Flatpak, use `flatpak/build.sh`, or `flatpak/build.sh --dev` for faster local builds. See [development](docs/development.md) for prerequisites and tests, and [Flatpak distribution](docs/flatpak.md) for packaging and publishing.

## More

- [Feature comparison with Warehouse](docs/parity.md)
- [Custom remotes, including auto-built repositories](docs/custom-remotes.md)
- [Reported issues and their fixes](docs/known-issues.md)

## License

GPL-3.0-or-later. This is an independent project and isn't maintained by the Warehouse developers.
