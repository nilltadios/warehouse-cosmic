# Flatpak distribution

Warehouse COSMIC ships as `dev.nilltadios.WarehouseCosmic`, branch `stable`, initially for x86_64. The runtime is `org.freedesktop.Platform//26.08`. The SDK is `org.freedesktop.Sdk//26.08`. Rust 1.99.1 is a build tool and is removed before export. libcosmic, wgpu and the application code are compiled into the binaries; there is no separate COSMIC runtime to install.

## Build and install locally

Install the build prerequisites once:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.freedesktop.Platform//26.08 org.freedesktop.Sdk//26.08
```

The host needs `flatpak-builder`, `uv`, Cargo and Rust 1.99.1. The prepare script fetches Cargo.lock sources, vendors their exact versions, saves the official Rust checksum, and creates a source archive with its SHA-256. The build then runs offline inside the SDK; it does not package a host-built binary.

```sh
flatpak/build.sh
flatpak install --user flatpak/out/dev.nilltadios.WarehouseCosmic.flatpak
flatpak run dev.nilltadios.WarehouseCosmic
```

For this workstation, `flatpak/install.sh` previews installing the local bundle and switching the existing GUI/CLI launch commands to Flatpak. `flatpak/install.sh --apply` performs that plan, backs up native executables and their desktop entry with timestamps, and refreshes the launcher. It preserves app data. To undo, uninstall the Flatpak without `--delete-data`, remove the two launch wrappers, and restore the printed `.native-backup-*` files.

For local iteration, use the dev mode. It regenerates `flatpak/build-inputs-dev` from the working tree, builds with Cargo's `flatpak-dev` profile (release optimizations without LTO, incremental), and skips signing, static deltas and the bundle. `--install` also installs the result for the current user:

```sh
flatpak/build.sh --dev
flatpak/build.sh --dev --install
```

Builds use the CPU count minus two parallel jobs (at least two). Set `BUILD_JOBS` to change that. Vendored crates are archived separately and cached in `flatpak/cache` by the `Cargo.lock` hash, so a source-only change re-archives only the app. `ci/prepare-flatpak.py` refuses to overwrite an input directory. To regenerate release inputs after changing source, rename `flatpak/build-inputs` first or pass a new output directory:

```sh
uv run --python 3.15 ci/prepare-flatpak.py flatpak/build-inputs-next
flatpak/build.sh flatpak/out flatpak/build-inputs-next
```

Cargo artifacts are cached in the builder's `/run/ccache` mount, outside the app. The build refreshes the app source timestamps before invoking Cargo so a cache cannot hide code changes from archives with normalized timestamps. Cargo.lock and its source checksums still control dependency reuse.

Local builds are unsigned unless `FLATPAK_SIGNING_KEY` names a key in `GNUPGHOME`. Public builds require the stable publisher signing key. Do not distribute the ephemeral verification key.

## Signed repository on GitHub Pages

The publishing model follows the Parsec autobuild project: an OSTree repository, a `.flatpakrepo` file, a one-click `.flatpakref`, and `state.json` are deployed together. The workflow builds on pushes to `main` and manual dispatches. It has no scheduled job. Each successful source build creates a new signed repository commit; installed users receive it through `flatpak update`, COSMIC Store, Discover or GNOME Software.

Maintainer setup:

1. Push this source to the intended GitHub repository with `main` as its default branch.
2. Add the stable signing key's ASCII-armored private half as the Actions secret `GPG_PRIVATE_KEY`. CI requires a signing key that can operate noninteractively. Keep its private half out of Git and retain a secure backup. Reuse the same key on subsequent builds.
3. Set repository Settings → Pages → Source to GitHub Actions. If using a custom domain or a nonstandard Pages path, set the repository variable `PAGES_BASE_URL` to its HTTPS directory URL.
4. Run "Build and publish Flatpak". The workflow builds in the Freedesktop 26.08 container, signs the commit and repository summary, verifies the commit signature, generates install files with the public key, uploads the bundle artifact, and deploys Pages.

The default URL is derived from the actual repository owner/name. No uncreated repository URL is embedded in the install files. `ci/publish.py` prepares local site files; GitHub's deploy job performs the public deployment.

Site outputs:

| Path | Purpose |
|---|---|
| `repo/` | Signed OSTree repository |
| `warehouse-cosmic.flatpakrepo` | Add the `warehouse-cosmic` remote |
| `dev.nilltadios.WarehouseCosmic.flatpakref` | Add the remote and install the app |
| `state.json` | Source commit, archive hash, version, runtime, compiler and published commit |
| `index.html` | Install instructions |

The published site is **https://nilltadios.github.io/warehouse-cosmic/** (repository `nilltadios/warehouse-cosmic`). Open its `.flatpakref` in COSMIC Store, Discover or GNOME Software; it supplies Flathub as `RuntimeRepo` so missing Freedesktop runtimes can be installed. In a terminal:

```sh
flatpak remote-add --user --if-not-exists warehouse-cosmic https://nilltadios.github.io/warehouse-cosmic/warehouse-cosmic.flatpakrepo
flatpak install --user warehouse-cosmic dev.nilltadios.WarehouseCosmic
flatpak update --user dev.nilltadios.WarehouseCosmic
```

To move a copy installed from a downloaded bundle onto the remote, keeping its data, run `flatpak install --user --reinstall warehouse-cosmic dev.nilltadios.WarehouseCosmic`. The signing key fingerprint is `E3CD 796A EED6 8D6D 4747  362A 4174 B10F A66F C268`; the README shows how to compare it with the key the site serves. Forks set their own `PAGES_BASE_URL` and key.

Commits marked `[skip ci]` are not built, so the published `source_commit` in `state.json` can lag behind `main`. Use `[skip ci]` only for changes that do not affect the shipped app; dependency updates should be built and published.

Before a new app release, update the Cargo version and AppStream release entry, check `verification/check-dependencies.py`, and regenerate inputs. Cargo.lock pins the shared libcosmic source, including its Git revision. The explicit Rust archive SHA-256 must match the version in `rust-toolchain.toml`; preparation checks this against the official checksum. The SDK Rust extension is not used because its 26.08 branch currently provides 1.98.0.

This workflow starts a fresh remote repository per build, as the Parsec publisher does. It does not promise remote history for old package rollback. Back up bundles if archived releases are needed. An app-data snapshot does not back up the package itself.

## Host access and app data

The GUI stays inside the Flatpak and uses Wayland only. GPU access is `--device=dri`; there is no X11 socket permission. The app includes its own fallback icon theme. Package queries, transactions, package launches and folder opening use `flatpak-spawn --host` with argument arrays. Transactions use `--watch-bus` so their helper exits when its bridge disappears. Launched apps are independent. System changes still go through host PolicyKit.

The app needs access to `org.freedesktop.Flatpak` and the home directory, including other apps' `~/.var/app` data, to perform its management functions. This is a powerful management application: Flatpak host-command access and home access together mean it is effectively unsandboxed. Every permission and its use:

| Permission | Used for |
|---|---|
| `--talk-name=org.freedesktop.Flatpak` | Run host `flatpak`, `pkexec`, `gio trash` and `xdg-open` through `flatpak-spawn --host` (package transactions, Trash, opening folders and launching apps) |
| `--filesystem=home`, `~/.var/app` | Read sizes of, snapshot, restore and trash other apps' data |
| `~/.local/share/flatpak:ro`, `/var/lib/flatpak:ro` | Read user and system installations and exported app icons |
| `--filesystem=host-etc` | Read `/etc/flatpak/installations.d` to discover configured named installations |
| `xdg-config/cosmic:ro`, `com.system76.CosmicSettingsDaemon` | Follow the COSMIC theme without writing desktop settings |
| `--own-name=dev.nilltadios.WarehouseCosmic.UI.*` | The live GUI control API used by `warehouse-cosmic-cli ui` |
| `--talk-name=org.kde.KWin` | Optional on KDE: raise the window when files are opened, place test windows on a monitor and deliver test pointer input (`ui hover`/`ui scroll`). Unused elsewhere |
| `--socket=wayland`, `--device=dri`, `--share=ipc` | Native Wayland window with GPU rendering |

Shared COSMIC themes at `~/.config/cosmic` are explicitly read-only through `--filesystem=xdg-config/cosmic:ro`. This narrower grant overrides the broad home permission for that directory and also exposes it under the app's sandbox config path. Warehouse's own preferences remain in a separate writable directory. Theme choices affect this app rather than rewriting desktop theme files.

Trash operations use the host's `gio trash` through the same host bridge. This uses the desktop's Trash instead of a private Trash under the Flatpak's XDG data directory. The host therefore needs the `gio` command as well as Flatpak. A missing helper, permission denial, or folder still present after the operation is reported as an error. There is no permanent-delete fallback. The native development build uses the Rust Trash implementation directly.

The Flatpak's own snapshots and reports normally live in `~/.var/app/dev.nilltadios.WarehouseCosmic/data/warehouse-cosmic/`. A previous native install's library stays at `~/.local/share/warehouse-cosmic/`; import its snapshot archives to use them in the Flatpak. Existing official Warehouse snapshots are also listed read-only.

The Settings gear opens a native side panel. Its Appearance section saves the Theme choice in `~/.var/app/dev.nilltadios.WarehouseCosmic/config/warehouse-cosmic/preferences.json`. Window size and mode are saved in `window-state.json` in the same folder. Changes are written atomically, and the previous file is retained with a timestamped `.bak-*` suffix. The theme preference is loaded before the window opens. Settings contains application preferences; About contains version and license information. Filters, sort orders and the last page are saved on a normal close in `view-state.json` beside it; capture instances never write it.

The packaged standalone CLI starts no GUI or server:

```sh
flatpak run --command=warehouse-cosmic-cli dev.nilltadios.WarehouseCosmic commands --json
flatpak run --command=warehouse-cosmic-cli dev.nilltadios.WarehouseCosmic run --script /absolute/path/audit.json
```

Agent commands retain their dry-run default, explicit `--apply`, structured errors, and path-root options. GUI and CLI copies inside the same package share their operation lock. A concurrently installed native copy has a separate data/lock directory; avoid managing the same installation simultaneously through both distributions.

## Remove or switch back

```sh
flatpak uninstall --user dev.nilltadios.WarehouseCosmic
flatpak remote-delete --user warehouse-cosmic
```

Leave out `--delete-data` to retain snapshots and reports. The native binaries can remain installed independently. Public signing-key and GitHub repository setup are separate from the local build and test result.
