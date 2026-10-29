# Warehouse COSMIC

The distributed application is the Flatpak `dev.nilltadios.WarehouseCosmic`, on Freedesktop Platform 26.08. Rust 1.99.1 is a build tool. Cargo.lock pins all source versions, including libcosmic.

## Launch and inspect

Use the installed Flatpak for desktop launches and UI tests:

```sh
flatpak run dev.nilltadios.WarehouseCosmic
flatpak run --command=warehouse-cosmic-cli dev.nilltadios.WarehouseCosmic commands --json
```

Iterate quickly, verify in the package:

- While changing UI, check renders with the native debug build in capture mode, which exits after saving: `cargo build --bins && target/debug/warehouse-cosmic --preview PAGE --capture /absolute/new.png` (`PAGE-narrow` for the minimum width). For live interaction use `target/debug/warehouse-cosmic-cli ui start --config-dir PRIVATE_DIR`, and close that instance afterwards.
- For Flatpak checks while iterating, use `flatpak/build.sh --dev` (about a minute after the first build); install it with `--install` only when installing is authorized.
- Before reporting a change as done, verify it in the Flatpak. A source build alone does not validate the Flatpak runtime, host bridge, permissions, exported icon or launcher. See `docs/flatpak.md` for release builds and the local bundle.
- Wait for long builds through background-task notifications instead of sleep-and-poll loops.

When opening the app for someone to use, leave that window open. Screenshot/test instances should use `--preview PAGE --capture /absolute/output.png`, which exits after rendering. Close only instances created for that test. Never interrupt an existing window or package transaction without authorization. Use the desktop-testing workflow and the requested monitor on machines that provide it.

## Universal app code

The app and its CLI must behave the same on every Wayland desktop. Use standard protocols and portals (xdg-activation, file chooser, document portal, D-Bus interfaces the app owns), not compositor- or desktop-specific APIs. Desktop-specific tooling (for example KWin pointer input) belongs in repo-only helpers behind a Cargo feature that the Flatpak never enables, such as `cosmic-tools` (`warehouse-cosmic-cosmic`) and `kde-tools` (`warehouse-cosmic-kde`), and docs present it as "if you're on COSMIC / KDE Plasma, you can also run …". Do not add desktop-specific Flatpak permissions.

## Agent CLI

Read `llms.txt`, `docs/cli.md` and `docs/agents.md`. Discover commands and parameter schemas before invoking mutations. Requests are typed JSON; mutations are dry-run plans unless `--apply` is passed. System mutations also require `--allow-system` and PolicyKit. No MCP server or application daemon is needed.

Package queries and transactions run on the host through `flatpak-spawn --host` when packaged. Keep arguments separate; never interpolate a shell command. Trash uses host `gio trash` and verifies the original path disappeared. Failures must remain visible. Never substitute permanent deletion.

## Verification

Read `docs/known-issues.md` for deferred reports and evidence before continuing related work. A documentation-only report does not authorize fixing, rebuilding or restarting the app.

Run tests and Clippy for Rust changes. `verification/backend-smoke.sh` and `verification/verify-cli.sh` apply changes only to isolated fixtures. `verification/test-publisher.py` checks signing and publication files. `verification/verify-flatpak.py` installs into a private directory, shares existing runtimes read-only, and checks the actual packaged CLI, host access, disposable desktop Trash operations, and short-lived GUI captures.

Keep reproducible commands, logs, inputs and reports in `verification/`. Publishing files locally does not deploy a public repository. GitHub Pages deployment needs the chosen repository, its signing secret, and Pages configuration. Do not publish private keys or verification fixtures.

## Releases

Every push to `main` publishes a signed build. Before pushing a change that ships in the app, bump the version: `version` in `Cargo.toml` and a new `<release version="…" date="…">` with a short description in `resources/dev.nilltadios.WarehouseCosmic.metainfo.xml`. The site and `state.json` read the version from those. The next bump is 0.2.0, covering everything published after 0.1.0 (Warehouse parity, the WC-001 to WC-006 fixes, the agent CLI and the new UI). Commits that don't change the shipped app may use `[skip ci]`. Commits carry no AI attribution or co-author trailers.
