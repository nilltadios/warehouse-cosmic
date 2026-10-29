# Opening packages and restoring the window

Opening a `.flatpak` or `.flatpakref` file displays a compact review with filenames, file types and sizes. Choose User, System or a configured named installation before pressing Install. User is selected initially. Cancel leaves the installation unchanged. The review also handles multiple selected files and wraps long names. The agent CLI retains its structured command plans.

Dragging files onto the window's page area does the same as opening them. While a drag is over the window, a banner explains what each file type does. `.flatpak` and `.flatpakref` files open the install review, one `.flatpakrepo` file opens Add Remote, and one `.tar.zst` snapshot opens Import Snapshot. Unsupported, mixed or multiple remote/snapshot files are refused with an explanation, as in Warehouse. File managers provide `text/uri-list` paths; sandboxed senders use the document portal's file transfer, whose files the app reads through `/run/user/UID/doc`. Nothing is installed until the review is confirmed.

Normal launches share one application instance. Opening another package file forwards it to the existing window and presents its install review. Additional files join an open review without resetting the chosen installation. Files opened during a package operation wait until that operation finishes. Desktop activation tokens are forwarded to the running application so the compositor can present it.

Explicit file opens and `ui start` request standard Wayland activation (xdg-activation) for the window, the same on every desktop. Whether a minimized window comes forward is the compositor's decision. If you need to force it in a test, the repo-only helpers can: `warehouse-cosmic-cosmic present` on COSMIC and `warehouse-cosmic-kde present` on KDE Plasma (see [GUI CLI](ui-cli.md#if-youre-on-cosmic)).

`ui start` reuses that normal instance. Tests can explicitly start a separate instance with `ui start --config-dir PATH`; the config must include the shared COSMIC theme directory. Renderer captures are also isolated.

Warehouse COSMIC remembers its last normal window size and whether it closed windowed, maximized or fullscreen. Maximized/fullscreen dimensions do not replace the normal size. A minimized window reopens visibly. The compositor decides window position and may constrain dimensions to fit the current monitor.

Window state is saved atomically on a normal close in `~/.var/app/dev.nilltadios.WarehouseCosmic/config/warehouse-cosmic/window-state.json`. A changed previous file is retained with a timestamped backup. Theme preferences remain in their separate file. Capture instances use predictable dimensions and do not save over window preferences.

If you're testing on KDE Plasma, you can also run these KWin-based checks. Run `uv run --python 3.15 verification/check-install-review-flatpak.py` for actual bundle-review captures and unchanged-host-state checks. Run `uv run --python 3.15 verification/check-window-state-flatpak.py` for real close/reopen cycles. The latter isolates config while preserving shared COSMIC themes, checks the fixture's process environment before changing any KWin window, and closes only its own test windows.

Run `uv run --python 3.15 verification/check-single-instance-flatpak.py` to open files through a second Flatpak launch and verify that the original window and GUI identity receive them. It uses a private configuration and closes only the instance it started.
