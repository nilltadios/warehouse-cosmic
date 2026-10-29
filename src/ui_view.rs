// SPDX-License-Identifier: GPL-3.0-or-later
//! GUI automation uses the same messages as visible controls. Every control
//! an agent may activate is listed by `ui_controls` with a readable label and
//! the authorization it needs, so `inspect` shows what a person can click.
use super::*;
use crate::ui_bridge::{Node, Pending, Tree};
use cosmic::Application;
use serde_json::{Value, json};
use warehouse_cosmic::ui_protocol::{self as protocol, Request};

/// What activating a control requires beyond being visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Guard {
    /// Navigation, selection, review dialogs and copying.
    None,
    /// Changes packages, app data or snapshots in the user's own scope.
    Apply,
    /// Changes something; `true` also needs `--allow-system`.
    Scoped(bool),
}
pub(super) struct Control {
    pub id: String,
    pub label: String,
    pub message: Message,
    pub guard: Guard,
}
fn control(id: impl Into<String>, label: impl Into<String>, message: Message) -> Control {
    Control {
        id: id.into(),
        label: label.into(),
        message,
        guard: Guard::None,
    }
}
fn guarded(
    id: impl Into<String>,
    label: impl Into<String>,
    message: Message,
    guard: Guard,
) -> Control {
    Control {
        guard,
        ..control(id, label, message)
    }
}
fn system_operation(operation: &Operation) -> bool {
    operation
        .commands()
        .is_ok_and(|commands| commands.iter().any(|c| c.privileged))
}
const FIELDS: [(&str, &str); 8] = [
    (
        "search",
        "Page search (Packages, Remotes, User Data, Snapshots)",
    ),
    ("install.query", "Remote package search"),
    ("remote.name", "Add Remote: name"),
    ("remote.url", "Add Remote: URL or .flatpakrepo path"),
    ("version.commit", "Change Version: commit"),
    ("snapshot.name", "New/Import/Rename Snapshot: name"),
    ("snapshot.search", "New Snapshot: app search"),
    ("import.id", "Import Snapshot: application ID"),
];

