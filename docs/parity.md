# Feature mapping

The reference is Warehouse 2.2.0 as installed on this machine and upstream source commit `007cd801b22c8b5c84fa97cac5634a2bcc877f6c`. Source fetch scripts and verified reference captures are in `/home/nilllinux22/Documents/warehouse-cosmic-tests/2026-10-27/warehouse-cosmic/`.

| Warehouse workflow | Warehouse COSMIC |
|---|---|
| Packages sidebar, list and details | Same three-column organization with libcosmic widgets and exported Flatpak icons |
| Search and filters | Name/ID/ref text, applications/runtimes, remote, installation, runtime, end-of-life state, sort order |
| Package properties and copying | List fields and Flatpak info rows, individual copying, launch command copying |
| Check updates and change distribution source | Read installed/remote commits, update the current origin, or reinstall from another remote in the same installation |
| Change version and hold updates | Remote commit history, commit field, update mask option |
| Runtime pinning and update masking | Explicit exact-scope operations; wildcard removals are shown in confirmation |
| Batch uninstall and data removal | Selection mode, ID copying, snapshot creation, uninstall and Trash operations |
| Active and leftover data | Separate views with responsive cards and folder/copy/snapshot/Trash actions |
| Matching installation for leftover data | Populate the package search with the app ID |
| Snapshot management | Create, restore, import/export, rename, Trash, batch restore/Trash, install matching IDs |
| Existing Warehouse snapshots | Read the official library; import a copy before renaming or trashing it here |
| Remotes | Compact grouped rows, show/hide disabled, exact-installation package filter, details/action menu, popular presets, URL/repo-file addition, enable/disable/remove and copying |
| Install | Search an enabled remote, queue results, install multiple local bundles/ref files |
| Cancellation and errors | Cancel owned Flatpak transactions; retained partial results and durable reports |
| Keyboard access | Ctrl+R/F/P/M/D/S/I/B/A/T/O, F5 and Escape; native widget keyboard navigation |

This is a native implementation of the main workflows, not a pixel-identical GTK port. It uses COSMIC's controls, spacing and theme. Files open through the native chooser or command line; drag-and-drop installation and translated UI catalogs are not implemented. Version selection currently uses the commit field beside the displayed history. Filter choices last for the current session. The Settings panel saves the theme choice between launches.
