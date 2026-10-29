# Agent workflow

Use `warehouse-cosmic-cli`. It works with the GUI closed. Run `warehouse-cosmic-cli guide` first: it prints the discovery commands for both layers below.

To work in the open window instead, use the `ui` command group described in [GUI CLI](ui-cli.md). `ui inspect` shows the page content, the open dialog and every control a person could click, each with a label and the flags it needs, so an agent can see and do what the person sees and does. Activate controls by ID, `ui wait`, then inspect again. Capture the renderer and the widget tree for layout reports. Native hover/scroll checks must report compositor failures rather than manufacture a passing hover state. Use an isolated test instance for preference changes and close only that instance afterward. The [project brief](project-brief.md) records the intended behavior.

1. Discover supported commands with `commands --json` or `describe ID`. Do not invent fields or use undocumented Flatpak shell flags.
2. Read `package.list`, `remote.list`, `data.list`, and `snapshot.list` as needed. Check returned `warnings` before treating the inventory as complete.
3. Use the returned installation and exact full ref. An app's data is shared across all installations and branches of its ID.
4. Plan mutations without `--apply`. Review the scope and the description. Uninstalling a package, removing app data, restoring a snapshot, and changing a remote are separate effects.
5. Apply only work the person has authorized. Add `--apply`; add `--allow-system` only when system changes and PolicyKit interaction are intended.
6. Check `ok`, each result's `mode`, the exit status, and the recorded backend report. Re-read the affected state to verify the result. On error, inspect completed steps before deciding whether a retry is safe.

The same rules hold in the GUI: `ui activate` opens a confirmation dialog for every change; read `dialog` (it includes the exact Flatpak commands) and confirm with `ui activate dialog.confirm --apply` only for authorized work.

`package.history` returns parsed `commits` (commit, subject, date; the first is the remote's current version) next to the raw log, ready for `package.version`.

A read-only audit is ready in `examples/audit.json`. Run it with:

```sh
warehouse-cosmic-cli run --script examples/audit.json
```

To back up app data, copy `examples/backup.json`, replace the example ID with an ID returned by `data.list`, and plan it. Ask the person to close the app before applying the snapshot if it is running. The CLI does not close applications for you.

For file imports or exports from agent-supplied paths, use the read/write root flags described in `cli.md`. Result files must be new paths. Stable machine-readable references are in `schemas/commands.json` and `schemas/request.json`; regenerate them with `verification/update-cli-reference.sh` after changing the command types.

The CLI and GUI use the same process-independent operation lock, so concurrent mutations are refused instead of overlapping. Refresh the GUI after an external CLI mutation to see the latest inventory. No persistent agent configuration is installed by this project.