impl App {
    pub(super) fn ui_controls(&self) -> Vec<Control> {
        if self.job.is_some() {
            return vec![control(
                "operation.cancel",
                "Cancel Operation",
                Message::Cancel,
            )];
        }
        if let Some(dialog) = &self.dialog {
            return self.dialog_controls(dialog);
        }
        let mut list = vec![
            control("header.logo", "About Warehouse COSMIC", Message::About),
            control("header.sidebar", "Toggle sidebar", Message::ToggleSidebar),
            control("header.settings", "Settings", Message::Settings),
            control("header.about", "About Warehouse COSMIC", Message::About),
        ];
        if !self.loading {
            list.push(control("header.refresh", "Refresh", Message::Refresh));
        }
        for page in Page::ALL {
            list.push(control(
                format!("nav.{}", page.slug()),
                page.name(),
                Message::Page(page),
            ));
        }
        if self.notice.is_some() {
            list.push(control(
                "notice.dismiss",
                "Dismiss report",
                Message::ClearNotice,
            ));
        }
        if self.core.window.show_context {
            for (index, (name, label)) in [
                ("system", "Theme: Match desktop"),
                ("dark", "Theme: Dark"),
                ("light", "Theme: Light"),
            ]
            .into_iter()
            .enumerate()
            {
                list.push(control(
                    format!("settings.theme:{name}"),
                    label,
                    Message::Theme(index),
                ));
            }
            list.push(control(
                "settings.close",
                "Close Settings",
                Message::CloseSettings,
            ));
        }
        if self.loading {
            return list;
        }
        if self.toolbar().is_some() {
            list.push(control(
                "packages.select",
                if self.batch { "Done" } else { "Select" },
                Message::Batch(!self.batch),
            ));
        }
        match self.page {
            Page::Packages => self.package_controls(&mut list),
            Page::Remotes => self.remote_controls(&mut list),
            Page::Data => self.data_controls(&mut list),
            Page::Snapshots => self.snapshot_controls(&mut list),
            Page::Install => self.install_controls(&mut list),
        }
        list
    }
    fn dialog_controls(&self, dialog: &Dialog) -> Vec<Control> {
        let mut list = vec![control(
            "dialog.cancel",
            "Cancel or close",
            Message::CloseDialog,
        )];
        let scopes = |list: &mut Vec<Control>| {
            for (index, name) in self.inventory.installations.iter().enumerate() {
                list.push(control(
                    format!("install.scope:{name}"),
                    format!("Installation: {name}"),
                    Message::InstallScope(index),
                ));
            }
        };
        match dialog {
            Dialog::InstallFiles(files) => {
                let system = self
                    .inventory
                    .installations
                    .get(self.install_scope)
                    .is_some_and(|name| name != "user");
                if !files.is_empty() {
                    list.push(guarded(
                        "install.confirm",
                        "Install",
                        Message::InstallFilesConfirmed,
                        Guard::Scoped(system),
                    ));
                }
                scopes(&mut list);
            }
            Dialog::Confirm(operation) => {
                list.push(guarded(
                    "dialog.confirm",
                    operation.verb(),
                    Message::Confirm,
                    Guard::Scoped(system_operation(operation)),
                ));
                list.push(control(
                    "dialog.details",
                    "Show or hide command details",
                    Message::ShowCommands(!self.show_commands),
                ));
            }
            Dialog::Uninstall(packages) => {
                if packages.iter().any(|p| !p.is_runtime()) {
                    list.push(control(
                        "uninstall.keep",
                        "Keep app settings and content",
                        Message::UninstallData(false),
                    ));
                    list.push(control(
                        "uninstall.trash",
                        "Move app settings and content to Trash",
                        Message::UninstallData(true),
                    ));
                }
                list.push(guarded(
                    "dialog.confirm",
                    "Uninstall",
                    Message::Confirm,
                    Guard::Scoped(packages.iter().any(|p| p.installation != "user")),
                ));
            }
            Dialog::Remote => {
                list.push(control(
                    "dialog.review",
                    "Review remote",
                    Message::Ask(Operation::RemoteAdd {
                        installation: self
                            .inventory
                            .installations
                            .get(self.install_scope)
                            .cloned()
                            .unwrap_or_else(|| "user".into()),
                        name: self.remote_name.trim().into(),
                        location: self.remote_url.trim().into(),
                    }),
                ));
                list.push(control(
                    "remote.choose-file",
                    "Choose .flatpakrepo file",
                    Message::ChooseFile(FilePurpose::Remote),
                ));
                scopes(&mut list);
            }
            Dialog::Source(p) => {
                let remotes: Vec<_> = self
                    .inventory
                    .remotes
                    .iter()
                    .filter(|r| r.installation == p.installation && !r.disabled())
                    .collect();
                for (index, r) in remotes.iter().enumerate() {
                    list.push(control(
                        format!("source:{}", r.name),
                        format!("Source: {}", remotes::title(r)),
                        Message::RemoteChoice(index),
                    ));
                }
                if let Some(r) = remotes.get(self.remote_choice) {
                    list.push(control(
                        "dialog.review",
                        "Review source change",
                        Message::Ask(Operation::Reinstall {
                            package: p.clone(),
                            remote: (*r).clone(),
                        }),
                    ));
                }
            }
            Dialog::InstallIds(ids) => {
                let remotes = self.enabled_remotes();
                for (index, r) in remotes.iter().enumerate() {
                    list.push(control(
                        format!("remote-choice:{}:{}", r.installation, r.name),
                        format!("Install from {} ({})", remotes::title(r), r.installation),
                        Message::RemoteChoice(index),
                    ));
                }
                if let Some(r) = remotes.get(self.remote_choice) {
                    list.push(control(
                        "dialog.review",
                        "Review installation",
                        Message::Ask(Operation::Install {
                            installation: r.installation.clone(),
                            remote: r.name.clone(),
                            refs: ids.clone(),
                        }),
                    ));
                }
            }
            Dialog::Updates(p, Ok(status)) if status.update_available && !status.masked => {
                list.push(control(
                    "dialog.review",
                    "Review update",
                    Message::Ask(Operation::Update(vec![p.clone()])),
                ));
            }
            Dialog::Version(p) => {
                if let Some(Ok(commits)) = &self.history {
                    for c in commits {
                        list.push(control(
                            format!("version.commit:{}", c.commit),
                            format!("{} ({})", c.subject, c.date),
                            Message::CommitChoice(c.commit.clone()),
                        ));
                    }
                }
                list.push(control(
                    "version.hold",
                    "Block future updates after changing version",
                    Message::Hold(!self.hold),
                ));
                list.push(control(
                    "dialog.review",
                    "Review version change",
                    Message::Ask(Operation::Version(
                        p.clone(),
                        self.commit.trim().into(),
                        self.hold,
                    )),
                ));
            }
            Dialog::NewSnapshot => {
                let candidates = self.snapshot_candidates();
                let all = !candidates.is_empty()
                    && candidates
                        .iter()
                        .all(|d| self.snapshot_pick.contains(&d.id));
                for d in &candidates {
                    let picked = self.snapshot_pick.contains(&d.id);
                    list.push(control(
                        format!("snapshot.pick:{}", d.id),
                        format!(
                            "{} {}",
                            if picked { "Deselect" } else { "Select" },
                            self.data_name(d)
                        ),
                        Message::SnapshotPick(d.id.clone(), !picked),
                    ));
                }
                if !candidates.is_empty() {
                    list.push(control(
                        "snapshot.select-all",
                        if all { "Deselect All" } else { "Select All" },
                        Message::SnapshotPickAll(!all),
                    ));
                }
                if !self.snapshot_pick.is_empty() && !self.snapshot_name.trim().is_empty() {
                    list.push(guarded(
                        "snapshot.create",
                        "Create",
                        Message::CreateSnapshot,
                        Guard::Apply,
                    ));
                }
            }
            Dialog::ImportSnapshot => {
                list.push(control(
                    "snapshot.choose-file",
                    "Choose .tar.zst file",
                    Message::ChooseFile(FilePurpose::Snapshot),
                ));
                if let Some(path) = &self.import_path {
                    list.push(control(
                        "dialog.review",
                        "Review import",
                        Message::Ask(Operation::ImportSnapshot {
                            id: self.import_id.trim().into(),
                            path: path.clone(),
                            name: self.snapshot_name.clone(),
                        }),
                    ));
                }
            }
            Dialog::Rename(s) => list.push(control(
                "dialog.review",
                "Review rename",
                Message::Ask(Operation::RenameSnapshot(
                    s.clone(),
                    self.snapshot_name.clone(),
                )),
            )),
            _ => (),
        }
        list
    }
    fn package_controls(&self, list: &mut Vec<Control>) {
        list.push(control("packages.filters", "Filters", Message::Filters));
        for p in self.visible_packages() {
            let key = p.key();
            list.push(control(
                format!("package:{key}"),
                format!("{} ({})", p.name, p.installation),
                if self.batch {
                    Message::Toggle(key.clone(), !self.selection.contains(&key))
                } else {
                    Message::Select(key)
                },
            ));
        }
        if self.filters {
            let v = &self.view;
            list.push(control(
                "filter.applications",
                "Show applications",
                Message::Applications(!v.applications),
            ));
            list.push(control(
                "filter.runtimes",
                "Show runtimes",
                Message::Runtimes(!v.runtimes),
            ));
            list.push(control(
                "filter.eol",
                "End of life only",
                Message::Eol(!v.eol_only),
            ));
            list.push(control(
                "filter.installation:all",
                "All installations",
                Message::FilterInstallation(String::new()),
            ));
            for name in &self.inventory.installations {
                list.push(control(
                    format!("filter.installation:{name}"),
                    format!("Installation {name}"),
                    Message::FilterInstallation(name.clone()),
                ));
            }
            for (key, label) in self.remote_filter_options() {
                let on = v.remotes.contains(&key);
                list.push(control(
                    format!("filter.remote:{key}"),
                    format!("Remote {label}"),
                    Message::FilterRemote(key, !on),
                ));
            }
            for runtime in self.runtime_options() {
                let on = v.runtime_refs.contains(&runtime);
                list.push(control(
                    format!("filter.runtime:{runtime}"),
                    format!("Runtime {runtime}"),
                    Message::FilterRuntime(runtime, !on),
                ));
            }
            for (name, sort) in [
                ("name", PackageSort::Name),
                ("id", PackageSort::Id),
                ("remote", PackageSort::Remote),
            ] {
                list.push(control(
                    format!("filter.sort:{name}"),
                    format!("Sort by {name}"),
                    Message::PackageSort(sort),
                ));
            }
            if !v.filters_are_default() {
                list.push(control(
                    "filters.reset",
                    "Reset Filters",
                    Message::ResetFilters,
                ));
            }
        }
        if self.batch {
            self.batch_controls(list);
            return;
        }
        let Some(p) = self.package() else {
            return;
        };
        if !p.is_runtime() {
            list.push(control(
                "package.open",
                format!("Open {}", p.name),
                Message::Launch(p.clone()),
            ));
        }
        list.push(control(
            "package.uninstall",
            "Uninstall…",
            Message::Uninstall(vec![p.clone()]),
        ));
        list.push(control(
            "package.change-version",
            "Change Version…",
            Message::History,
        ));
        list.push(control(
            "package.check-updates",
            "Check for Updates",
            Message::CheckUpdates(p.clone()),
        ));
        list.push(control(
            "package.change-source",
            "Change Source…",
            Message::ChooseSource(p.clone()),
        ));
        if let Some(runtime) = self.runtime_package(&p.runtime, &p.installation) {
            list.push(control(
                "package.show-runtime",
                format!("Show runtime {}", runtime.name),
                Message::ShowRuntime(runtime.key()),
            ));
        }
        list.push(control(
            "package.mask",
            if p.masked {
                "Allow automatic updates"
            } else {
                "Block automatic updates"
            },
            Message::Ask(Operation::Mask(p.clone(), !p.masked)),
        ));
        if p.is_runtime() {
            list.push(control(
                "package.pin",
                if p.pinned {
                    "Unpin runtime"
                } else {
                    "Pin runtime"
                },
                Message::Ask(Operation::Pin(p.clone(), !p.pinned)),
            ));
        } else {
            if self.inventory.data.iter().any(|d| d.id == p.id) {
                list.push(control(
                    "package.data.open",
                    "Open user data",
                    Message::Open(self.paths.app_data(&p.id).unwrap_or_default()),
                ));
                list.push(control(
                    "package.data.trash",
                    "Move data to Trash…",
                    Message::Ask(Operation::TrashData(vec![p.id.clone()])),
                ));
            }
            list.push(control(
                "package.snapshot",
                "New Snapshot…",
                Message::SnapshotFor(p.id.clone()),
            ));
            list.push(control(
                "package.view-snapshots",
                "View Snapshots",
                Message::ViewSnapshots(p.id.clone()),
            ));
            list.push(control(
                "package.copy-launch",
                "Copy Launch Command",
                Message::Copy(format!(
                    "flatpak run {} {}",
                    backend::scope(&p.installation),
                    p.full_ref()
                )),
            ));
        }
        if let Some(remote) = self.origin_remote(p) {
            list.push(control(
                "package.reinstall",
                "Reinstall…",
                Message::Ask(Operation::Reinstall {
                    package: p.clone(),
                    remote: remote.clone(),
                }),
            ));
        }
    }
    fn batch_controls(&self, list: &mut Vec<Control>) {
        list.push(control(
            "batch.select-all",
            "Select All Visible",
            Message::SelectAll,
        ));
        if self.selection.is_empty() {
            return;
        }
        match self.page {
            Page::Packages => {
                list.push(control(
                    "batch.copy-names",
                    "Copy Names",
                    Message::Copy(self.copy_packages(|p| p.name.clone())),
                ));
                list.push(control(
                    "batch.copy-ids",
                    "Copy IDs",
                    Message::Copy(self.copy_packages(|p| p.id.clone())),
                ));
                list.push(control(
                    "batch.copy-refs",
                    "Copy Refs",
                    Message::Copy(self.copy_packages(Package::full_ref)),
                ));
                list.push(control(
                    "batch.update",
                    "Update Selected",
                    Message::BatchUpdate,
                ));
                list.push(control(
                    "batch.uninstall",
                    "Uninstall…",
                    Message::BatchUninstall,
                ));
            }
            Page::Snapshots => {
                list.push(control(
                    "batch.copy-ids",
                    "Copy IDs",
                    Message::Copy(self.selected_ids().join("\n")),
                ));
                list.push(control("batch.restore", "Restore", Message::BatchRestore));
                list.push(control(
                    "batch.install",
                    "Install Apps…",
                    Message::BatchInstall,
                ));
                list.push(control("batch.trash", "Trash", Message::BatchSnapshotTrash));
                return;
            }
            _ => {
                list.push(control(
                    "batch.copy-ids",
                    "Copy IDs",
                    Message::Copy(self.selected_data().join("\n")),
                ));
                if self.leftovers {
                    list.push(control(
                        "batch.find",
                        "Find in a Remote",
                        Message::FindPackage(self.selected_data().join(" ")),
                    ));
                }
            }
        }
        list.push(control(
            "batch.snapshot",
            "Create Snapshot…",
            Message::BatchSnapshot,
        ));
        list.push(control(
            "batch.trash-data",
            "Trash Data",
            Message::BatchTrash,
        ));
    }
    fn remote_controls(&self, list: &mut Vec<Control>) {
        list.push(control(
            "remotes.show-disabled",
            if self.show_disabled_remotes {
                "Hide disabled remotes"
            } else {
                "Show disabled remotes"
            },
            Message::ShowDisabledRemotes(!self.show_disabled_remotes),
        ));
        list.push(control(
            "remotes.add-custom",
            "Add a custom remote",
            Message::AddRemote,
        ));
        list.push(control(
            "remotes.add-file",
            "Add a repo file",
            Message::AddRemoteFile,
        ));
        for (index, preset) in remotes::POPULAR.iter().enumerate() {
            if !remotes::added(preset, &self.inventory.remotes) {
                list.push(control(
                    format!("remotes.preset:{}", preset.name),
                    format!("Add {}", preset.title),
                    Message::PopularRemote(index),
                ));
            }
        }
        for r in self
            .inventory
            .remotes
            .iter()
            .filter(|r| remotes::visible(r, &self.query, self.show_disabled_remotes))
        {
            let prefix = format!("remote:{}:{}", r.installation, r.name);
            let key = (r.installation.clone(), r.name.clone());
            list.push(control(
                format!("{prefix}:menu"),
                format!("Actions for {} ({})", remotes::title(r), r.installation),
                Message::RemoteMenu((self.remote_menu.as_ref() != Some(&key)).then_some(key)),
            ));
            list.push(control(
                format!("{prefix}:filter"),
                format!("Show installed packages from {}", remotes::title(r)),
                Message::BrowseRemote(r.clone()),
            ));
        }
        let Some(r) = self.remote_menu.as_ref().and_then(|(installation, name)| {
            self.inventory
                .remotes
                .iter()
                .find(|r| &r.installation == installation && &r.name == name)
        }) else {
            return;
        };
        list.push(control(
            "remote.menu.details",
            "Details",
            Message::RemoteDetails(r.clone()),
        ));
        if !r.disabled() {
            list.push(control(
                "remote.menu.search",
                "Search packages",
                Message::InstallBrowse(Some((r.installation.clone(), r.name.clone()))),
            ));
        }
        list.push(control(
            "remote.menu.copy-name",
            "Copy name",
            Message::Copy(r.name.clone()),
        ));
        if !r.url.is_empty() {
            list.push(control(
                "remote.menu.copy-url",
                "Copy URL",
                Message::Copy(r.url.clone()),
            ));
        }
        list.push(control(
            "remote.menu.toggle",
            if r.disabled() { "Enable" } else { "Disable" },
            Message::Ask(Operation::RemoteEnabled(r.clone(), r.disabled())),
        ));
        list.push(control(
            "remote.menu.remove",
            "Remove remote…",
            Message::Ask(Operation::RemoteRemove(r.clone())),
        ));
    }
    fn data_controls(&self, list: &mut Vec<Control>) {
        list.push(control(
            "data.active",
            "Active Data",
            Message::Leftovers(false),
        ));
        list.push(control(
            "data.leftover",
            "Leftover Data",
            Message::Leftovers(true),
        ));
        for (name, sort) in [
            ("name", DataSort::Name),
            ("id", DataSort::Id),
            ("size", DataSort::Size),
        ] {
            list.push(control(
                format!("data.sort:{name}"),
                format!("Sort by {name}"),
                Message::DataSort(sort),
            ));
        }
        list.push(control(
            "data.sort-direction",
            if self.view.data_descending {
                "Sort ascending"
            } else {
                "Sort descending"
            },
            Message::DataDescending(!self.view.data_descending),
        ));
        for d in self.visible_data() {
            let prefix = format!("data:{}", d.id);
            let name = self.data_name(d);
            if self.batch {
                let on = self.selection.contains(&d.id);
                list.push(control(
                    format!("{prefix}:toggle"),
                    format!("{} {name}", if on { "Deselect" } else { "Select" }),
                    Message::Toggle(d.id.clone(), !on),
                ));
                continue;
            }
            list.push(control(
                format!("{prefix}:copy"),
                format!("Copy ID of {name}"),
                Message::Copy(d.id.clone()),
            ));
            list.push(control(
                format!("{prefix}:open"),
                format!("Open folder of {name}"),
                Message::Open(self.paths.app_data(&d.id).unwrap_or_default()),
            ));
            list.push(control(
                format!("{prefix}:snapshot"),
                format!("New snapshot of {name}"),
                Message::SnapshotFor(d.id.clone()),
            ));
            list.push(control(
                format!("{prefix}:trash"),
                format!("Move data of {name} to Trash…"),
                Message::Ask(Operation::TrashData(vec![d.id.clone()])),
            ));
            if !d.installed {
                list.push(control(
                    format!("{prefix}:find"),
                    format!("Find {name} in a remote"),
                    Message::FindPackage(d.id.clone()),
                ));
            }
        }
        if self.batch {
            self.batch_controls(list);
        }
    }
    fn snapshot_controls(&self, list: &mut Vec<Control>) {
        list.push(control(
            "snapshots.new",
            "New Snapshot…",
            Message::NewSnapshot,
        ));
        list.push(control(
            "snapshots.import",
            "Import…",
            Message::ImportSnapshot,
        ));
        list.push(control(
            "snapshots.open-library",
            "Open Snapshot Folder",
            Message::Open(self.paths.snapshots()),
        ));
        for (id, items) in self.snapshot_groups() {
            if !self.app_installed(&id) {
                list.push(control(
                    format!("snapshots.group:{id}:install"),
                    format!("Install {}", self.app_name(&id)),
                    Message::FindPackage(id.clone()),
                ));
            }
            for s in items {
                let prefix = format!("snapshot:{}", s.path.display());
                if self.batch {
                    let key = s.path.display().to_string();
                    let on = self.selection.contains(&key);
                    list.push(control(
                        format!("{prefix}:toggle"),
                        format!("{} {}", if on { "Deselect" } else { "Select" }, s.name),
                        Message::Toggle(key, !on),
                    ));
                    continue;
                }
                list.push(control(
                    format!("{prefix}:restore"),
                    format!("Restore {}…", s.name),
                    Message::Ask(Operation::Restore(s.clone())),
                ));
                list.push(control(
                    format!("{prefix}:export"),
                    format!("Export {}…", s.name),
                    Message::ChooseFile(FilePurpose::Export(s.clone())),
                ));
                if s.official {
                    list.push(control(
                        format!("{prefix}:import-copy"),
                        format!("Import a copy of {}", s.name),
                        Message::Ask(Operation::ImportSnapshot {
                            id: s.id.clone(),
                            path: s.path.clone(),
                            name: s.name.clone(),
                        }),
                    ));
                } else {
                    list.push(control(
                        format!("{prefix}:rename"),
                        format!("Rename {}…", s.name),
                        Message::RenameSnapshot(s.clone()),
                    ));
                    list.push(control(
                        format!("{prefix}:trash"),
                        format!("Trash {}…", s.name),
                        Message::Ask(Operation::TrashSnapshot(s.clone())),
                    ));
                }
            }
        }
        if self.batch {
            self.batch_controls(list);
        }
    }
    fn install_controls(&self, list: &mut Vec<Control>) {
        if let Some((installation, remote)) = &self.install_target {
            list.push(control(
                "install.back",
                "Back to Install Packages",
                Message::InstallBrowse(None),
            ));
            if !self.searching {
                list.push(control("install.search", "Search", Message::InstallSearch));
            }
            for (name, reference) in self.results.iter().take(200) {
                if self.is_installed(installation, reference) {
                    continue;
                }
                let key = (installation.clone(), remote.clone(), reference.clone());
                let queued = self.queue.contains_key(&key);
                list.push(control(
                    format!("install.result:{reference}"),
                    format!("{} {name}", if queued { "Remove" } else { "Add" }),
                    Message::Queue(key, !queued),
                ));
            }
        } else {
            list.push(control(
                "install.choose-file",
                "Open Files…",
                Message::ChooseFile(FilePurpose::Install),
            ));
            for r in self.enabled_remotes() {
                list.push(control(
                    format!("install.remote:{}:{}", r.installation, r.name),
                    format!("Search {} ({})", remotes::title(r), r.installation),
                    Message::InstallBrowse(Some((r.installation.clone(), r.name.clone()))),
                ));
            }
            for (key, name) in &self.queue {
                list.push(control(
                    format!("install.queue.remove:{}:{}:{}", key.0, key.1, key.2),
                    format!("Remove {name} from pending"),
                    Message::Queue(key.clone(), false),
                ));
            }
            if !self.queue.is_empty() {
                list.push(control(
                    "install.queue.clear",
                    "Remove All",
                    Message::ClearQueue,
                ));
            }
        }
        if !self.queue.is_empty() {
            list.push(control(
                "install.queue.review",
                format!("Install {} pending…", self.queue.len()),
                Message::InstallQueue,
            ));
        }
    }
    fn ui_dialog_state(&self) -> Value {
        let Some(dialog) = &self.dialog else {
            return Value::Null;
        };
        match dialog {
            Dialog::InstallFiles(files) => json!({"kind":"install-files","files":files,
                "installation":self.inventory.installations.get(self.install_scope)}),
            Dialog::Confirm(op) => {
                json!({"kind":"confirmation","title":op.title(),"action":op.verb(),
                "destructive":op.destructive(),"explanation":op.explanation().trim(),
                "commands":op.command_lines(),"requires_system":system_operation(op)})
            }
            Dialog::Uninstall(packages) => json!({"kind":"uninstall",
                "packages":packages.iter().map(|p| json!({"name":p.name,"ref":p.full_ref(),"installation":p.installation})).collect::<Vec<_>>(),
                "app_data":if self.uninstall_data { "trash" } else { "keep" }}),
            Dialog::Remote => {
                json!({"kind":"add-remote","name":self.remote_name,"url":self.remote_url,
                "installation":self.inventory.installations.get(self.install_scope)})
            }
            Dialog::RemoteDetails(r) => json!({"kind":"remote-details","remote":r}),
            Dialog::About => json!({"kind":"about"}),
            Dialog::Message(title, body) => json!({"kind":"message","title":title,"body":body}),
            Dialog::Version(p) => json!({"kind":"version","package":p.full_ref(),
                "history":match &self.history { None => json!("loading"), Some(Ok(c)) => json!(c), Some(Err(e)) => json!({"error":e}) },
                "commit":self.commit,"block_updates":self.hold}),
            Dialog::Source(p) => {
                json!({"kind":"source","package":p.full_ref(),"current":p.origin,"choice":self.remote_choice})
            }
            Dialog::Updates(p, result) => json!({"kind":"updates","package":p.full_ref(),
                "status":match result { Ok(s) => json!(s), Err(e) => json!({"message":e}) }}),
            Dialog::InstallIds(ids) => json!({"kind":"install-ids","ids":ids,
                "remote":self.enabled_remotes().get(self.remote_choice).map(|r| format!("{}:{}", r.installation, r.name))}),
            Dialog::NewSnapshot => {
                json!({"kind":"new-snapshot","name":self.snapshot_name,"search":self.snapshot_query,
                "selected":self.snapshot_pick,
                "apps":self.snapshot_candidates().iter().map(|d| json!({"id":d.id,"name":self.data_name(d),"installed":d.installed,"bytes":d.bytes})).collect::<Vec<_>>()})
            }
            Dialog::ImportSnapshot => {
                json!({"kind":"import-snapshot","id":self.import_id,"name":self.snapshot_name,"path":self.import_path})
            }
            Dialog::Rename(s) => {
                json!({"kind":"rename-snapshot","snapshot":s.path,"name":self.snapshot_name})
            }
        }
    }
    /// What the current page shows, in the same order as the rendered list.
    fn ui_page_state(&self) -> Value {
        match self.page {
            Page::Packages => json!({
                "filters_open":self.filters, "filters":self.view, "batch":self.batch,
                "selection":self.selection,
                "packages":self.visible_packages().iter().map(|p| json!({"key":p.key(),"name":p.name,"id":p.id,"ref":p.full_ref(),"installation":p.installation,"origin":p.origin,"version":p.version,"masked":p.masked,"pinned":p.pinned,"eol":p.eol()})).collect::<Vec<_>>(),
                "selected":self.package().filter(|_| !self.batch).map(|p| json!({"package":p,
                    "data_bytes":self.inventory.data.iter().find(|d| d.id == p.id).map(|d| d.bytes),
                    "details":self.detail})),
            }),
            Page::Data => json!({"filter":if self.leftovers { "leftover" } else { "active" },
                "sort":self.view.data_sort,"descending":self.view.data_descending,"batch":self.batch,"selection":self.selection,
                "folders":self.visible_data().iter().map(|d| json!({"id":d.id,"name":self.data_name(d),"bytes":d.bytes,"installed":d.installed,"error":d.error})).collect::<Vec<_>>()}),
            Page::Snapshots => json!({"batch":self.batch,"selection":self.selection,
                "groups":self.snapshot_groups().iter().map(|(id, items)| json!({"id":id,"name":self.app_name(id),"installed":self.app_installed(id),
                    "snapshots":items.iter().map(|s| json!({"name":s.name,"path":s.path,"timestamp":s.timestamp,"version":s.version,"bytes":s.bytes,"official":s.official})).collect::<Vec<_>>()})).collect::<Vec<_>>()}),
            Page::Install => json!({
                "target":self.install_target.as_ref().map(|(installation, remote)| json!({"installation":installation,"remote":remote})),
                "query":self.install_query,"searching":self.searching,"searched":self.searched,
                "remotes":self.enabled_remotes().iter().map(|r| json!({"name":r.name,"title":remotes::title(r),"installation":r.installation})).collect::<Vec<_>>(),
                "results":self.results.iter().take(200).map(|(name, reference)| {
                    let (installation, remote) = self.install_target.clone().unwrap_or_default();
                    json!({"name":name,"ref":reference,"installed":self.is_installed(&installation, reference),
                        "queued":self.queue.contains_key(&(installation, remote, reference.clone()))})
                }).collect::<Vec<_>>(),
                "result_count":self.results.len(),
                "queue":self.queue.iter().map(|(k, name)| json!({"installation":k.0,"remote":k.1,"ref":k.2,"name":name})).collect::<Vec<_>>(),
            }),
            Page::Remotes => {
                json!({"show_disabled":self.show_disabled_remotes,"menu":self.remote_menu,
                "presets":remotes::POPULAR.iter().map(|p| json!({"name":p.name,"title":p.title,"added":remotes::added(p, &self.inventory.remotes)})).collect::<Vec<_>>()})
            }
        }
    }
    pub(super) fn ui_state(&self) -> Value {
        let modal = self.dialog.as_ref().map(|dialog| match dialog {
            Dialog::InstallFiles(_) => "install-files",
            Dialog::Confirm(_) => "confirmation",
            Dialog::Uninstall(_) => "uninstall",
            Dialog::Remote => "add-remote",
            Dialog::RemoteDetails(_) => "remote-details",
            Dialog::About => "about",
            Dialog::Message(..) => "message",
            Dialog::Version(_) => "version",
            Dialog::Source(_) => "source",
            Dialog::Updates(..) => "updates",
            Dialog::InstallIds(_) => "install-ids",
            Dialog::NewSnapshot => "new-snapshot",
            Dialog::ImportSnapshot => "import-snapshot",
            Dialog::Rename(_) => "rename-snapshot",
        });
        let files = match &self.dialog {
            Some(Dialog::InstallFiles(files)) => files.clone(),
            _ => vec![],
        };
        let flatpak_instance = std::fs::read_to_string("/.flatpak-info")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix("instance-id=").map(str::to_string))
            });
        let controls = self.ui_controls();
        json!({"instance":self.ui_instance, "process_id":std::process::id(), "flatpak_instance":flatpak_instance,
            "page":self.page.name(), "page_id":self.page.slug(), "theme":self.preferences.theme, "rendered_theme_dark":cosmic::theme::active().theme_type.is_dark(), "settings_open":self.core.window.show_context,
            "modal":modal, "dialog":self.ui_dialog_state(), "loading":self.loading, "detail_loading":self.detail_loading, "busy":self.job.is_some(), "operation":self.job.as_ref().map(|_| &self.job_title), "sidebar_visible":self.core.nav_bar_active(),
            "window":self.window_state, "pointer":self.ui_pointer, "pointer_serial":self.ui_pointer_serial, "search":self.query, "notice":self.notice, "toast":self.last_toast, "drag_over":self.drag_over,
            "show_disabled_remotes":self.show_disabled_remotes, "remote_menu":self.remote_menu,
            "data_filter":if self.leftovers { "leftover" } else { "active" },
            "remotes":self.inventory.remotes.iter().filter(|r| remotes::visible(r, &self.query, self.show_disabled_remotes)).map(|r| json!({"name":r.name,"title":remotes::title(r),"installation":r.installation,"disabled":r.disabled()})).collect::<Vec<_>>(),
            "selected_package":self.selected, "visible_packages":self.visible_packages().iter().map(|p| json!({"id":p.id,"installation":p.installation,"origin":p.origin,"key":p.key()})).collect::<Vec<_>>(),
            "install_files":files, "installation":self.inventory.installations.get(self.install_scope),
            "view":self.ui_page_state(),
            "fields":FIELDS.iter().map(|(id, label)| json!({"id":id,"label":label})).collect::<Vec<_>>(),
            "controls":controls.iter().map(|c| json!({"id":c.id,"label":c.label,"requires":match c.guard {
                Guard::None => "none", Guard::Apply | Guard::Scoped(false) => "apply", Guard::Scoped(true) => "apply+allow-system" }})).collect::<Vec<_>>(),
            "actions":controls.iter().map(|c| c.id.clone()).chain(std::iter::once("escape".to_string())).collect::<Vec<_>>()})
    }
    pub(super) fn ui_activate(
        &self,
        target: &str,
        apply: bool,
        allow_system: bool,
    ) -> Result<Message, String> {
        let Some(control) = self.ui_controls().into_iter().find(|c| c.id == target) else {
            return Err(format!(
                "Control {target:?} is not available in the current view; inspect controls first"
            ));
        };
        match control.guard {
            Guard::None => (),
            Guard::Apply | Guard::Scoped(false) if !apply => {
                return Err(format!("{} requires --apply", control.label));
            }
            Guard::Scoped(true) if !apply => {
                return Err(format!(
                    "{} changes a system installation and requires --apply --allow-system",
                    control.label
                ));
            }
            Guard::Scoped(true) if !allow_system => {
                return Err(format!(
                    "{} changes a system installation and also requires --allow-system",
                    control.label
                ));
            }
            _ => (),
        }
        Ok(control.message)
    }
    pub(super) fn ui_request(&mut self, pending: Pending) -> Task<Message> {
        let failure = |message: String| {
            pending.respond(protocol::error(message));
            Task::none()
        };
        match pending.envelope.request.clone() {
            Request::Present => {
                pending.respond(protocol::success(json!({"state":self.ui_state()})));
                self.present_window()
            }
            Request::ActivationToken => {
                let Some(window) = self.core.main_window_id() else {
                    return failure("No main window".into());
                };
                iced::platform_specific::shell::commands::activation::request_token(
                    Some(Self::APP_ID.into()),
                    Some(window),
                )
                .map(move |token| {
                    cosmic::Action::App(Message::UiActivationToken(pending.clone(), token))
                })
            }
            Request::Inspect | Request::Tree => iced::advanced::widget::operate(Tree {
                target: None,
                nodes: vec![],
            })
            .map(move |nodes| cosmic::Action::App(Message::UiTree(pending.clone(), nodes, false))),
            Request::Resolve { target } => {
                let operation = Tree {
                    target: Some(widget::Id::new(target)),
                    nodes: vec![],
                };
                iced::advanced::widget::operate(operation).map(move |nodes| {
                    cosmic::Action::App(Message::UiTree(pending.clone(), nodes, true))
                })
            }
            Request::Activate { target } => {
                if target == "escape" {
                    let task = self.on_escape();
                    pending.respond(protocol::success(json!({"state":self.ui_state()})));
                    return task;
                }
                match self.ui_activate(
                    &target,
                    pending.envelope.apply,
                    pending.envelope.allow_system,
                ) {
                    Ok(message) => {
                        let task = self.update(message);
                        pending.respond(protocol::success(json!({"state":self.ui_state()})));
                        task
                    }
                    Err(error) => failure(error),
                }
            }
            Request::Open { files } => {
                if self.job.is_some() {
                    return failure("An operation is running".into());
                }
                if files.is_empty()
                    || files
                        .iter()
                        .any(|path| !path.is_absolute() || !path.is_file())
                {
                    return failure("Choose existing absolute file paths".into());
                }
                let task = self.update(Message::FilesPicked(Ok(files)));
                pending.respond(protocol::success(json!({"state":self.ui_state()})));
                task
            }
            Request::Drop { uris } => {
                if self.job.is_some() {
                    return failure("An operation is running".into());
                }
                // Same handler as a compositor drop of text/uri-list data.
                let data = uris.join("\r\n").into_bytes();
                let task = self.update(Message::Dropped("text/uri-list".into(), data));
                pending.respond(protocol::success(json!({"state":self.ui_state()})));
                task
            }
            Request::Fill { target, text } => {
                if self.job.is_some() {
                    return failure("An operation is running".into());
                }
                let message = match target.as_str() {
                    "search" => Message::Search(text),
                    "install.query" => Message::Field(Field::InstallQuery, text),
                    "remote.name" => Message::Field(Field::RemoteName, text),
                    "remote.url" => Message::Field(Field::RemoteUrl, text),
                    "version.commit" => Message::Field(Field::Commit, text),
                    "snapshot.name" => Message::Field(Field::SnapshotName, text),
                    "snapshot.search" => Message::Field(Field::SnapshotSearch, text),
                    "import.id" => Message::Field(Field::ImportId, text),
                    _ => {
                        return failure(format!(
                            "Unknown text field. Use one of: {}",
                            FIELDS.map(|(id, _)| id).join(", ")
                        ));
                    }
                };
                let task = self.update(message);
                pending.respond(protocol::success(json!({"state":self.ui_state()})));
                task
            }
            Request::Theme { theme } => {
                let index = match theme.as_str() {
                    "system" | "desktop" => 0,
                    "dark" => 1,
                    "light" => 2,
                    _ => return failure("Use system, dark or light".into()),
                };
                let task = self.update(Message::Theme(index));
                pending.respond(protocol::success(json!({"state":self.ui_state()})));
                task
            }
            Request::Capture { path, overwrite } => {
                if !path.is_absolute() || (!overwrite && path.exists()) {
                    return failure(
                        "Capture needs a new absolute output path, or explicit --overwrite".into(),
                    );
                }
                let Some(window) = self.core.main_window_id() else {
                    return failure("No main window".into());
                };
                iced::window::screenshot(window).map(move |shot| {
                    cosmic::Action::App(Message::UiCaptured(
                        pending.clone(),
                        path.clone(),
                        overwrite,
                        shot,
                    ))
                })
            }
            Request::Window {
                width,
                height,
                mode,
            } => {
                self.initial_window_mode = None;
                let Some(window) = self.core.main_window_id() else {
                    return failure("No main window".into());
                };
                let task: iced::Task<Message> = if let Some(mode) = mode {
                    match mode.as_str() {
                        "windowed" => iced::Task::batch([
                            iced::window::set_mode(window, iced::window::Mode::Windowed),
                            iced::window::maximize(window, false),
                        ]),
                        "maximized" => iced::Task::batch([
                            iced::window::set_mode(window, iced::window::Mode::Windowed),
                            iced::window::maximize(window, true),
                        ]),
                        "fullscreen" => {
                            iced::window::set_mode(window, iced::window::Mode::Fullscreen)
                        }
                        _ => return failure("Use windowed, maximized or fullscreen".into()),
                    }
                } else {
                    let width = width.unwrap_or(self.window_state.width);
                    let height = height.unwrap_or(self.window_state.height);
                    if !(940..=16384).contains(&width) || !(560..=16384).contains(&height) {
                        return failure(
                            "Window dimensions must fit 940..16384 by 560..16384 logical pixels"
                                .into(),
                        );
                    }
                    iced::window::resize(window, iced::Size::new(width as f32, height as f32))
                };
                pending.respond(protocol::success(
                    json!({"accepted":true,"state":self.ui_state()}),
                ));
                task.map(cosmic::Action::App)
            }
            Request::Close => {
                if !pending.envelope.apply {
                    return failure("Closing the selected GUI requires --apply".into());
                }
                if self.job.is_some() {
                    return failure(
                        "Wait for or cancel the running operation before closing".into(),
                    );
                }
                self.save_window();
                pending.respond(protocol::success(
                    json!({"closing":true,"instance":self.ui_instance}),
                ));
                cosmic::task::future(async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Message::UiCloseNow
                })
            }
        }
    }
    pub(super) fn ui_tree(&self, pending: Pending, nodes: Vec<Node>, resolve: bool) {
        if resolve {
            if let Some(node) = nodes.first() {
                pending.respond(protocol::success(json!({"bounds":node.bounds})));
            } else {
                pending.respond(protocol::error(
                    "The target has no rendered widget in this view",
                ));
            }
        } else {
            pending.respond(protocol::success(
                json!({"state":self.ui_state(),"tree":nodes}),
            ));
        }
    }
    pub(super) fn ui_capture(
        &self,
        pending: Pending,
        path: PathBuf,
        overwrite: bool,
        shot: iced::window::Screenshot,
    ) {
        let result = (|| -> Result<(), String> {
            let image =
                image::RgbaImage::from_raw(shot.size.width, shot.size.height, shot.rgba.to_vec())
                    .ok_or("Invalid renderer screenshot")?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true);
            if overwrite {
                options.create(true).truncate(true);
            } else {
                options.create_new(true);
            }
            let mut file = options.open(&path).map_err(|e| e.to_string())?;
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut file, image::ImageFormat::Png)
                .map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => pending.respond(protocol::success(
                json!({"path":path,"width":shot.size.width,"height":shot.size.height}),
            )),
            Err(error) => pending.respond(protocol::error(error)),
        }
    }
}
