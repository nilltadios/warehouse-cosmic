// SPDX-License-Identifier: GPL-3.0-or-later
//! Flatpak is invoked directly with argument arrays. No shell interpolation.
use crate::snapshots::{self, Snapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub id: String,
    pub version: String,
    pub branch: String,
    pub arch: String,
    pub origin: String,
    pub installation: String,
    pub reference: String,
    pub size: String,
    pub options: String,
    pub runtime: String,
    pub masked: bool,
    pub pinned: bool,
    pub masks: Vec<String>,
    pub pins: Vec<String>,
}
impl Package {
    pub fn is_runtime(&self) -> bool {
        self.options.split(',').any(|x| x == "runtime")
    }
    pub fn key(&self) -> String {
        format!("{}:{}", self.installation, self.full_ref())
    }
    pub fn full_ref(&self) -> String {
        if self.reference.starts_with("app/") || self.reference.starts_with("runtime/") {
            self.reference.clone()
        } else {
            format!(
                "{}/{}",
                if self.is_runtime() { "runtime" } else { "app" },
                self.reference
            )
        }
    }
    pub fn eol(&self) -> bool {
        self.options.contains("eol=")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remote {
    pub name: String,
    pub title: String,
    pub url: String,
    pub installation: String,
    pub options: String,
}
impl Remote {
    pub fn disabled(&self) -> bool {
        self.options.split(',').any(|x| x == "disabled")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFolder {
    pub id: String,
    pub bytes: u64,
    pub installed: bool,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub packages: Vec<Package>,
    pub remotes: Vec<Remote>,
    pub data: Vec<DataFolder>,
    pub snapshots: Vec<Snapshot>,
    pub installations: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
    pub data: PathBuf,
}
impl Paths {
    pub fn discover() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is missing")?;
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("warehouse-cosmic");
        Ok(Self { home, data })
    }
    pub fn app_data(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self.home.join(".var/app").join(id))
    }
    pub fn snapshots(&self) -> PathBuf {
        self.data.join("Snapshots")
    }
    pub fn official_snapshots(&self) -> PathBuf {
        self.home
            .join(".var/app/io.github.flattool.Warehouse/data/Snapshots")
    }
}
pub fn validate_id(id: &str) -> Result<()> {
    if id.split('.').count() < 3
        || id.split('.').any(str::is_empty)
        || !id
            .bytes()
            .all(|x| x.is_ascii_alphanumeric() || b"._-".contains(&x))
    {
        return Err(format!("Invalid application ID: {id}"));
    }
    Ok(())
}
pub fn move_to_trash(path: &Path) -> Result<()> {
    if crate::host::sandboxed() {
        // Host GIO uses the desktop's actual Trash and environment. Using the
        // sandbox's XDG_DATA_HOME would put files in the app's private Trash.
        let output = crate::host::command("gio", true)
            .args(["trash", "--"])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Cannot run host gio trash for {}: {e}", path.display()))?;
        if !output.status.success() {
            return Err(format!(
                "Host Trash failed for {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    } else {
        trash::delete(path).map_err(|e| e.to_string())?;
    }
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "Cannot verify that {} left its original location: {e}",
            path.display()
        )),
        Ok(_) => Err(format!(
            "{} still exists after the Trash operation. A running application may have recreated it.",
            path.display()
        )),
    }
}
pub fn scope(installation: &str) -> String {
    match installation {
        "user" => "--user".into(),
        "system" => "--system".into(),
        name => format!("--installation={name}"),
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn query(args: &[String]) -> Result<String> {
    let output = crate::host::command("flatpak", true)
        .args(args)
        .env("LC_ALL", "C")
        .env("FLATPAK_FANCY_OUTPUT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Cannot run flatpak: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "flatpak {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
pub fn parse_packages(text: &str) -> Result<Vec<Package>> {
    text.lines()
        .filter(|s| !s.is_empty())
        .map(|line| {
            let c: Vec<_> = line.split('\t').collect();
            // Flatpak omits trailing empty columns for runtimes with no parent runtime.
            if !(10..=11).contains(&c.len()) {
                return Err(format!(
                    "Unexpected Flatpak list row ({} columns): {line}",
                    c.len()
                ));
            }
            Ok(Package {
                name: c[0].into(),
                id: c[1].into(),
                version: c[2].into(),
                branch: c[3].into(),
                arch: c[4].into(),
                origin: c[5].into(),
                installation: normalize_installation(c[6]),
                reference: c[7].into(),
                size: c[8].into(),
                options: c[9].into(),
                runtime: c.get(10).unwrap_or(&"").to_string(),
                masked: false,
                pinned: false,
                masks: vec![],
                pins: vec![],
            })
        })
        .collect()
}
fn normalize_installation(value: &str) -> String {
    if value.starts_with("system (") {
        value
            .trim_start_matches("system (")
            .trim_end_matches(')')
            .into()
    } else {
        value.into()
    }
}
pub fn load(paths: &Paths) -> Result<Inventory> {
    let mut inv = Inventory {
        packages: parse_packages(&query(&args(&[
            "list",
            "--columns=name,application,version,branch,arch,origin,installation,ref,size,options,runtime",
        ]))?)?,
        ..Inventory::default()
    };
    let mut installations = BTreeSet::from(["user".to_string(), "system".to_string()]);
    if let Ok(files) = fs::read_dir(crate::host::installations_dir()) {
        for file in files.flatten() {
            if let Ok(text) = fs::read_to_string(file.path()) {
                for line in text.lines() {
                    if let Some(name) = line
                        .trim()
                        .strip_prefix("[Installation \"")
                        .and_then(|v| v.strip_suffix("\"]"))
                    {
                        installations.insert(name.into());
                    }
                }
            }
        }
    }
    installations.extend(inv.packages.iter().map(|p| p.installation.clone()));
    inv.installations = installations.into_iter().collect();
    for installation in &inv.installations {
        let flag = scope(installation);
        match query(&args(&[
            "remotes",
            &flag,
            "--show-disabled",
            "--columns=name,title,url,options",
        ])) {
            Ok(text) => {
                for line in text.lines().filter(|s| !s.is_empty()) {
                    let c: Vec<_> = line.split('\t').collect();
                    if c.len() >= 3 {
                        inv.remotes.push(Remote {
                            name: c[0].into(),
                            title: c[1].into(),
                            url: c[2].into(),
                            options: c.get(3).unwrap_or(&"").to_string(),
                            installation: installation.clone(),
                        });
                    } else {
                        inv.warnings.push(format!("Unexpected remote row: {line}"));
                    }
                }
            }
            Err(e) => inv.warnings.push(e),
        }
        let masks = query(&args(&["mask", &flag])).unwrap_or_else(|e| {
            inv.warnings.push(e);
            String::new()
        });
        let pins = query(&args(&["pin", &flag])).unwrap_or_else(|e| {
            inv.warnings.push(e);
            String::new()
        });
        for p in inv
            .packages
            .iter_mut()
            .filter(|p| &p.installation == installation)
        {
            p.masks = masks
                .lines()
                .map(str::trim)
                .filter(|pattern| ref_matches(pattern, p))
                .map(str::to_string)
                .collect();
            p.pins = pins
                .lines()
                .map(str::trim)
                .filter(|pattern| ref_matches(pattern, p))
                .map(str::to_string)
                .collect();
            p.masked = !p.masks.is_empty();
            p.pinned = !p.pins.is_empty();
        }
    }
    inv.packages
        .sort_by_key(|p| (p.name.to_lowercase(), p.key()));
    let installed: HashSet<_> = inv
        .packages
        .iter()
        .filter(|p| !p.is_runtime())
        .map(|p| p.id.as_str())
        .collect();
    let root = paths.home.join(".var/app");
    match fs::read_dir(&root) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                if validate_id(&id).is_err() {
                    continue;
                }
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => {
                        let (bytes, error) = match dir_size(&entry.path()) {
                            Ok(n) => (n, None),
                            Err(e) => (0, Some(e)),
                        };
                        inv.data.push(DataFolder {
                            installed: installed.contains(id.as_str()),
                            id,
                            bytes,
                            error,
                        });
                    }
                    Ok(kind) if kind.is_symlink() => inv
                        .warnings
                        .push(format!("Skipped symbolic link {}", entry.path().display())),
                    _ => (),
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => inv
            .warnings
            .push(format!("Cannot read {}: {e}", root.display())),
    }
    inv.data.sort_by_key(|d| d.id.to_lowercase());
    for (root, official) in [
        (paths.snapshots(), false),
        (paths.official_snapshots(), true),
    ] {
        match snapshots::list(&root, official) {
            Ok(s) => inv.snapshots.extend(s),
            Err(e) => inv.warnings.push(e),
        }
    }
    inv.snapshots
        .sort_by_key(|a| std::cmp::Reverse(a.timestamp));
    Ok(inv)
}
fn ref_matches(pattern: &str, p: &Package) -> bool {
    // Flatpak masks and pins allow glob patterns and abbreviated refs.
    let full = p.full_ref();
    [p.id.as_str(), p.reference.as_str(), full.as_str()]
        .iter()
        .any(|value| glob_match(pattern, value) || glob_match(&format!("{pattern}/*"), value))
}
fn glob_match(pattern: &str, value: &str) -> bool {
    let (p, v) = (pattern.as_bytes(), value.as_bytes());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < v.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == v[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == b'*' {
        i += 1;
    }
    i == p.len()
}
pub fn dir_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(p) = pending.pop() {
        let meta = fs::symlink_metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        if meta.is_dir() {
            for entry in fs::read_dir(&p).map_err(|e| e.to_string())? {
                pending.push(entry.map_err(|e| e.to_string())?.path());
            }
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}
pub fn size(bytes: u64) -> String {
    let mut n = bytes as f64;
    let mut unit = "B";
    for u in ["KiB", "MiB", "GiB", "TiB"] {
        if n < 1024. {
            break;
        }
        n /= 1024.;
        unit = u;
    }
    format!("{n:.1} {unit}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    pub args: Vec<String>,
    pub privileged: bool,
}
impl CommandSpec {
    fn new(values: Vec<String>) -> Self {
        let privileged = values
            .iter()
            .any(|a| a == "--system" || a.starts_with("--installation="));
        Self {
            args: values,
            privileged,
        }
    }
    pub fn display(&self) -> String {
        format!(
            "{}flatpak {}",
            if self.privileged { "pkexec " } else { "" },
            self.args
                .iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}
#[derive(Debug, Clone)]
pub enum Operation {
    Update(Vec<Package>),
    Reinstall {
        package: Package,
        remote: Remote,
    },
    Uninstall(Vec<Package>, bool),
    Mask(Package, bool),
    Pin(Package, bool),
    Version(Package, String, bool),
    RemoteEnabled(Remote, bool),
    RemoteRemove(Remote),
    RemoteAdd {
        installation: String,
        name: String,
        location: String,
    },
    Install {
        installation: String,
        remote: String,
        refs: Vec<String>,
    },
    InstallFile {
        installation: String,
        path: PathBuf,
    },
    InstallFiles {
        installation: String,
        paths: Vec<PathBuf>,
    },
    /// Queued remote packages, grouped by installation and remote.
    InstallQueue(Vec<InstallGroup>),
    TrashData(Vec<String>),
    Snapshot {
        ids: Vec<String>,
        name: String,
    },
    Restore(Snapshot),
    RestoreMany(Vec<Snapshot>),
    TrashSnapshot(Snapshot),
    TrashSnapshots(Vec<Snapshot>),
    ExportSnapshot(Snapshot, PathBuf),
    RenameSnapshot(Snapshot, String),
    ImportSnapshot {
        id: String,
        path: PathBuf,
        name: String,
    },
}
impl Operation {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Update(_) => "Update packages",
            Self::Reinstall { .. } => "Change package source",
            Self::Uninstall(..) => "Uninstall packages",
            Self::Mask(_, true) => "Block updates",
            Self::Mask(_, false) => "Allow updates",
            Self::Pin(_, true) => "Pin runtime",
            Self::Pin(_, false) => "Unpin runtime",
            Self::Version(..) => "Change package version",
            Self::RemoteEnabled(_, true) => "Enable remote",
            Self::RemoteEnabled(_, false) => "Disable remote",
            Self::RemoteRemove(..) => "Remove remote",
            Self::RemoteAdd { .. } => "Add remote",
            Self::Install { .. }
            | Self::InstallFile { .. }
            | Self::InstallFiles { .. }
            | Self::InstallQueue(_) => "Install packages",
            Self::TrashData(..) => "Move user data to Trash",
            Self::Snapshot { .. } => "Create snapshots",
            Self::Restore(..) => "Restore snapshot",
            Self::RestoreMany(..) => "Restore snapshots",
            Self::TrashSnapshot(..) => "Move snapshot to Trash",
            Self::TrashSnapshots(..) => "Move snapshots to Trash",
            Self::ExportSnapshot(..) => "Export snapshot",
            Self::RenameSnapshot(..) => "Rename snapshot",
            Self::ImportSnapshot { .. } => "Import snapshot",
        }
    }
    pub fn commands(&self) -> Result<Vec<CommandSpec>> {
        let mut result = Vec::new();
        match self {
            Self::Update(packages) => {
                if packages.is_empty() {
                    return Err("Select at least one package".into());
                }
                for p in packages {
                    if p.masked {
                        return Err(format!(
                            "Updates are blocked for {}. Unblock it before updating.",
                            p.id
                        ));
                    }
                    result.push(CommandSpec::new(args(&[
                        "update",
                        &scope(&p.installation),
                        "--noninteractive",
                        "-y",
                        &p.full_ref(),
                    ])));
                }
            }
            Self::Reinstall {
                package: p,
                remote: r,
            } => {
                if p.installation != r.installation {
                    return Err(
                        "The package and its new remote must use the same installation".into(),
                    );
                }
                if r.disabled() {
                    return Err("Enable the target remote first".into());
                }
                result.push(CommandSpec::new(args(&[
                    "install",
                    &scope(&p.installation),
                    "--noninteractive",
                    "-y",
                    "--reinstall",
                    &r.name,
                    &p.full_ref(),
                ])));
            }
            Self::Uninstall(packages, _) => {
                for p in packages {
                    result.push(CommandSpec::new(args(&[
                        "uninstall",
                        &scope(&p.installation),
                        "--noninteractive",
                        "-y",
                        &p.full_ref(),
                    ])));
                }
            }
            Self::Mask(p, on) | Self::Pin(p, on) => {
                let pin = matches!(self, Self::Pin(..));
                let patterns = if *on {
                    vec![p.full_ref()]
                } else {
                    let patterns = if pin { p.pins.clone() } else { p.masks.clone() };
                    if patterns.is_empty() {
                        vec![p.full_ref()]
                    } else {
                        patterns
                    }
                };
                for pattern in patterns {
                    let mut a = args(&[if pin { "pin" } else { "mask" }, &scope(&p.installation)]);
                    if !on {
                        a.push("--remove".into());
                    }
                    a.push(pattern);
                    result.push(CommandSpec::new(a));
                }
            }
            Self::Version(p, commit, _) => {
                if p.masks.iter().any(|pattern| {
                    pattern != &p.full_ref() && pattern != &p.id && pattern != &p.reference
                }) {
                    return Err("This package is covered by a broad mask. Unblock it first so you can review which mask patterns will be removed.".into());
                }
                if commit.len() != 64 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("Choose a valid 64-character commit".into());
                }
                let mut spec = CommandSpec::new(args(&[
                    "update",
                    &scope(&p.installation),
                    "--noninteractive",
                    "-y",
                    &format!("--commit={commit}"),
                    &p.full_ref(),
                ]));
                spec.privileged = p.installation != "user";
                result.push(spec);
            }
            Self::RemoteEnabled(r, on) => result.push(CommandSpec::new(args(&[
                "remote-modify",
                &scope(&r.installation),
                if *on { "--enable" } else { "--disable" },
                &r.name,
            ]))),
            Self::RemoteRemove(r) => result.push(CommandSpec::new(args(&[
                "remote-delete",
                &scope(&r.installation),
                &r.name,
            ]))),
            Self::RemoteAdd {
                installation,
                name,
                location,
            } => {
                if name.is_empty()
                    || name.starts_with('-')
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                {
                    return Err(
                        "Use letters, numbers, dots, underscores or hyphens for the remote name"
                            .into(),
                    );
                }
                validate_location(location)?;
                result.push(CommandSpec::new(args(&[
                    "remote-add",
                    &scope(installation),
                    "--if-not-exists",
                    name,
                    location,
                ])));
            }
            Self::Install {
                installation,
                remote,
                refs,
            } => {
                if refs.is_empty()
                    || remote.is_empty()
                    || remote.starts_with('-')
                    || refs.iter().any(|s| {
                        s.is_empty() || s.starts_with('-') || s.chars().any(char::is_whitespace)
                    })
                {
                    return Err("Choose a remote and at least one valid package reference".into());
                }
                let mut a = args(&[
                    "install",
                    &scope(installation),
                    "--noninteractive",
                    "-y",
                    remote,
                ]);
                a.extend(refs.clone());
                result.push(CommandSpec::new(a));
            }
            Self::InstallFile { installation, path } => {
                if !path.is_absolute() || !path.is_file() {
                    return Err("Select an existing local package file".into());
                }
                let option = match path.extension().and_then(|s| s.to_str()) {
                    Some("flatpak") => "--bundle",
                    Some("flatpakref") => "--from",
                    _ => return Err("Select a .flatpak or .flatpakref file".into()),
                };
                result.push(CommandSpec::new(args(&[
                    "install",
                    &scope(installation),
                    "--noninteractive",
                    "-y",
                    option,
                    &path.to_string_lossy(),
                ])));
            }
            Self::InstallFiles {
                installation,
                paths,
            } => {
                if paths.is_empty() {
                    return Err("Select at least one package file".into());
                }
                for path in paths {
                    result.extend(
                        Self::InstallFile {
                            installation: installation.clone(),
                            path: path.clone(),
                        }
                        .commands()?,
                    );
                }
            }
            Self::InstallQueue(groups) => {
                if groups.is_empty() {
                    return Err("Queue at least one package".into());
                }
                for group in groups {
                    result.extend(
                        Self::Install {
                            installation: group.installation.clone(),
                            remote: group.remote.clone(),
                            refs: group.refs.clone(),
                        }
                        .commands()?,
                    );
                }
            }
            Self::TrashData(ids) | Self::Snapshot { ids, .. } => {
                if ids.is_empty() {
                    return Err("Select at least one application".into());
                }
                for id in ids {
                    validate_id(id)?;
                }
            }
            Self::ImportSnapshot { id, .. } => validate_id(id)?,
            Self::RestoreMany(snapshots) | Self::TrashSnapshots(snapshots) => {
                if snapshots.is_empty() {
                    return Err("Select at least one snapshot".into());
                }
                if matches!(self, Self::RestoreMany(_)) {
                    let ids: BTreeSet<_> = snapshots.iter().map(|s| &s.id).collect();
                    if ids.len() != snapshots.len() {
                        return Err("Select one snapshot per application to restore".into());
                    }
                } else if snapshots.iter().any(|s| s.official) {
                    return Err("The official Warehouse snapshot library is read-only here. Import copies before editing it.".into());
                }
            }
            Self::RenameSnapshot(_, name) if name.trim().is_empty() => {
                return Err("Enter a snapshot name".into());
            }
            _ => (),
        }
        for spec in &mut result {
            spec.privileged = spec
                .args
                .iter()
                .any(|a| a == "--system" || a.starts_with("--installation="));
        }
        Ok(result)
    }
    /// The exact Flatpak commands followed by the plain-language explanation.
    pub fn description(&self) -> String {
        let mut text = self.command_lines();
        text.push_str(&self.explanation());
        text
    }
    pub fn command_lines(&self) -> String {
        self.commands()
            .map(|c| {
                c.iter()
                    .map(CommandSpec::display)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }
    /// The label of the confirming button.
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Update(_) => "Update",
            Self::Reinstall { .. } => "Reinstall",
            Self::Uninstall(..) => "Uninstall",
            Self::Mask(_, true) => "Block Updates",
            Self::Mask(_, false) => "Allow Updates",
            Self::Pin(_, true) => "Pin",
            Self::Pin(_, false) => "Unpin",
            Self::Version(..) => "Change Version",
            Self::RemoteEnabled(_, true) => "Enable",
            Self::RemoteEnabled(_, false) => "Disable",
            Self::RemoteRemove(..) => "Remove",
            Self::RemoteAdd { .. } => "Add Remote",
            Self::Install { .. }
            | Self::InstallFile { .. }
            | Self::InstallFiles { .. }
            | Self::InstallQueue(_) => "Install",
            Self::TrashData(..) | Self::TrashSnapshot(..) | Self::TrashSnapshots(..) => "Trash",
            Self::Snapshot { .. } => "Create",
            Self::Restore(..) | Self::RestoreMany(..) => "Restore",
            Self::ExportSnapshot(..) => "Export",
            Self::RenameSnapshot(..) => "Rename",
            Self::ImportSnapshot { .. } => "Import",
        }
    }
    /// Whether the confirming button should use destructive styling.
    pub fn destructive(&self) -> bool {
        matches!(
            self,
            Self::Uninstall(..)
                | Self::RemoteRemove(..)
                | Self::RemoteEnabled(_, false)
                | Self::TrashData(..)
                | Self::TrashSnapshot(..)
                | Self::TrashSnapshots(..)
                | Self::Restore(..)
                | Self::RestoreMany(..)
        )
    }
    pub fn explanation(&self) -> String {
        match self {
            Self::Update(_) => "\nUpdate from the installed origins, including dependencies required by Flatpak. App data is kept.".into(),
            Self::Reinstall{package:p,remote:r} => format!("\nChange {} from {} to {} in {}. App data is kept.",p.id,p.origin,r.name,p.installation),
            Self::Uninstall(p, true) => format!(
                "\nAfter each successful uninstall, move its app data to Trash. Data is shared across installations and branches. {} package(s).",
                p.len()
            ),
            Self::Uninstall(p, false) => format!("\nKeep app data. {} package(s).", p.len()),
            Self::TrashData(ids) => format!(
                "{}\nApp data is shared across installations and branches. Recover it from your desktop Trash. Close these apps yourself before continuing.",
                ids.join("\n")
            ),
            Self::Snapshot { ids, name } => format!(
                "{}\n{name}\nClose these apps yourself for a consistent snapshot.",
                ids.join("\n")
            ),
            Self::Restore(s) => format!(
                "{}\n{}\nCurrent data will be kept in the recovery folder. Close this app yourself before restoring.",
                s.id,
                s.path.display()
            ),
            Self::TrashSnapshot(s) => format!("{}\nMove this snapshot to Trash.", s.path.display()),
            Self::RestoreMany(s) => format!(
                "{}\nCurrent data will be kept in recovery folders. Close these apps yourself before continuing.",
                s.iter()
                    .map(|s| format!("{}: {}", s.id, s.name))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            Self::TrashSnapshots(s) => format!("Move {} selected snapshots to Trash.", s.len()),
            Self::ExportSnapshot(s, path) => {
                format!("{}\nExport to {}", s.path.display(), path.display())
            }
            Self::RenameSnapshot(s, name) => format!("{}\nNew name: {name}", s.path.display()),
            Self::ImportSnapshot { id, path, .. } => format!(
                "{id}\n{}\nImport into Warehouse COSMIC's snapshot library.",
                path.display()
            ),
            Self::RemoteEnabled(_, false) => {
                "\nApps from this remote will stop receiving updates.".into()
            }
            Self::RemoteRemove(_) => {
                "\nFlatpak will refuse removal while installed packages still need this remote."
                    .into()
            }
            Self::Version(_, _, hold) => {
                format!("\nBlock future updates after changing version: {hold}")
            }
            Self::Mask(p, false) if p.masks.iter().any(|s| s.contains('*')) => {
                "\nRemoving wildcard mask patterns also allows updates to other matching packages."
                    .into()
            }
            Self::Pin(p, false) if p.pins.iter().any(|s| s.contains('*')) => {
                "\nRemoving wildcard pin patterns also unpins other matching runtimes.".into()
            }
            Self::InstallQueue(groups) => format!(
                "\nInstall {} package(s) and the runtimes they require.",
                groups.iter().map(|g| g.refs.len()).sum::<usize>()
            ),
            Self::Install { refs, .. } => format!(
                "\nInstall {} package(s) and the runtimes they require.",
                refs.len()
            ),
            _ => String::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallGroup {
    pub installation: String,
    pub remote: String,
    pub refs: Vec<String>,
}
fn validate_location(value: &str) -> Result<()> {
    if value.starts_with('-') {
        return Err("Invalid location".into());
    }
    if Path::new(value).is_absolute() && Path::new(value).is_file() {
        return Ok(());
    }
    let url = url::Url::parse(value).map_err(|e| e.to_string())?;
    if !["https", "http", "file", "oci+https", "oci+http"].contains(&url.scheme()) {
        return Err("Use an HTTP(S) repository URL or a local .flatpakrepo file".into());
    }
    Ok(())
}
#[derive(Debug, Default)]
pub struct Progress {
    pub lines: Vec<String>,
    pub completed: usize,
    pub total: usize,
}
#[derive(Debug, Clone, Default)]
pub struct Job {
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<Progress>>,
}
impl Job {
    pub fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("Operation cancelled. Completed steps remain applied.".into())
        } else {
            Ok(())
        }
    }
    pub fn line(&self, line: String) {
        if let Ok(mut p) = self.progress.lock() {
            p.lines.push(line);
            if p.lines.len() > 300 {
                p.lines.remove(0);
            }
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub title: String,
    pub started: u64,
    pub finished: u64,
    pub steps: Vec<String>,
    pub errors: Vec<String>,
    pub cancelled: bool,
    pub output: Vec<String>,
    pub log: PathBuf,
}
pub fn run_command(spec: &CommandSpec, job: &Job) -> Result<String> {
    job.check()?;
    // "$ " marks the command itself, so progress views can tell it from output.
    job.line(format!("$ {}", spec.display()));
    let mut cmd = if spec.privileged {
        let mut c = crate::host::command("pkexec", true);
        c.arg("/usr/bin/flatpak");
        c
    } else {
        crate::host::command("flatpak", true)
    };
    let mut child = cmd
        .args(&spec.args)
        .env("LC_ALL", "C")
        .env("FLATPAK_FANCY_OUTPUT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut readers = Vec::new();
    for pipe in [
        child
            .stdout
            .take()
            .map(|v| Box::new(v) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|v| Box::new(v) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let job = job.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(pipe).split(b'\n').flatten() {
                job.line(String::from_utf8_lossy(&line).into_owned());
            }
        }));
    }
    let status = loop {
        if job.cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break child.wait().map_err(|e| e.to_string())?;
        }
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    for reader in readers {
        let _ = reader.join();
    }
    job.check()?;
    if status.success() {
        Ok(spec.display())
    } else {
        Err(format!(
            "{} exited with {status}. See operation output for details.",
            spec.display()
        ))
    }
}
pub fn execute(op: Operation, paths: &Paths, job: &Job) -> Result<Report> {
    // Coordinate the GUI and independent CLI invocations without a server.
    fs::create_dir_all(paths.data.join("operations")).map_err(|e| e.to_string())?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(paths.data.join("operation.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|e| format!("Another Warehouse operation holds the lock: {e}"))?;
    let mut report = Report {
        title: op.title().into(),
        started: now(),
        finished: 0,
        steps: vec![],
        errors: vec![],
        cancelled: false,
        output: vec![],
        log: PathBuf::new(),
    };
    let result = (|| -> Result<()> {
        op.commands()?;
        match &op {
            Operation::TrashData(ids) => {
                for id in ids {
                    job.check()?;
                    let p = paths.app_data(id)?;
                    ensure_real_dir(&p)?;
                    move_to_trash(&p)?;
                    report.steps.push(format!("Moved {} to Trash", p.display()));
                }
            }
            Operation::Snapshot { ids, name } => {
                for id in ids {
                    job.check()?;
                    let s = snapshots::create(paths, id, name, job)?;
                    report.steps.push(format!("Created {}", s.path.display()));
                }
            }
            Operation::Restore(s) => report.steps.push(snapshots::restore(paths, s, job)?),
            Operation::RestoreMany(selected) => {
                for s in selected {
                    job.check()?;
                    report.steps.push(snapshots::restore(paths, s, job)?);
                }
            }
            Operation::TrashSnapshots(selected) => {
                for s in selected {
                    job.check()?;
                    snapshots::trash_snapshot(paths, s)?;
                    report
                        .steps
                        .push(format!("Moved {} to Trash", s.path.display()));
                }
            }
            Operation::TrashSnapshot(s) => {
                snapshots::trash_snapshot(paths, s)?;
                report
                    .steps
                    .push(format!("Moved {} to Trash", s.path.display()));
            }
            Operation::RenameSnapshot(s, name) => {
                snapshots::rename(paths, s, name)?;
                report.steps.push(format!("Renamed to {name}"));
            }
            Operation::ExportSnapshot(s, path) => {
                snapshots::export(paths, s, path, job)?;
                report.steps.push(format!("Exported {}", path.display()));
            }
            Operation::ImportSnapshot { id, path, name } => {
                let s = snapshots::import(paths, id, path, name, job)?;
                report.steps.push(format!("Imported {}", s.path.display()));
            }
            Operation::Version(p, _, hold) => {
                // Preserve the previous mask even if the version change fails.
                let mut unmasked: Vec<&str> = vec![];
                for pattern in &p.masks {
                    let spec = CommandSpec::new(args(&[
                        "mask",
                        &scope(&p.installation),
                        "--remove",
                        pattern,
                    ]));
                    if let Err(e) = run_command(&spec, job) {
                        for previous in &unmasked {
                            run_command(
                                &CommandSpec::new(args(&[
                                    "mask",
                                    &scope(&p.installation),
                                    previous,
                                ])),
                                &Job::default(),
                            )?;
                        }
                        return Err(e);
                    }
                    unmasked.push(pattern.as_str());
                }
                let changed = run_command(&op.commands()?[0], job);
                let restore_job = Job::default();
                if changed.is_err() {
                    for pattern in unmasked {
                        run_command(
                            &CommandSpec::new(args(&["mask", &scope(&p.installation), pattern])),
                            &restore_job,
                        )?;
                    }
                }
                if *hold && changed.is_ok() {
                    run_command(
                        &CommandSpec::new(args(&["mask", &scope(&p.installation), &p.full_ref()])),
                        &restore_job,
                    )
                    .map_err(|e| format!("Mask could not be restored: {e}"))?;
                }
                report.steps.push(changed?);
            }
            _ => {
                let commands = op.commands()?;
                if let Ok(mut p) = job.progress.lock() {
                    p.total = commands.len();
                }
                for (i, spec) in commands.iter().enumerate() {
                    job.check()?;
                    match run_command(spec, job) {
                        Ok(step) => {
                            report.steps.push(step);
                            if let Operation::Uninstall(packages, true) = &op {
                                let p = &packages[i];
                                if !p.is_runtime() {
                                    let data = paths.app_data(&p.id)?;
                                    if data.exists() {
                                        ensure_real_dir(&data)?;
                                        move_to_trash(&data)?;
                                        report
                                            .steps
                                            .push(format!("Moved {} to Trash", data.display()));
                                    }
                                }
                            }
                        }
                        Err(e) => report.errors.push(e),
                    }
                    if let Ok(mut p) = job.progress.lock() {
                        p.completed = i + 1;
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        report.errors.push(e);
    }
    report.cancelled = job.cancel.load(Ordering::Relaxed);
    report.finished = now();
    let dir = paths.data.join("operations");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let log = dir.join(format!(
        "{}-{}-{nonce}.json",
        report.started,
        std::process::id()
    ));
    report.log = log.clone();
    report.output = job
        .progress
        .lock()
        .map(|p| p.lines.clone())
        .unwrap_or_default();
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&log)
        .map_err(|e| format!("Cannot save operation report: {e}"))?;
    file.write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(report)
}
pub fn ensure_real_dir(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.is_symlink() {
        Err(format!(
            "Refusing non-directory or symbolic link {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}
pub fn details(p: &Package) -> Result<String> {
    query(&args(&["info", &scope(&p.installation), &p.full_ref()]))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub installation: String,
    pub reference: String,
    pub origin: String,
    pub installed_commit: String,
    pub available_commit: String,
    pub update_available: bool,
    pub masked: bool,
}
pub fn check_update(p: &Package) -> Result<UpdateStatus> {
    let installed = query(&args(&[
        "info",
        &scope(&p.installation),
        "--show-commit",
        &p.full_ref(),
    ]))?
    .trim()
    .to_string();
    let available = query(&args(&[
        "remote-info",
        &scope(&p.installation),
        "--show-commit",
        &p.origin,
        &p.full_ref(),
    ]))?
    .trim()
    .to_string();
    if [&installed, &available]
        .iter()
        .any(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Flatpak did not return valid installed and remote commits".into());
    }
    Ok(UpdateStatus {
        installation: p.installation.clone(),
        reference: p.full_ref(),
        origin: p.origin.clone(),
        update_available: installed != available,
        installed_commit: installed,
        available_commit: available,
        masked: p.masked,
    })
}
pub fn history(p: &Package) -> Result<String> {
    query(&args(&[
        "remote-info",
        &scope(&p.installation),
        "--log",
        &p.origin,
        &p.full_ref(),
    ]))
}
/// One entry of `flatpak remote-info --log`. The first entry is the remote's current commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub commit: String,
    pub subject: String,
    pub date: String,
}
pub fn parse_history(text: &str) -> Vec<Commit> {
    let mut commits: Vec<Commit> = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Commit" if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) => {
                if !commits.iter().any(|c| c.commit == value) {
                    commits.push(Commit {
                        commit: value.into(),
                        subject: String::new(),
                        date: String::new(),
                    });
                }
            }
            "Subject" => {
                if let Some(last) = commits.last_mut().filter(|c| c.subject.is_empty()) {
                    last.subject = value.into();
                }
            }
            "Date" => {
                if let Some(last) = commits.last_mut().filter(|c| c.date.is_empty()) {
                    last.date = value.into();
                }
            }
            _ => (),
        }
    }
    commits
}
pub fn remote_packages(r: &Remote, query_text: &str) -> Result<Vec<(String, String)>> {
    let text = query(&args(&[
        "remote-ls",
        &scope(&r.installation),
        "--app",
        "--columns=name,ref",
        &r.name,
    ]))?;
    Ok(text
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(name, r)| {
            format!("{name} {r}")
                .to_lowercase()
                .contains(&query_text.to_lowercase())
        })
        .map(|(a, b)| (a.into(), b.into()))
        .collect())
}
pub fn launch(p: &Package) -> Result<()> {
    if p.is_runtime() {
        return Err("Runtimes cannot be opened".into());
    }
    let mut child = crate::host::command("flatpak", false)
        .args(["run", &scope(&p.installation), &p.full_ref()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
pub fn open(path: &Path) -> Result<()> {
    let mut child = crate::host::command("xdg-open", false)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn package() -> Package {
        parse_packages("Test\torg.example.App\t1.0\tstable\tx86_64\tflathub\tuser\torg.example.App/x86_64/stable\t1.2 MB\t\torg.example.Platform/x86_64/1\n").unwrap().remove(0)
    }
    #[test]
    fn parser_keeps_empty_columns_and_scope() {
        let p = package();
        assert_eq!(p.full_ref(), "app/org.example.App/x86_64/stable");
        assert!(!p.is_runtime());
        assert!(parse_packages("bad row").is_err());
        let runtime=parse_packages("Platform\torg.example.Platform\t\tstable\tx86_64\tflathub\tuser\torg.example.Platform/x86_64/stable\t1 MB\truntime\n").unwrap();
        assert!(runtime[0].is_runtime());
        assert!(runtime[0].runtime.is_empty());
    }
    #[test]
    fn exact_uninstall_and_no_shell() {
        let mut p = package();
        p.installation = "extra".into();
        let cmd = Operation::Uninstall(vec![p], false).commands().unwrap();
        assert_eq!(
            cmd[0].args,
            [
                "uninstall",
                "--installation=extra",
                "--noninteractive",
                "-y",
                "app/org.example.App/x86_64/stable"
            ]
        );
    }
    #[test]
    fn custom_origin_updates_and_migration_keep_exact_scope() {
        let mut p = package();
        p.origin = "parsec-autobuild".into();
        let update = Operation::Update(vec![p.clone()]).commands().unwrap();
        assert_eq!(
            update[0].args,
            [
                "update",
                "--user",
                "--noninteractive",
                "-y",
                "app/org.example.App/x86_64/stable"
            ]
        );
        let remote = Remote {
            name: "custom-source".into(),
            title: "Custom".into(),
            url: "https://example.org/repo".into(),
            installation: "user".into(),
            options: String::new(),
        };
        let change = Operation::Reinstall {
            package: p.clone(),
            remote: remote.clone(),
        }
        .commands()
        .unwrap();
        assert!(change[0].args.contains(&"--reinstall".into()));
        assert!(change[0].args.contains(&"custom-source".into()));
        p.masked = true;
        assert!(Operation::Update(vec![p]).commands().is_err());
    }
    #[test]
    fn reject_option_injection_and_bad_commits() {
        assert!(
            Operation::RemoteAdd {
                installation: "user".into(),
                name: "--bad".into(),
                location: "https://example.org".into()
            }
            .commands()
            .is_err()
        );
        assert!(
            Operation::Version(package(), "$(touch nope)".into(), false)
                .commands()
                .is_err()
        );
        assert!(validate_id("../../etc").is_err());
        assert!(validate_id("org.example.App").is_ok());
    }
    #[test]
    fn masks_match_partial_and_glob_refs() {
        let p = package();
        assert!(ref_matches("org.example.*", &p));
        assert!(ref_matches("app/org.example.App", &p));
        assert!(!ref_matches("org.other.*", &p));
    }
    #[test]
    fn size_does_not_follow_symlinks() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("f"), b"abc").unwrap();
        std::os::unix::fs::symlink("/", t.path().join("link")).unwrap();
        assert_eq!(dir_size(t.path()).unwrap(), 4);
        assert!(ensure_real_dir(&t.path().join("link")).is_err());
    }
    #[test]
    fn version_history_is_parsed_into_selectable_commits() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let text = format!(
            "        Ref: app/org.example.App/x86_64/stable\n     Commit: {a}\n    Subject: Update to 2.0\n       Date: 2026-10-01 10:00:00 +0000\n    History:\n\n     Commit: {b}\n    Subject: Initial\n       Date: 2026-09-01 10:00:00 +0000\n     Commit: not-a-commit\n"
        );
        let commits = parse_history(&text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].commit, a);
        assert_eq!(commits[0].subject, "Update to 2.0");
        assert_eq!(commits[1].date, "2026-09-01 10:00:00 +0000");
    }
    #[test]
    fn queued_installs_keep_each_remote_and_installation() {
        let op = Operation::InstallQueue(vec![
            InstallGroup {
                installation: "user".into(),
                remote: "flathub".into(),
                refs: vec!["app/org.example.A/x86_64/stable".into()],
            },
            InstallGroup {
                installation: "system".into(),
                remote: "fedora".into(),
                refs: vec!["app/org.example.B/x86_64/stable".into()],
            },
        ]);
        let commands = op.commands().unwrap();
        assert_eq!(commands.len(), 2);
        assert!(commands[0].args.contains(&"--user".to_string()) && !commands[0].privileged);
        assert!(commands[1].args.contains(&"fedora".to_string()) && commands[1].privileged);
        assert_eq!(op.verb(), "Install");
        assert!(Operation::InstallQueue(vec![]).commands().is_err());
    }
}
