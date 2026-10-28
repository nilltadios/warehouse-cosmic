# Verification

Run checks from the project root. Logs, fixtures and machine-readable reports are saved in `verification/`. A non-zero exit status is a failure.

## Rust and backend

```sh
cargo fmt --all --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
verification/backend-smoke.sh
verification/verify-cli.sh
```

The Rust suite covers parsing, installation scopes, argument handling, the host bridge, strict JSON requests, script validation, file roots, and snapshot integrity/recovery. Process-level CLI tests run without a display. The backend smoke script generates a signed local Flatpak repository and isolates HOME/XDG directories before applying package operations. It tests commits, updates, source switching, app-data preservation, snapshots and uninstall. The CLI script also checks dry-run/apply behavior, exit codes, locking and desktop Trash.

## Packaged application

```sh
flatpak/build.sh
flatpak/install.sh --apply
uv run --python 3.15 verification/verify-flatpak.py
uv run --python 3.15 verification/verify-installed-flatpak.py
uv run --python 3.15 verification/check-remotes-flatpak.py
verification/capture-ui.sh DP-1
```

The package uses Freedesktop Platform/SDK 26.08 and a checksummed Rust 1.99.1 build toolchain. Build tools are removed before export. The Flatpak is compiled inside the SDK with vendored, locked dependencies and no network access during Cargo compilation.

`verify-flatpak.py` installs into a private Flatpak directory and reuses the existing user runtime directory as its read-only system runtime source. It checks CLI discovery, actual host package queries, update plans, a deliberately denied host permission, disposable app-data/snapshot Trash operations, and GUI captures. Its app-data fixture has a unique ID and marker. Applied operations never target existing app data. Signing keys belong only to the verification fixture.

Run the normal-install check as well. A custom runtime installation can expose paths that the normal installation hides. `verify-installed-flatpak.py` checks the normal package's explicit read-only access to user Flatpak exports, real icon readability, host `gio`, the command catalog, runtime and installed commit. These workstation checks use the existing Parsec and ArtCraft installations as references; adapt those IDs on other machines.

`capture-ui.sh` always launches the installed Flatpak. Capture outputs go under the home task bundle so the sandbox can write them. Each preview exits after saving its own renderer output. Existing user windows are preserved. Inspect the images; file existence alone does not establish layout quality. A capture does not prove native file chooser, clipboard or pointer interactions.

`check-remotes-flatpak.py` captures the compact remote list, narrow window, action menu, disabled/search/empty states, details, preset review and package-filter navigation. It verifies that host remotes do not change, that the narrow render is smaller and that test instances exit. `check-popular-remotes.py` fetches the preset publishers' definitions and saves response data, URLs, times and hashes without configuring host remotes.

`--preview PAGE` only navigates views. `--capture /absolute/path.png` captures and exits. Paths must be accessible to the sandbox. External files selected through a portal and CLI paths granted with `flatpak run --filesystem=/chosen/directory` follow Flatpak's usual access rules.

## Publishing and dependencies

```sh
uv run --python 3.15 verification/test-publisher.py
uv run --python 3.15 verification/check-workflow.py
uv run --python 3.15 verification/check-dependencies.py
```

The publisher fixture rejects unsigned commits, verifies signatures, checks embedded keys/runtime references, preserves existing site directories and tests a client against the signed summary. It prepares local files without deploying a public site. Workflow lint uses an official actionlint release verified against its published checksum. The dependency audit records upstream URLs, retrieval time, versions, libcosmic HEAD and Cargo's compatible-update result.

## Current local result

`verification/flatpak-packaging/results.json` is the current packaging report. Recollect it with `uv run --python 3.15 verification/finalize-flatpak.py`. It combines the Rust test suite, Clippy, format/shell/workflow/AppStream checks, signed publisher fixture, packaged consumer tests and normal-install regression checks. Preference tests cover theme persistence, backups and invalid-file preservation. The installed commit must match the latest export. Captures verified native Wayland and AMD Radeon RX 7700S rendering through Vulkan.

Reports identify the installed commit and source archive. Check those identities before applying an earlier result to newer source. A native test pass does not establish that a later Flatpak was built or tested. During an update, new GUI controls remain pending packaged verification until their corresponding reports are regenerated.

| Requirement or report | Reproduction and evidence |
|---|---|
| Product goals and accepted UI behavior | `docs/project-brief.md` and `docs/parity.md` |
| Rust/backend and CLI contracts | `cargo test --locked`, `cargo clippy --all-targets --locked -- -D warnings`, isolated backend/CLI fixture scripts |
| Native Wayland, rendering and sandbox access | Packaged consumer logs, normal-install check and saved app renders |
| Remotes layout, menus, filters and additions | `check-remotes-flatpak.py`, `remotes-results.json`, saved normal/narrow/state captures |
| Preset provenance | `check-popular-remotes.py`, publisher responses and `popular-remotes-results.json` |
| Local bundle review without installation | `check-install-review-flatpak.py`, `install-review-results.json`, before/after host state and saved renders |
| Window size and mode persistence | `check-window-state-flatpak.py`, `window-state-results.json`, actual KWin observations and private preference readback |
| Opening files in the existing window | `check-single-instance-flatpak.py`, `single-instance-results.json`, installed desktop launcher, unchanged window and GUI identities |
| Theme persistence and Light/Dark rendering | Preference tests, `check-settings-flatpak.py`, `settings-results.json` and brightness comparison |
| Live GUI automation | `ui commands`, `ui schema`, exact-instance inspection, actual widget bounds, native pointer-delivery readback and renderer captures; see `docs/ui-cli.md` |

A result is complete only when its script succeeds and any required visual review is performed. Failures remain in the saved logs. A blocked compositor input request, an unreadable file, a leftover test window or a host-state mismatch must be reported as a failure rather than skipped silently.

Interactive system PolicyKit authorization and native file chooser/clipboard mouse interactions were not automatically exercised. User-level transactions were exercised against signed isolated repositories. The real Parsec publisher and installed app were inspected read-only; its extra-data install was not repeated. Remaining UI differences are listed in `docs/parity.md`.

The earlier native-development checkpoint remains in `verification/results.json` as historical evidence. Public GitHub Pages deployment still requires the chosen repository, a stable publisher signing key and Pages configuration; see `docs/flatpak.md`.
