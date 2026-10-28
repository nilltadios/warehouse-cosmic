# Controlling and testing the GUI

The `ui` commands control an actual running Warehouse COSMIC window. The GUI owns a desktop D-Bus interface for its lifetime. There is no MCP server, extra daemon or persistent test service. Backend package commands remain usable with the GUI closed.

For the installed Flatpak, prefix these commands with:

```sh
flatpak run --command=warehouse-cosmic-cli dev.nilltadios.WarehouseCosmic
```

Discover the interface and select an instance:

```sh
warehouse-cosmic-cli ui commands
warehouse-cosmic-cli ui schema
warehouse-cosmic-cli ui instances
warehouse-cosmic-cli ui start --monitor DP-1
warehouse-cosmic-cli ui --instance INSTANCE inspect
```

If several windows are running, commands require an exact instance. This prevents tests from silently selecting another window. `inspect` returns the current page, modal, theme, window state, pending files, busy/loading flags, available action IDs and the actual widget tree with rendered bounds. `tree` returns the same renderer inspection. `ui wait` waits for inventory/detail reads and owned operations to finish.

`ui start` presents the normal running instance when one exists. `ui start --config-dir PATH` explicitly starts a separate test instance with private preferences. Normal desktop launches and opening files reuse the main window.

## Reproduce a UI issue

```sh
warehouse-cosmic-cli ui --instance INSTANCE navigate remotes
warehouse-cosmic-cli ui --instance INSTANCE activate remotes.show-disabled
warehouse-cosmic-cli ui --instance INSTANCE fill search flathub
warehouse-cosmic-cli ui --instance INSTANCE capture /absolute/new-output.png
warehouse-cosmic-cli ui --instance INSTANCE open /absolute/package.flatpak
warehouse-cosmic-cli ui --instance INSTANCE activate install.scope:user
warehouse-cosmic-cli ui --instance INSTANCE activate dialog.cancel
warehouse-cosmic-cli ui --instance INSTANCE theme light
warehouse-cosmic-cli ui --instance INSTANCE window --mode maximized
warehouse-cosmic-cli ui --instance INSTANCE window --mode windowed --width 1100 --height 700
warehouse-cosmic-cli ui --instance INSTANCE navigate data
warehouse-cosmic-cli ui --instance INSTANCE activate data.leftover
warehouse-cosmic-cli ui --instance INSTANCE activate data.active
```

`activate`, also spelled `click`, invokes the same message handler as the named visible control. It tests GUI transitions and rendering rather than calling the backend behind the window. It is a semantic action, not a physical pointer click or keyboard typing test. Use the action IDs returned by the current inspection; unavailable controls are refused.

Opening files displays review without installing. Confirming installation or another package/data change requires `--apply`. System changes also require `--allow-system`. Closing the selected window requires `ui close --apply`. Capture output must be a new absolute path unless `--overwrite` is explicit.

## Native hover and scrolling

```sh
warehouse-cosmic-cli ui --instance INSTANCE hover header.settings --capture /absolute/settings-hover.png
warehouse-cosmic-cli ui --instance INSTANCE hover header.about --capture /absolute/about-hover.png
warehouse-cosmic-cli ui --instance INSTANCE scroll --target content --pixels 400 --capture /absolute/scrolled.png
```

These commands send real Wayland pointer input on supported KDE sessions. They find the selected Flatpak's own window, obtain current rendered target bounds and verify that pointer events reach the GUI. They fail if the compositor blocks input. A hover state is never substituted just to make a screenshot pass. With `--capture`, they save the app's actual render while hovered/scrolled and then restore the pointer. Without capture, the pointer stays at the requested control.

Native input uses KWin's EIS interface through the pure Rust `reis` library. Each explicit test command requests pointer capabilities, sends bounded hover/scroll input, then disconnects. The Flatpak grants access to `org.kde.KWin` for this optional testing feature. No input session exists during normal startup. Other compositors may not provide this interface; semantic actions and renderer captures do not depend on it. Monitor placement also uses KWin. Native file chooser, physical keyboard, focus/occlusion and other desktop-specific interactions still need separate checks where a semantic action does not exercise them.

`ui activation-token` requests a standard Wayland desktop activation token for a launch test. The compositor decides whether to grant one based on the requesting window and recent input. Tests obtain a token while their own window is active, then pass it to the second desktop launch. This tests the same token forwarding used when a file manager opens a package; an arbitrary background shell launch has no guarantee of taking focus.

Use `ui start --config-dir /absolute/private-config --monitor DP-1` for isolated GUI preferences. Preserve the shared COSMIC theme in that directory, for example with a `cosmic` symlink. Test only disposable package/data fixtures when confirming mutations. Close only the instance the test created, and preserve existing windows. See [verification](verification.md) for saved test scripts and evidence.
