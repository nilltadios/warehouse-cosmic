# Warehouse COSMIC project brief

Warehouse COSMIC is a native Rust and libcosmic Flatpak toolbox based on the workflows in [flattool/Warehouse](https://github.com/flattool/warehouse). The goal is to keep Warehouse's useful package, remote, app-data and snapshot management while fitting a COSMIC application and working well on this KDE Wayland desktop.

## Intended product

- Implement the application in Rust with libcosmic. Use native Wayland by default and GPU rendering when available.
- Preserve Warehouse's main workflows: browse installed apps/runtimes, filter/search, inspect package information, install/uninstall, update, change source/version, manage update masks/runtime pins, manage remotes, clean up app data, and create/restore/manage snapshots.
- Use a new original red-and-black icon. Keep the application recognizable in its header, launcher and task manager.
- Distribute the app itself as a Flatpak. Include the GUI and CLI in that package. The existing Proton Authenticator COSMIC project was a reference; this project lives in its own sibling directory.
- Support custom signed Flatpak repositories, including the distribution model in `parsec-flatpak-autobuild`. A standalone bundle origin must not be mistaken for a working update repository.
- Keep dependencies current through explicit, recorded audits. Commit the lockfile and pin build inputs so another build uses the same versions. Publishing a signed repository requires a chosen public repository and signing configuration; preparing a workflow does not deploy it.
- Make the application usable by people and agents. Provide discoverable commands, typed JSON schemas, dry-run plans, durable operation reports and controls for testing the actual GUI. No MCP or separate background service is required.

## Interface requirements

The aim is a useful native adaptation rather than reproducing GTK widgets. Warehouse's layout remains the visual reference when it communicates a workflow more clearly.

| Area | Expected behavior |
|---|---|
| Header | Warehouse logo first, then sidebar toggle and refresh; visible icons, consistent button sizing, and tooltips below the buttons with a gap |
| Package list | Distinct hover and selection feedback, real exported app icons inside the Flatpak, and a separate detail pane |
| Package details | Related version/update/source actions stay together and wrap only when space requires it |
| Remotes | A centered column, compact grouped rows, disabled-remotes visibility control, per-row menus and package filters, and popular/custom/file additions |
| Local package installation | A compact file review with User/System choices and a clear Install action, without a large command dump |
| Opening files | Forward package files to the existing window and present its install review instead of creating another instance |
| Settings | A gear button opens Settings; theme choice belongs under Appearance and persists separately from About |
| Cards and reports | Long identifiers stay within their cards; reports fit their content and place dismissal at the top right; scrollbars have their own gutter |
| User Data filters | The selected Active Data or Leftover Data button has an accent background and checkmark |
| Sidebar icons | Snapshots uses a history/restore icon; Install Packages uses a download icon |
| Window | Restore the last normal size and windowed/maximized/fullscreen mode after closing and reopening |
| Errors | Report actual failures and partial results. Do not claim an operation succeeded just because a command returned or a screenshot file exists |

## Host integration and data

The Flatpak uses the host Flatpak tool for package transactions and authorization. It needs access to package metadata, exported icons and managed app-data directories. Shared `~/.config/cosmic` is explicitly read-only; this app's theme and window preferences use its private config directory.

App-data removal moves data to the desktop Trash and verifies the original path disappeared. It must not fall back to permanent deletion. Snapshots preserve links without following them, reject unsafe archives and retain recovery data during restoration. Snapshotting the manager's own data excludes its snapshot library.

## How changes are accepted

Source compilation is an initial check. The distributed Flatpak must also be built, installed and exercised. GUI launches and captures use the Flatpak, because a native development binary does not test sandbox permissions, icon access, the runtime or host integration.

Test and screenshot instances close afterward. A window opened for someone to use stays open. Existing user windows are preserved unless their interruption was explicitly authorized. Native input tests are scoped to the selected app instance.

See [verification](verification.md) for reproducible commands and limits, [GUI CLI](ui-cli.md) for live UI controls, [feature mapping](parity.md) for remaining differences, [remotes](remotes.md), [desktop behavior](desktop-behavior.md), and [Flatpak distribution](flatpak.md).
