// SPDX-License-Identifier: GPL-3.0-or-later
//! Process-per-invocation CLI. Backend commands do not initialize the GUI.
use crate::{
    backend::{self, Inventory, Job, Operation, Package, Paths, Remote},
    snapshots::Snapshot,
};
use clap::{Parser, Subcommand};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Parser)]
#[command(
    name = "warehouse-cosmic-cli",
    version,
    about = "Flatpak management and explicit GUI controls. Package mutations require --apply.",
    long_about = "Warehouse COSMIC CLI uses the same Rust backend as the desktop app. Backend commands run without a window. The ui subcommands inspect and operate the open window.\n\nStart with `guide`; `docs` prints the full documentation. Discover backend commands with `commands --json` and the live window's controls with `ui inspect`. Package mutations require --apply. System mutations also require --allow-system and may show PolicyKit authorization."
)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Start here: how agents discover, plan, apply and verify, for the CLI and the live GUI.
    Guide,
    /// Print bundled documentation. Without a topic, list the topics.
    Docs { topic: Option<String> },
    /// Inspect and control an actual running GUI through its desktop D-Bus API.
    Ui(crate::ui_client::Args),
    /// List command IDs, descriptions and parameter schemas.
    Commands {
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print one command's schema, effects and example as JSON.
    Describe { command: String },
    /// Read the complete current inventory as JSON.
    Inspect,
    /// Print the JSON Schema for script commands.
    Schema,
    /// Run one or more commands, stopping at the first error. Applied steps are not rolled back.
    Run {
        #[arg(
            long = "cmd",
            conflicts_with = "script",
            required_unless_present = "script"
        )]
        commands: Vec<String>,
        #[arg(long, conflicts_with = "commands")]
        script: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
        /// Explicitly allow system mutations and interactive PolicyKit authorization.
        #[arg(long, requires = "apply")]
        allow_system: bool,
        /// If either root is supplied, external file reads/writes require their corresponding root.
        #[arg(long)]
        automation_read_root: Option<PathBuf>,
        #[arg(long)]
        automation_write_root: Option<PathBuf>,
        /// Also write the result envelope to a new file. Existing files are refused.
        #[arg(long)]
        report: Option<PathBuf>,
    },
}
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub reference: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageRefs {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub references: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reinstall {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub reference: String,
    pub remote: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Toggle {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub reference: String,
    pub enabled: bool,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Uninstall {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub references: Vec<String>,
    #[serde(default)]
    pub trash_data: bool,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Version {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub reference: String,
    pub commit: String,
    #[serde(default)]
    pub block_updates: bool,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteTarget {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub name: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteAdd {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub name: String,
    pub location: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteToggle {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub name: String,
    pub enabled: bool,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Install {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub remote: String,
    pub refs: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallFiles {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub files: Vec<PathBuf>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ids {
    pub ids: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSnapshot {
    pub ids: Vec<String>,
    pub name: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapshotPaths {
    pub paths: Vec<PathBuf>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameSnapshot {
    pub path: PathBuf,
    pub name: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportSnapshot {
    pub id: String,
    pub file: PathBuf,
    pub name: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportSnapshot {
    pub path: PathBuf,
    pub destination: PathBuf,
}
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    All,
    Applications,
    Runtimes,
}
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct List {
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub installation: Option<String>,
    #[serde(default)]
    pub kind: Kind,
}
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataState {
    #[default]
    All,
    Active,
    Leftover,
}
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataList {
    #[serde(default)]
    pub state: DataState,
    #[serde(default)]
    pub search: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteSearch {
    /// Installation that holds the target: "user", "system" or a configured installation name. Use the `installation` value returned by package.list or remote.list.
    pub installation: String,
    pub name: String,
    #[serde(default)]
    pub search: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", content = "params", deny_unknown_fields)]
pub enum Request {
    #[serde(rename = "package.update-check")]
    UpdateCheck(Target),
    #[serde(rename = "package.update")]
    Update(PackageRefs),
    #[serde(rename = "package.reinstall")]
    Reinstall(Reinstall),
    #[serde(rename = "inventory.list")]
    Inventory(Empty),
    #[serde(rename = "package.list")]
    Packages(List),
    #[serde(rename = "package.info")]
    Info(Target),
    #[serde(rename = "package.history")]
    History(Target),
    #[serde(rename = "package.uninstall")]
    Uninstall(Uninstall),
    #[serde(rename = "package.mask")]
    Mask(Toggle),
    #[serde(rename = "package.pin")]
    Pin(Toggle),
    #[serde(rename = "package.version")]
    Version(Version),
    #[serde(rename = "package.install")]
    Install(Install),
    #[serde(rename = "package.install-files")]
    InstallFiles(InstallFiles),
    #[serde(rename = "remote.list")]
    Remotes(Empty),
    #[serde(rename = "remote.packages")]
    RemotePackages(RemoteSearch),
    #[serde(rename = "remote.add")]
    RemoteAdd(RemoteAdd),
    #[serde(rename = "remote.enable")]
    RemoteEnable(RemoteToggle),
    #[serde(rename = "remote.remove")]
    RemoteRemove(RemoteTarget),
    #[serde(rename = "data.list")]
    Data(DataList),
    #[serde(rename = "data.trash")]
    TrashData(Ids),
    #[serde(rename = "snapshot.list")]
    Snapshots(Empty),
    #[serde(rename = "snapshot.create")]
    Snapshot(CreateSnapshot),
    #[serde(rename = "snapshot.restore")]
    Restore(SnapshotPaths),
    #[serde(rename = "snapshot.trash")]
    TrashSnapshots(SnapshotPaths),
    #[serde(rename = "snapshot.rename")]
    Rename(RenameSnapshot),
    #[serde(rename = "snapshot.import")]
    Import(ImportSnapshot),
    #[serde(rename = "snapshot.export")]
    Export(ExportSnapshot),
}
#[derive(Debug, Serialize)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip)]
    pub exit: u8,
}
type Result<T> = std::result::Result<T, Error>;
fn error(code: &'static str, exit: u8, message: impl Into<String>) -> Error {
    Error {
        code,
        exit,
        message: message.into(),
        details: None,
    }
}
fn invalid(message: impl Into<String>) -> Error {
    error("invalid_argument", 2, message)
}
fn failure(message: impl Into<String>) -> Error {
    error("backend_error", 1, message)
}
impl Request {
    pub fn name(&self) -> String {
        serde_json::to_value(self).expect("Request serializes")["command"]
            .as_str()
            .unwrap_or_default()
            .into()
    }
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Self::UpdateCheck(_)
                | Self::Inventory(_)
                | Self::Packages(_)
                | Self::Info(_)
                | Self::History(_)
                | Self::Remotes(_)
                | Self::RemotePackages(_)
                | Self::Data(_)
                | Self::Snapshots(_)
        )
    }
}
fn entry<T: JsonSchema>(name: &str, description: &str, mutates: bool, example: Value) -> Value {
    json!({"command":name,"description":description,"mutates":mutates,"params_schema":schema_for!(T),"example":example,"dry_run_by_default":mutates})
}
pub fn catalog() -> Vec<Value> {
    let target = json!({"installation":"user","reference":"org.example.App/x86_64/stable"});
    vec![
        entry::<Target>(
            "package.update-check",
            "Compare installed and remote commits using the package's actual origin.",
            false,
            target.clone(),
        ),
        entry::<PackageRefs>(
            "package.update",
            "Update exact installed packages from their origins, preserving app data and honoring masks.",
            true,
            json!({"installation":"user","references":["app/org.example.App/x86_64/stable"]}),
        ),
        entry::<Reinstall>(
            "package.reinstall",
            "Reinstall an exact package from a chosen remote in the same installation, preserving app data.",
            true,
            json!({"installation":"user","reference":"app/org.example.App/x86_64/stable","remote":"custom-repo"}),
        ),
        entry::<Empty>(
            "inventory.list",
            "Read packages, remotes, app data, snapshots and warnings.",
            false,
            json!({}),
        ),
        entry::<List>(
            "package.list",
            "Filter installed packages by kind, installation and search text.",
            false,
            json!({"kind":"applications"}),
        ),
        entry::<Target>(
            "package.info",
            "Read all Flatpak details of one exact installed package.",
            false,
            target.clone(),
        ),
        entry::<Target>(
            "package.history",
            "Read the origin's available version commits (may use the network).",
            false,
            target.clone(),
        ),
        entry::<Uninstall>(
            "package.uninstall",
            "Uninstall exact packages; optionally move shared app data to Trash.",
            true,
            json!({"installation":"user","references":["org.example.App/x86_64/stable"],"trash_data":false}),
        ),
        entry::<Toggle>(
            "package.mask",
            "Set an update mask. enabled=true blocks updates. Removal can affect wildcard matches.",
            true,
            json!({"installation":"user","reference":"org.example.App/x86_64/stable","enabled":true}),
        ),
        entry::<Toggle>(
            "package.pin",
            "Pin or unpin an installed runtime. Applications cannot be pinned.",
            true,
            json!({"installation":"user","reference":"runtime/org.example.Platform/x86_64/stable","enabled":true}),
        ),
        entry::<Version>(
            "package.version",
            "Switch to a remote commit; restore the old masks if the update fails.",
            true,
            json!({"installation":"user","reference":"org.example.App/x86_64/stable","commit":"<64 hex characters>","block_updates":true}),
        ),
        entry::<Install>(
            "package.install",
            "Install exact references from an enabled remote.",
            true,
            json!({"installation":"user","remote":"flathub","refs":["org.example.App"]}),
        ),
        entry::<InstallFiles>(
            "package.install-files",
            "Install .flatpak bundles and .flatpakref files in order.",
            true,
            json!({"installation":"user","files":["/path/app.flatpak"]}),
        ),
        entry::<Empty>(
            "remote.list",
            "Read all remotes, including disabled remotes and their installation.",
            false,
            json!({}),
        ),
        entry::<RemoteSearch>(
            "remote.packages",
            "Search applications in one enabled remote (may use the network).",
            false,
            json!({"installation":"user","name":"flathub","search":"editor"}),
        ),
        entry::<RemoteAdd>(
            "remote.add",
            "Add a URL or a local .flatpakrepo file.",
            true,
            json!({"installation":"user","name":"flathub","location":"https://dl.flathub.org/repo/flathub.flatpakrepo"}),
        ),
        entry::<RemoteToggle>(
            "remote.enable",
            "Enable or disable a remote. Disabling stops updates from it.",
            true,
            json!({"installation":"user","name":"flathub","enabled":true}),
        ),
        entry::<RemoteTarget>(
            "remote.remove",
            "Remove a remote. Flatpak refuses while installed packages need it.",
            true,
            json!({"installation":"user","name":"example"}),
        ),
        entry::<DataList>(
            "data.list",
            "List app data folders in ~/.var/app with sizes. state: leftover finds data from uninstalled apps (nothing is changed). The result's summary gives folders matched, total bytes and folders scanned.",
            false,
            json!({"state":"leftover"}),
        ),
        entry::<Ids>(
            "data.trash",
            "Move shared app data to the desktop Trash. Close these apps first.",
            true,
            json!({"ids":["org.example.App"]}),
        ),
        entry::<Empty>(
            "snapshot.list",
            "List this app's snapshots and the official Warehouse library (read-only).",
            false,
            json!({}),
        ),
        entry::<CreateSnapshot>(
            "snapshot.create",
            "Create Warehouse-compatible tar.zst backups. Close these apps first.",
            true,
            json!({"ids":["org.example.App"],"name":"Before changes"}),
        ),
        entry::<SnapshotPaths>(
            "snapshot.restore",
            "Restore one snapshot per app after validating archives; keep current data in recovery folders.",
            true,
            json!({"paths":["/path/to/library/org.example.App/123_data-0.tar.zst"]}),
        ),
        entry::<SnapshotPaths>(
            "snapshot.trash",
            "Move selected snapshots from this app's library to Trash.",
            true,
            json!({"paths":["/path/to/library/org.example.App/123_data-0.tar.zst"]}),
        ),
        entry::<RenameSnapshot>(
            "snapshot.rename",
            "Rename a snapshot in this app's library; back up its metadata.",
            true,
            json!({"path":"/path/snapshot.tar.zst","name":"Before update"}),
        ),
        entry::<ImportSnapshot>(
            "snapshot.import",
            "Validate and copy a .tar.zst snapshot into this app's library.",
            true,
            json!({"id":"org.example.App","file":"/path/backup.tar.zst","name":"Imported"}),
        ),
        entry::<ExportSnapshot>(
            "snapshot.export",
            "Export an archive and metadata. Back up existing destinations first.",
            true,
            json!({"path":"/path/to/library/snapshot.tar.zst","destination":"/path/export.tar.zst"}),
        ),
    ]
}
pub fn parse_command(text: &str) -> Result<Request> {
    if text.trim_start().starts_with('{') {
        return serde_json::from_str(text).map_err(|e| invalid(e.to_string()));
    }
    let (name, params) = text.split_once('=').unwrap_or((text, "{}"));
    let params: Value = serde_json::from_str(params).map_err(|e| invalid(e.to_string()))?;
    serde_json::from_value(json!({"command":name.trim(),"params":params}))
        .map_err(|e| invalid(e.to_string()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    schema_version: u32,
    commands: Vec<Request>,
}
pub fn parse_script(text: &str) -> Result<Vec<Request>> {
    if let Ok(script) = serde_json::from_str::<Script>(text) {
        if script.schema_version != 1 {
            return Err(invalid("Unsupported script schema_version; expected 1"));
        }
        return Ok(script.commands);
    }
    if let Ok(commands) = serde_json::from_str::<Vec<Request>>(text) {
        return Ok(commands);
    }
    text.lines()
        .enumerate()
        .filter(|(_, s)| !s.trim().is_empty() && !s.trim_start().starts_with('#'))
        .map(|(i, s)| {
            parse_command(s).map_err(|mut e| {
                e.message = format!("Line {}: {}", i + 1, e.message);
                e
            })
        })
        .collect()
}
#[derive(Default)]
pub struct Policy {
    pub apply: bool,
    pub allow_system: bool,
    pub read_root: Option<PathBuf>,
    pub write_root: Option<PathBuf>,
}
impl Policy {
    fn external(&self, path: &Path, write: bool) -> Result<PathBuf> {
        let resolved = if write {
            let abs = std::path::absolute(path).map_err(|e| invalid(e.to_string()))?;
            if abs.exists() {
                fs::canonicalize(&abs).map_err(|e| invalid(e.to_string()))?
            } else {
                let parent = abs
                    .parent()
                    .ok_or_else(|| invalid("Destination needs a parent directory"))?;
                fs::canonicalize(parent)
                    .map_err(|e| invalid(e.to_string()))?
                    .join(
                        abs.file_name()
                            .ok_or_else(|| invalid("Destination needs a filename"))?,
                    )
            }
        } else {
            match fs::canonicalize(path) {
                Ok(resolved) => resolved,
                // Inside the Flatpak some host paths (the host's /tmp) are not
                // mounted. Without file roots, accept a file the host's Flatpak
                // can read; with roots, paths must resolve locally to be checked.
                Err(_)
                    if self.read_root.is_none()
                        && self.write_root.is_none()
                        && path.is_absolute()
                        && crate::host::is_file(path) =>
                {
                    path.to_path_buf()
                }
                Err(e) => return Err(invalid(e.to_string())),
            }
        };
        if self.read_root.is_some() || self.write_root.is_some() {
            let root = if write {
                &self.write_root
            } else {
                &self.read_root
            };
            let root = root.as_ref().ok_or_else(|| {
                error(
                    "path_denied",
                    4,
                    if write {
                        "No write root was granted"
                    } else {
                        "No read root was granted"
                    },
                )
            })?;
            if !resolved.starts_with(root) {
                return Err(error(
                    "path_denied",
                    4,
                    format!("{} is outside {}", resolved.display(), root.display()),
                ));
            }
        }
        Ok(resolved)
    }
}
fn package(inv: &Inventory, installation_name: &str, reference: &str) -> Result<Package> {
    installation(inv, installation_name)?;
    let installation = installation_name;
    let matches: Vec<_> = inv
        .packages
        .iter()
        .filter(|p| {
            p.installation == installation
                && (p.full_ref() == reference || p.reference == reference || p.id == reference)
        })
        .collect();
    match matches.as_slice() {
        [p] => Ok((*p).clone()),
        [] => {
            let elsewhere: BTreeSet<_> = inv
                .packages
                .iter()
                .filter(|p| {
                    p.full_ref() == reference || p.reference == reference || p.id == reference
                })
                .map(|p| p.installation.as_str())
                .collect();
            Err(error(
                "not_found",
                3,
                if elsewhere.is_empty() {
                    format!("No installed package {reference} in {installation}")
                } else {
                    format!(
                        "No installed package {reference} in {installation}; it is installed in: {}",
                        elsewhere.into_iter().collect::<Vec<_>>().join(", ")
                    )
                },
            ))
        }
        _ => Err(error(
            "ambiguous_target",
            3,
            format!(
                "Use a full ref: {}",
                matches
                    .iter()
                    .map(|p| p.full_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}
fn remote(inv: &Inventory, installation_name: &str, name: &str) -> Result<Remote> {
    installation(inv, installation_name)?;
    let installation = installation_name;
    inv.remotes
        .iter()
        .find(|r| r.installation == installation && r.name == name)
        .cloned()
        .ok_or_else(|| {
            error(
                "not_found",
                3,
                format!(
                    "No remote {name} in {installation}; remotes there: {}",
                    inv.remotes
                        .iter()
                        .filter(|r| r.installation == installation)
                        .map(|r| r.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })
}
fn snapshot(inv: &Inventory, path: &Path) -> Result<Snapshot> {
    let path = fs::canonicalize(path).map_err(|e| error("not_found", 3, e.to_string()))?;
    inv.snapshots
        .iter()
        .find(|s| fs::canonicalize(&s.path).ok().as_ref() == Some(&path))
        .cloned()
        .ok_or_else(|| error("not_found", 3, "Snapshot is outside the known libraries"))
}
fn installation(inv: &Inventory, name: &str) -> Result<()> {
    if inv.installations.iter().any(|s| s == name) {
        Ok(())
    } else {
        Err(invalid(format!(
            "Unknown installation {name}; available: {}",
            inv.installations.join(", ")
        )))
    }
}
fn prepare(
    request: &Request,
    inv: &Inventory,
    paths: &Paths,
    policy: &Policy,
) -> Result<Operation> {
    let op = match request {
        Request::Update(p) => Operation::Update(
            p.references
                .iter()
                .map(|r| package(inv, &p.installation, r))
                .collect::<Result<_>>()?,
        ),
        Request::Reinstall(p) => Operation::Reinstall {
            package: package(inv, &p.installation, &p.reference)?,
            remote: remote(inv, &p.installation, &p.remote)?,
        },
        Request::Uninstall(p) => Operation::Uninstall(
            p.references
                .iter()
                .map(|r| package(inv, &p.installation, r))
                .collect::<Result<_>>()?,
            p.trash_data,
        ),
        Request::Mask(p) => {
            Operation::Mask(package(inv, &p.installation, &p.reference)?, p.enabled)
        }
        Request::Pin(p) => {
            let target = package(inv, &p.installation, &p.reference)?;
            if !target.is_runtime() {
                return Err(invalid("Only runtimes can be pinned"));
            }
            Operation::Pin(target, p.enabled)
        }
        Request::Version(p) => Operation::Version(
            package(inv, &p.installation, &p.reference)?,
            p.commit.clone(),
            p.block_updates,
        ),
        Request::Install(p) => {
            let r = remote(inv, &p.installation, &p.remote)?;
            if r.disabled() {
                return Err(invalid("Enable the remote before installing packages"));
            }
            Operation::Install {
                installation: p.installation.clone(),
                remote: p.remote.clone(),
                refs: p.refs.clone(),
            }
        }
        Request::InstallFiles(p) => {
            installation(inv, &p.installation)?;
            Operation::InstallFiles {
                installation: p.installation.clone(),
                paths: p
                    .files
                    .iter()
                    .map(|p| policy.external(p, false))
                    .collect::<Result<_>>()?,
            }
        }
        Request::RemoteAdd(p) => {
            installation(inv, &p.installation)?;
            let file_url = url::Url::parse(&p.location)
                .ok()
                .filter(|url| url.scheme() == "file");
            let location = if let Some(url) = file_url {
                let file = url
                    .to_file_path()
                    .map_err(|_| invalid("Invalid local repository file URL"))?;
                policy.external(&file, false)?.display().to_string()
            } else if Path::new(&p.location).exists() {
                policy
                    .external(Path::new(&p.location), false)?
                    .display()
                    .to_string()
            } else {
                p.location.clone()
            };
            Operation::RemoteAdd {
                installation: p.installation.clone(),
                name: p.name.clone(),
                location,
            }
        }
        Request::RemoteEnable(p) => {
            Operation::RemoteEnabled(remote(inv, &p.installation, &p.name)?, p.enabled)
        }
        Request::RemoteRemove(p) => Operation::RemoteRemove(remote(inv, &p.installation, &p.name)?),
        Request::TrashData(p) => {
            for id in &p.ids {
                backend::ensure_real_dir(&paths.app_data(id).map_err(invalid)?).map_err(invalid)?;
            }
            Operation::TrashData(p.ids.clone())
        }
        Request::Snapshot(p) => {
            for id in &p.ids {
                backend::ensure_real_dir(&paths.app_data(id).map_err(invalid)?).map_err(invalid)?;
            }
            Operation::Snapshot {
                ids: p.ids.clone(),
                name: p.name.clone(),
            }
        }
        Request::Restore(p) => Operation::RestoreMany(
            p.paths
                .iter()
                .map(|p| snapshot(inv, p))
                .collect::<Result<_>>()?,
        ),
        Request::TrashSnapshots(p) => Operation::TrashSnapshots(
            p.paths
                .iter()
                .map(|p| snapshot(inv, p))
                .collect::<Result<_>>()?,
        ),
        Request::Rename(p) => {
            let s = snapshot(inv, &p.path)?;
            if s.official {
                return Err(invalid(
                    "Import a copy before renaming an official Warehouse snapshot",
                ));
            }
            Operation::RenameSnapshot(s, p.name.clone())
        }
        Request::Import(p) => Operation::ImportSnapshot {
            id: p.id.clone(),
            path: policy.external(&p.file, false)?,
            name: p.name.clone(),
        },
        Request::Export(p) => Operation::ExportSnapshot(
            snapshot(inv, &p.path)?,
            policy.external(&p.destination, true)?,
        ),
        _ => return Err(invalid("Expected a mutating command")),
    };
    op.commands().map_err(invalid)?;
    Ok(op)
}
pub fn run_request(request: &Request, paths: &Paths, policy: &Policy, job: &Job) -> Result<Value> {
    job.check().map_err(|e| error("cancelled", 5, e))?;
    let inv = backend::load(paths).map_err(failure)?;
    let read = match request {
        Request::UpdateCheck(p) => Some(json!(
            backend::check_update(&package(&inv, &p.installation, &p.reference)?)
                .map_err(failure)?
        )),
        Request::Inventory(_) => Some(json!(inv)),
        Request::Packages(p) => Some(json!(
            inv.packages
                .iter()
                .filter(
                    |x| p.installation.as_ref().is_none_or(|i| i == &x.installation)
                        && match p.kind {
                            Kind::All => true,
                            Kind::Applications => !x.is_runtime(),
                            Kind::Runtimes => x.is_runtime(),
                        }
                        && format!("{} {} {}", x.name, x.id, x.reference)
                            .to_lowercase()
                            .contains(&p.search.to_lowercase())
                )
                .collect::<Vec<_>>()
        )),
        Request::Info(p) => Some(
            json!({"package":package(&inv,&p.installation,&p.reference)?,"details":backend::details(&package(&inv,&p.installation,&p.reference)?).map_err(failure)?}),
        ),
        Request::History(p) => {
            let history = backend::history(&package(&inv, &p.installation, &p.reference)?)
                .map_err(failure)?;
            Some(json!({"commits":backend::parse_history(&history),"history":history}))
        }
        Request::Remotes(_) => Some(json!(inv.remotes)),
        Request::RemotePackages(p) => Some(json!(
            backend::remote_packages(&remote(&inv, &p.installation, &p.name)?, &p.search)
                .map_err(failure)?
        )),
        Request::Data(p) => Some(json!(
            inv.data
                .iter()
                .filter(|d| match p.state {
                    DataState::All => true,
                    DataState::Active => d.installed,
                    DataState::Leftover => !d.installed,
                } && d.id.to_lowercase().contains(&p.search.to_lowercase()))
                .collect::<Vec<_>>()
        )),
        Request::Snapshots(_) => Some(json!(inv.snapshots)),
        _ => None,
    };
    if let Some(data) = read {
        let mut result =
            json!({"command":request.name(),"mode":"read","data":data,"warnings":inv.warnings});
        if let Request::Data(_) = request {
            // An empty list must be distinguishable from a scan that saw nothing.
            let matched = result["data"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            result["summary"] = json!({
                "folders": matched.len(),
                "bytes": matched.iter().filter_map(|d| d["bytes"].as_u64()).sum::<u64>(),
                "scanned_folders": inv.data.len(),
                "root": paths.home.join(".var/app"),
            });
        }
        return Ok(result);
    }
    let op = prepare(request, &inv, paths, policy)?;
    let commands = op.commands().map_err(invalid)?;
    if !policy.apply {
        return Ok(
            json!({"command":request.name(),"mode":"dry_run","description":op.description(),"commands":commands,"requires_system_authorization":commands.iter().any(|c|c.privileged),"applied":false}),
        );
    }
    if commands.iter().any(|c| c.privileged) && !policy.allow_system {
        return Err(error(
            "authorization_required",
            4,
            "System mutations require --allow-system and may show PolicyKit authorization",
        ));
    }
    let report = backend::execute(op, paths, job).map_err(|e| {
        if e.starts_with("Another Warehouse operation") {
            error("busy", 6, e)
        } else {
            failure(e)
        }
    })?;
    if report.cancelled {
        let mut e = error(
            "cancelled",
            5,
            format!(
                "Operation cancelled; completed steps remain applied. Report: {}",
                report.log.display()
            ),
        );
        e.details = Some(json!({"report":report}));
        return Err(e);
    }
    if !report.errors.is_empty() {
        let mut e = error(
            "operation_failed",
            1,
            format!(
                "{}\nReport: {}",
                report.errors.join("\n"),
                report.log.display()
            ),
        );
        e.details = Some(json!({"report":report}));
        return Err(e);
    }
    Ok(json!({"command":request.name(),"mode":"applied","report":report}))
}
fn emit(value: &Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)
}
fn envelope(results: Vec<Value>, failure: Option<&Error>) -> Value {
    json!({"schema":"warehouse-cosmic/1","ok":failure.is_none(),"results":results,"error":failure})
}
fn canonical_root(path: Option<PathBuf>) -> Result<Option<PathBuf>> {
    path.map(|p| {
        fs::canonicalize(p)
            .map_err(|e| invalid(e.to_string()))
            .and_then(|p| {
                if p.is_dir() {
                    Ok(p)
                } else {
                    Err(invalid("Automation root must be a directory"))
                }
            })
    })
    .transpose()
}
/// Documentation compiled into the CLI, so it reads the same on the host and
/// inside the Flatpak sandbox, whose /app paths the host cannot see.
const DOCS: [(&str, &str, &str); 7] = [
    (
        "llms",
        "Short index for agents",
        include_str!("../llms.txt"),
    ),
    (
        "cli",
        "Command contract, scripts, exit codes, file roots, recovery",
        include_str!("../docs/cli.md"),
    ),
    (
        "agents",
        "Plan, apply and verify workflow",
        include_str!("../docs/agents.md"),
    ),
    (
        "ui-cli",
        "Operating and testing the live window",
        include_str!("../docs/ui-cli.md"),
    ),
    (
        "flatpak",
        "Install, update, permissions and publishing",
        include_str!("../docs/flatpak.md"),
    ),
    (
        "parity",
        "Feature comparison with Warehouse",
        include_str!("../docs/parity.md"),
    ),
    (
        "development",
        "Building from source and tests",
        include_str!("../docs/development.md"),
    ),
];
fn run_cli(cli: Cli) -> Result<Value> {
    match cli.action {
        Action::Ui(args) => crate::ui_client::run(args).map_err(failure),
        Action::Guide => {
            io::stdout()
                .lock()
                .write_all(include_str!("guide.txt").as_bytes())
                .map_err(|e| failure(e.to_string()))?;
            Ok(Value::Null)
        }
        Action::Docs { topic } => {
            let mut out = io::stdout().lock();
            let text = match topic.as_deref() {
                None => {
                    let mut list =
                        String::from("Documentation topics (warehouse-cosmic-cli docs TOPIC):\n");
                    for (name, summary, _) in DOCS {
                        list.push_str(&format!("  {name:<12} {summary}\n"));
                    }
                    list.push_str(
                        "Online: https://github.com/nilltadios/warehouse-cosmic/tree/main/docs\n",
                    );
                    list
                }
                Some(name) => DOCS
                    .iter()
                    .find(|(topic, ..)| *topic == name.trim_end_matches(".md"))
                    .map(|(.., text)| text.to_string())
                    .ok_or_else(|| {
                        invalid(format!(
                            "Unknown topic {name}; available: {}",
                            DOCS.iter().map(|(t, ..)| *t).collect::<Vec<_>>().join(", ")
                        ))
                    })?,
            };
            out.write_all(text.as_bytes())
                .map_err(|e| failure(e.to_string()))?;
            Ok(Value::Null)
        }
        Action::Commands {
            search,
            json: as_json,
        } => {
            let entries: Vec<_> = catalog()
                .into_iter()
                .filter(|c| {
                    search.as_ref().is_none_or(|s| {
                        format!("{} {}", c["command"], c["description"])
                            .to_lowercase()
                            .contains(&s.to_lowercase())
                    })
                })
                .collect();
            if !as_json {
                let mut out = io::stdout().lock();
                for c in &entries {
                    writeln!(
                        out,
                        "{:<24} {}{}",
                        c["command"].as_str().unwrap_or_default(),
                        if c["mutates"] == true { "[apply] " } else { "" },
                        c["description"].as_str().unwrap_or_default()
                    )
                    .map_err(|e| failure(e.to_string()))?;
                }
                return Ok(Value::Null);
            }
            Ok(json!({"schema":"warehouse-cosmic-commands/1","commands":entries}))
        }
        Action::Describe { command } => catalog()
            .into_iter()
            .find(|c| c["command"] == command)
            .ok_or_else(|| invalid(format!("Unknown command {command}"))),
        Action::Inspect => run_request(
            &Request::Inventory(Empty {}),
            &Paths::discover().map_err(failure)?,
            &Policy::default(),
            &Job::default(),
        )
        .map(|r| envelope(vec![r], None)),
        Action::Schema => {
            Ok(serde_json::to_value(schema_for!(Request)).expect("Schema serializes"))
        }
        Action::Run {
            commands,
            script,
            apply,
            allow_system,
            automation_read_root,
            automation_write_root,
            report,
        } => {
            let requests = if let Some(path) = script {
                parse_script(&fs::read_to_string(path).map_err(|e| invalid(e.to_string()))?)?
            } else {
                commands
                    .iter()
                    .map(|s| parse_command(s))
                    .collect::<Result<Vec<_>>>()?
            };
            if requests.is_empty() {
                return Err(invalid("A run needs at least one command"));
            }
            let policy = Policy {
                apply,
                allow_system,
                read_root: canonical_root(automation_read_root)?,
                write_root: canonical_root(automation_write_root)?,
            };
            // Reserve an explicitly requested report before applying any changes.
            let report_file = report
                .map(|path| {
                    let path = policy.external(&path, true)?;
                    fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(path)
                        .map_err(|e| invalid(format!("Cannot create report: {e}")))
                })
                .transpose()?;
            let paths = Paths::discover().map_err(failure)?;
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            ctrlc::set_handler(move || {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            })
            .map_err(|e| failure(e.to_string()))?;
            let job = Job {
                cancel,
                ..Job::default()
            };
            let mut results = vec![];
            let mut failed = None;
            for request in requests {
                match run_request(&request, &paths, &policy, &job) {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        failed = Some(e);
                        break;
                    }
                }
            }
            let value = envelope(results, failed.as_ref());
            if let Some(mut file) = report_file {
                serde_json::to_writer_pretty(&mut file, &value)
                    .map_err(|e| failure(e.to_string()))?;
                file.write_all(b"\n").map_err(|e| failure(e.to_string()))?;
            }
            if let Some(e) = failed {
                let _ = emit(&value);
                eprintln!("{}: {}", e.code, e.message);
                std::process::exit(e.exit.into());
            }
            Ok(value)
        }
    }
}
pub fn main_entry() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = e.exit_code();
            if code == 0 {
                let _ = e.print();
                return ExitCode::SUCCESS;
            }
            let err = invalid(e.to_string());
            let _ = emit(&envelope(vec![], Some(&err)));
            return ExitCode::from(2);
        }
    };
    match run_cli(cli) {
        Ok(Value::Null) => ExitCode::SUCCESS,
        Ok(value) => match emit(&value) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            let _ = emit(&envelope(vec![], Some(&e)));
            eprintln!("{}: {}", e.code, e.message);
            ExitCode::from(e.exit)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_parameters_and_commands_are_errors() {
        assert!(parse_command("data.trash={\"ids\":[],\"force\":true}").is_err());
        assert!(parse_command("package.nuke").is_err());
    }
    #[test]
    fn scripts_are_parsed_before_any_effect() {
        assert!(parse_script("remote.list\nunknown.command").is_err());
        assert_eq!(parse_script("# note\nremote.list\n{\"command\":\"data.list\",\"params\":{\"state\":\"leftover\"}}").unwrap().len(),2);
    }
    #[test]
    fn catalog_has_schemas_and_roundtrips_examples() {
        for entry in catalog() {
            assert_eq!(entry["params_schema"]["type"], "object");
            assert!(
                parse_command(&format!(
                    "{}={}",
                    entry["command"].as_str().unwrap(),
                    entry["example"]
                ))
                .is_ok()
            );
        }
    }
    #[test]
    fn file_roots_follow_links_and_separate_read_write() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("in");
        let other = root.path().join("other");
        fs::create_dir(&allowed).unwrap();
        fs::create_dir(&other).unwrap();
        fs::write(other.join("data"), "x").unwrap();
        std::os::unix::fs::symlink(other.join("data"), allowed.join("link")).unwrap();
        let policy = Policy {
            read_root: Some(allowed.clone()),
            ..Policy::default()
        };
        assert!(policy.external(&allowed.join("link"), false).is_err());
        assert!(policy.external(&allowed.join("out"), true).is_err());
    }
    #[test]
    fn repository_file_urls_obey_the_read_root() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("allowed");
        fs::create_dir(&allowed).unwrap();
        let file = root.path().join("outside.flatpakrepo");
        fs::write(&file, "[Flatpak Repo]\n").unwrap();
        let request = Request::RemoteAdd(RemoteAdd {
            installation: "user".into(),
            name: "test".into(),
            location: url::Url::from_file_path(&file).unwrap().to_string(),
        });
        let inv = Inventory {
            installations: vec!["user".into()],
            ..Inventory::default()
        };
        let policy = Policy {
            read_root: Some(allowed),
            ..Policy::default()
        };
        let paths = Paths {
            home: root.path().join("home"),
            data: root.path().join("data"),
        };
        assert_eq!(
            prepare(&request, &inv, &paths, &policy).unwrap_err().code,
            "path_denied"
        );
    }
}
