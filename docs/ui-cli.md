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

If several windows are running, commands require an exact instance. This prevents tests from silently selecting another window. `ui wait` waits for inventory/detail reads and owned operations to finish.

## See what a person sees

`inspect` returns the window as data:

| Field | Contents |
|---|---|
| `page`, `page_id` | Visible page (`packages`, `remotes`, `data`, `snapshots`, `install`) |
| `view` | The page's content in display order: filtered packages and the selected package with its `flatpak info` details; data folders with sizes; snapshot groups; install remotes, search results (installed/queued) and the pending queue; remote presets |
| `modal`, `dialog` | Open dialog kind and its content: title, explanation, exact commands, choices and the current selection |
| `controls` | Every control that can be activated now: `id`, a readable `label` and `requires` (`none`, `apply` or `apply+allow-system`) |
| `fields` | Text fields that `ui fill` accepts |
| `operation` | While a change runs: its plain title ("Uninstalling Parsec…"), the current status line and a percentage when Flatpak reports one |
| `last_operation` | The finished change: title ("Parsec uninstalled" or "Couldn't uninstall Parsec"), `ok`, `cancelled`, steps, errors and the report path. `result.details` opens its details dialog |
| `notice`, `toast` | Other messages, and the last short confirmation such as "Copied to clipboard" |
| `busy`, `loading`, `detail_loading` | Work in progress |

`actions` remains as a plain list of control IDs. `ui controls` prints only the controls. `ui tree` adds the rendered widget tree with visible text and logical bounds, for layout checks.

Control IDs are stable and readable, for example `package.uninstall`, `uninstall.trash`, `dialog.confirm`, `filter.remote:user:flathub`, `data:org.example.App:snapshot`, `snapshot:/path/to/file.tar.zst:restore`, `install.remote:user:flathub`, `install.result:app/org.example.App/x86_64/stable`, `install.queue.review`, `snapshot.pick:org.example.App` and `settings.theme:dark`. Always read the current list instead of guessing: a control appears only while it is visible.

`ui start` presents the normal running instance when one exists. `ui start --config-dir PATH` explicitly starts a separate test instance with private preferences. Normal desktop launches and opening files reuse the main window.

## Reproduce a UI issue

```sh
warehouse-cosmic-cli ui --instance INSTANCE navigate remotes
warehouse-cosmic-cli ui --instance INSTANCE activate remotes.show-disabled
warehouse-cosmic-cli ui --instance INSTANCE fill search flathub
warehouse-cosmic-cli ui --instance INSTANCE capture /absolute/new-output.png
warehouse-cosmic-cli ui --instance INSTANCE open /absolute/package.flatpak
warehouse-cosmic-cli ui --instance INSTANCE drop "/absolute/My Package.flatpak"
warehouse-cosmic-cli ui --instance INSTANCE activate install.scope:user
warehouse-cosmic-cli ui --instance INSTANCE activate dialog.cancel
warehouse-cosmic-cli ui --instance INSTANCE theme light
warehouse-cosmic-cli ui --instance INSTANCE window --mode maximized
warehouse-cosmic-cli ui --instance INSTANCE window --mode windowed --width 1100 --height 700
warehouse-cosmic-cli ui --instance INSTANCE navigate install
warehouse-cosmic-cli ui --instance INSTANCE activate install.remote:user:flathub
warehouse-cosmic-cli ui --instance INSTANCE fill install.query flatseal
warehouse-cosmic-cli ui --instance INSTANCE activate install.search
warehouse-cosmic-cli ui --instance INSTANCE wait
```

`activate`, also spelled `click`, invokes the same message handler as the named visible control. It tests GUI transitions and rendering rather than calling the backend behind the window. It is a semantic action, not a physical pointer click or keyboard typing test. Unavailable controls are refused.

Every package, data or snapshot change first opens a confirmation dialog. Opening review dialogs, selecting, filtering, searching and copying need no flag. Confirming (`dialog.confirm`, `install.confirm`, `snapshot.create`) requires `--apply`; system installation changes also require `--allow-system`. Closing the selected window requires `ui close --apply`. Capture output must be a new absolute path unless `--overwrite` is explicit.

`ui open` behaves like Open Files. `ui drop` sends the files as `text/uri-list` data through the window's drag-and-drop handler, including URI decoding, file-type checks and the mixed/unsupported-type dialogs. Neither installs anything. A compositor drag from a file manager uses the same handler, but `ui drop` does not test the compositor's drag session itself.

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
