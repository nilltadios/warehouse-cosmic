# Feature mapping

The reference is Warehouse 2.2.0 (`io.github.flattool.Warehouse`, Flathub). The comparison below was rechecked on 2026-10-28 against that release's installed Python sources (`share/warehouse/src`), not only against screenshots.

| Warehouse workflow | Warehouse COSMIC |
|---|---|
| Packages sidebar, list and details | Same three-column organization with libcosmic widgets and exported Flatpak icons |
| Search | Name, ID, branch, remote and version text |
| Filters (apps/runtimes, remotes, runtimes) | Applications/Runtimes/End of life, an installation choice, multi-select remotes (each with its installation) and multi-select runtimes, Name/App ID/Remote sort, Reset Filters. Saved between launches like Warehouse's GSettings |
| Remembered page and window | Last page, filters and sort orders in `view-state.json`; window size and mode in `window-state.json` |
| Properties and copying | List fields and `flatpak info` rows, individual copying, Copy Launch Command |
| Runtime row opens the runtime | Show beside the Runtime row selects that runtime package |
| End-of-life notice | Warning banner with the publisher's reason |
| Open, Uninstall | Open launches the app; Uninstall opens an app-specific confirmation with Keep (default) or Trash for settings and content |
| Reinstall | Reinstall… from the package's current origin |
| Check updates and change source | Read installed/remote commits, update from the origin, or reinstall from another remote in the same installation |
| Change version | Radio list of the remote's commit history (subject, date, installed marker), a paste field fallback and the block-updates option |
| Disable updates, disable autoremoval | Block automatic updates (mask) and Pin runtime, with wildcard removals shown in the confirmation |
| Batch selection | Select mode, Select All, Copy Names/IDs/Refs, Update, Create Snapshot, Trash Data, Uninstall; Delete/Backspace acts on the selection after confirmation |
| Active and leftover data | Compact Active/Leftover toggles, Name/ID/Size sort with direction, responsive cards with copy/open/snapshot/Trash, total size |
| Install a leftover app | Find in a Remote opens the remote search with the app ID and searches it |
| Snapshots | Grouped by app with installed state; restore, export, rename, Trash, Install App, New Snapshot per app; batch restore/Trash/install |
| New snapshot dialog | Name field, app search, icons/names/IDs, selection markers, Select All, selected count, Create enabled only when valid |
| Existing Warehouse snapshots | Read the official library; import a copy before renaming or trashing it here |
| Remotes | Compact grouped rows, show/hide disabled, package filter, details/action menu (including Search packages), popular presets with Added state, URL/repo-file addition, enable/disable/remove and copying |
| Install from a remote | Search in a Remote section with one row per enabled remote and its installation, a per-remote search page with Installed/Add/Remove per result |
| Pending packages | A pending pane that explains itself when empty, groups queued packages by remote and installation, removes single items or all, and installs everything in one reviewed operation |
| Install local files | Open Files… or drag and drop `.flatpak`/`.flatpakref` onto the window; review with User/System choice |
| Drop a `.flatpakrepo` | Opens Add Remote for that file; several repo files or mixed types are refused with an explanation |
| Unsupported drops | Explained in a dialog, as in Warehouse. Additionally a `.tar.zst` snapshot opens Import Snapshot |
| Cancellation and errors | Cancel owned Flatpak transactions; retained partial results and durable reports; toasts for copy and finished operations |
| Keyboard access | Ctrl+P/M/D/S/I pages, Ctrl+F search, Ctrl+T filters, Ctrl+B select, Ctrl+A select all, Delete/Backspace in selection, Ctrl+O open files, Ctrl+R/F5 refresh, Ctrl+, settings, Escape back/close |

## Additions beyond Warehouse

- A headless CLI with typed schemas, dry-run plans and durable reports (`warehouse-cosmic-cli guide`).
- A live GUI control surface: every visible control is listed with a label and the authorization it needs, and the page content and dialog contents are readable as JSON (`ui inspect`). See [GUI CLI](ui-cli.md).
- Pending installs may mix remotes and installations; each group keeps its own remote and scope.
- Snapshot archives can be opened or dropped to import them.

## Remaining differences

This is a native implementation, not a pixel-identical GTK port. It uses COSMIC's controls, spacing and theme.

- Translated UI catalogs are not provided; the interface is English only.
- Dropping onto the sidebar is not handled; drop onto the page content area (the whole window apart from the navigation sidebar).
- Warehouse's right-click/long-press to start selection is replaced by the Select button and Ctrl+B.
- Remote search uses `flatpak remote-ls` for the chosen remote and matches name or ref text locally, as Warehouse does; results show a generic icon because remote icons are not downloaded.
