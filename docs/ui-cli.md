# Controlling and testing the GUI

The `ui` commands control an actual running Warehouse COSMIC window. The window provides a desktop D-Bus interface while it is open. Backend package commands remain usable with the GUI closed.

For the installed Flatpak, prefix these commands with:

```sh
flatpak run --command=warehouse-cosmic-cli dev.nilltadios.WarehouseCosmic
```

Discover the interface and select an instance:

```sh
warehouse-cosmic-cli ui commands
warehouse-cosmic-cli ui schema
warehouse-cosmic-cli ui instances
warehouse-cosmic-cli ui start
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

Controls that start another program also require `--apply`: `package.open` launches the app, and the folder buttons (`package.data.open`, `data:ID:open`, `snapshots.open-library`, `report.open-folder`) open the file manager. `inspect` lists them with `requires: "apply"`.

Every package, data or snapshot change first opens a confirmation dialog. Opening review dialogs, selecting, filtering, searching and copying need no flag. Confirming (`dialog.confirm`, `install.confirm`, `snapshot.create`) requires `--apply`; system installation changes also require `--allow-system`. Closing the selected window requires `ui close --apply`. Capture output must be a new absolute path unless `--overwrite` is explicit.

`ui open` behaves like Open Files. `ui drop` sends the files as `text/uri-list` data through the window's drag-and-drop handler, including URI decoding, file-type checks and the mixed/unsupported-type dialogs. Neither installs anything. A compositor drag from a file manager uses the same handler, but `ui drop` does not test the compositor's drag session itself.

## What `ui` does not do

`ui` never moves your mouse, presses keys or uses the desktop's screenshot tools; it works the same on every Wayland desktop. Physical pointer and keyboard input, the native file chooser and a compositor drag from a file manager are outside it and need a person to check.

## If you're on COSMIC

The repository also has optional COSMIC helpers with the same commands. They are not part of the app or the Flatpak; build them from a source checkout:

```sh
cargo build --features cosmic-tools --bin warehouse-cosmic-cosmic
target/debug/warehouse-cosmic-cosmic --instance INSTANCE hover header.settings --capture /absolute/settings-hover.png
target/debug/warehouse-cosmic-cosmic --instance INSTANCE scroll --target content --pixels 400 --capture /absolute/scrolled.png
target/debug/warehouse-cosmic-cosmic place DP-1
target/debug/warehouse-cosmic-cosmic present
target/debug/warehouse-cosmic-cosmic windows
```

`present` and `place` use COSMIC's toplevel-management and workspace protocols. `windows` lists the windows and monitors COSMIC reports. `hover` and `scroll` send real pointer input through the standard RemoteDesktop portal: the first run shows COSMIC's "allow remote control" dialog, and the approval is remembered in `~/.local/state/warehouse-cosmic/remote-desktop-token` until revoked. Like the KDE helpers, they confirm from the window's own `pointer` report that the input reached the requested control, fail rather than pretend, and leave the pointer where the test put it.

COSMIC does not report which process owns a window, so these helpers act only when exactly one Warehouse COSMIC window is open. Set `WAREHOUSE_COSMIC_DEBUG=1` to print the portal's monitor streams and the target coordinates. On other desktops every helper exits with an error saying it needs COSMIC.

## If you're on KDE Plasma

The repository also has optional KDE Plasma helpers that use real pointer input and window placement through KWin. They are not part of the app or the Flatpak; build them from a source checkout:

```sh
cargo build --features kde-tools --bin warehouse-cosmic-kde
target/debug/warehouse-cosmic-kde --instance INSTANCE hover header.settings --capture /absolute/settings-hover.png
target/debug/warehouse-cosmic-kde --instance INSTANCE scroll --target content --pixels 400 --capture /absolute/scrolled.png
target/debug/warehouse-cosmic-kde --instance INSTANCE place DP-1
target/debug/warehouse-cosmic-kde --instance INSTANCE present
```

`hover` and `scroll` send real pointer input through KWin's EIS interface (the `reis` library), then confirm from the window's own `pointer` report that the input reached the requested control. They fail rather than pretend if the compositor blocks input, and the pointer stays where the test left it. On other desktops every helper exits with an error saying it needs KDE Plasma.

`ui activation-token` requests a standard Wayland desktop activation token for a launch test. The compositor decides whether to grant one based on the requesting window and recent input. Tests obtain a token while their own window is active, then pass it to the second desktop launch. This tests the same token forwarding used when a file manager opens a package; an arbitrary background shell launch has no guarantee of taking focus.

Use `ui start --config-dir /absolute/private-config` for isolated GUI preferences. Preserve the shared COSMIC theme in that directory, for example with a `cosmic` symlink. Test only disposable package/data fixtures when confirming mutations. Close only the instance the test created, and preserve existing windows. See [verification](verification.md) for saved test scripts and evidence.
