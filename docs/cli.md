# Standalone CLI

`warehouse-cosmic-cli` does one invocation's work and exits. User-level operations work without the app open or a display session. Its command parser and schemas are Rust, and it calls the same backend as the graphical app.

## Discover and read

```sh
warehouse-cosmic-cli guide
warehouse-cosmic-cli --help
warehouse-cosmic-cli commands --search snapshot
warehouse-cosmic-cli commands --json
warehouse-cosmic-cli describe package.version
warehouse-cosmic-cli schema
warehouse-cosmic-cli inspect
warehouse-cosmic-cli run --cmd 'package.list={"kind":"runtimes","installation":"user"}'
warehouse-cosmic-cli run --cmd 'data.list={"state":"leftover"}'
```

`guide` prints a short workflow for the headless commands and the live GUI controls. `commands` prints a table by default. `--json` includes parameter schemas and examples. `describe ID` prints one catalog entry as JSON. `schema` prints the JSON Schema for a complete command request. Unknown command IDs, fields, enum values, and wrong parameter types are errors. Schemas and examples are generated from the same Rust types that parse requests.

Use exact references returned by `package.list`. Mutating package commands require an explicit installation (`user`, `system`, or a configured installation name). An application ID is accepted only when it resolves to one installed ref in that installation. If several branches or architectures match, the CLI refuses the ambiguous target and lists the full refs.

Read commands may read Flatpak metadata or contact a remote when requested. They do not install or uninstall packages or change app data. `warnings` reports incomplete inventory reads; inspect it before relying on an inventory as complete.

## Preview and apply

```sh
# Substitute an exact installed reference from package.list.
warehouse-cosmic-cli run --cmd 'package.mask={"installation":"user","reference":"app/org.example.App/x86_64/stable","enabled":true}'

# Perform the same operation after reviewing the returned plan.
warehouse-cosmic-cli run --apply --cmd 'package.mask={"installation":"user","reference":"app/org.example.App/x86_64/stable","enabled":true}'
```

Without `--apply`, every mutating command returns `mode: "dry_run"`, a description, Flatpak argument arrays, and whether system authorization is required. Filesystem operations describe their snapshot, Trash, or recovery action instead of inventing a shell command. A dry run makes no package or app-data changes. `--report` explicitly writes the requested result file.

`--allow-system` is a second explicit flag for system mutations. It requires `--apply` and may open a PolicyKit authorization dialog. User-level operations need no display. The CLI never launches, closes, restarts, or signals a managed application. Close affected apps yourself before changing their data.

## Scripts

Repeated `--cmd` values execute in their given order. Alternatively, `--script` accepts a JSON job, a JSON array, or a text/JSONL file. `--cmd` and `--script` cannot be combined.

```json
{
  "schema_version": 1,
  "commands": [
    {"command": "package.list", "params": {"kind": "applications"}},
    {"command": "data.list", "params": {"state": "leftover"}}
  ]
}
```

Text scripts accept `command.id={"parameter":"value"}` or one full JSON request per line. Blank lines and lines beginning with `#` are ignored. Every request is parsed before the first command runs. Target resolution happens just before each step, so a later step can refer to a package installed by an earlier applied step.

```sh
warehouse-cosmic-cli run --script examples/audit.json
warehouse-cosmic-cli run --script my-changes.json --apply --report /absolute/new-report.json
```

Execution stops on the first failure. Earlier successful mutations remain applied. A version-change failure restores the masks it removed, and a snapshot restore stages extraction before replacing current data. These protections do not make an entire script transactional. Do not automatically retry a failed mutation; inspect the result and recorded state first.

## Output and exit codes

`run` and `inspect` write one JSON envelope to stdout:

```json
{
  "schema": "warehouse-cosmic/1",
  "ok": true,
  "results": [{"command":"data.list","mode":"read","data":[],"warnings":[]}],
  "error": null
}
```

Each successful result has a command ID and `mode` (`read`, `dry_run`, or `applied`). `data.list` also returns a `summary` with the folders matched, their total `bytes`, the number of folders scanned and the scanned root, so an empty list is distinguishable from a scan that found nothing. Target errors name the valid choices: an unknown installation lists the available ones, and a package missing from the requested installation says where it is installed. Applied results contain the backend report, including step results, errors, timestamps, recent output, and the durable log path. Failed runs retain earlier successful results and include an error code and message. A failed backend operation's report path identifies any partially applied steps. Diagnostics also go to stderr. Help and version output are plain text.

The schema identifier changes if meanings change; new optional fields may be added within version 1. Consumers should ignore unrecognized output fields. Input fields are strict.

| Exit | Meaning |
|---|---|
| 0 | All requested commands succeeded, or all plans were valid |
| 1 | Backend or I/O failure, including a partially completed operation |
| 2 | Invalid command, parameter, script, target type, or output path |
| 3 | Missing or ambiguous target |
| 4 | System authorization or external-file authority was not granted |
| 5 | Cancelled with Ctrl+C; completed steps remain applied |
| 6 | Another GUI or CLI operation holds the mutation lock |

## File access roots

By default, explicit local CLI paths use the caller's filesystem permissions. To confine externally supplied paths:

```sh
warehouse-cosmic-cli run --script import.json --apply \
  --automation-read-root /absolute/in \
  --automation-write-root /absolute/out
```

If either root is supplied, both permissions become explicit: an omitted read or write root grants none of that access. Roots must exist. Input symlinks are resolved, and output parents are resolved before the path is compared by whole directory components. An output parent must already exist.

The roots cover external bundle/repository/snapshot files, exports, and explicit result files. They do not restrict Flatpak's own installation paths, the fixed `~/.var/app` data operations, this app's snapshot library, or its operation reports. A script file supplied by the caller is read before these roots are applied. These checks constrain command arguments; they are not a sandbox against another process swapping paths concurrently.

## Recovery

App-data removal uses the desktop Trash. Restore it with the desktop's Trash interface. Snapshot restore reports the exact `.warehouse-recovery-*` folder containing previous data. To undo, close the affected app, move its current `~/.var/app/APP_ID` folder aside, and rename that reported recovery folder to `~/.var/app/APP_ID`. Export replacement and metadata renaming retain timestamped `.bak-*` files.
