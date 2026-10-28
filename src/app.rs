// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    backend::{self, DataFolder, Inventory, Job, Operation, Package, Paths, Remote},
    preferences::{self, AppTheme, Preferences},
    remotes,
    snapshots::{self, Snapshot},
    window_state::{self, WindowState},
};
use cosmic::{
    Application,
    app::Task,
    iced::{self, Alignment, Length, Subscription},
    prelude::*,
    widget::{self, icon, nav_bar},
};
use std::{
    collections::{BTreeSet, HashSet},
    path::PathBuf,
    sync::atomic::Ordering,
    time::Duration,
};
#[path = "ui_view.rs"]
mod ui_view;

// Header tooltips need room below the title bar. Position::Top is clamped
// against the window edge and would cover the button that triggered it.
fn header_tooltip<'a>(
    button: impl Into<Element<'a, Message>>,
    label: &'a str,
) -> Element<'a, Message> {
    widget::tooltip(
        button,
        widget::text(label),
        widget::tooltip::Position::Bottom,
    )
    .gap(8)
    .into()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Packages,
    Remotes,
    Data,
    Snapshots,
    Install,
}
impl Page {
    fn name(self) -> &'static str {
        match self {
            Self::Packages => "Packages",
            Self::Remotes => "Remotes",
            Self::Data => "User Data",
            Self::Snapshots => "Snapshots",
            Self::Install => "Install Packages",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Field {
    RemoteName,
    RemoteUrl,
    InstallQuery,
    SnapshotName,
    ImportId,
    Commit,
}
#[derive(Debug, Clone)]
pub enum Message {
    UiRequest(crate::ui_bridge::Pending),
    UiTree(crate::ui_bridge::Pending, Vec<crate::ui_bridge::Node>, bool),
    UiCaptured(
        crate::ui_bridge::Pending,
        PathBuf,
        bool,
        iced::window::Screenshot,
    ),
    UiPointer(iced::Point),
    UiActivationToken(crate::ui_bridge::Pending, Option<String>),
    UiCloseNow,
    CheckUpdates(Package),
    UpdateChecked(Package, Result<backend::UpdateStatus, String>),
    ChooseSource(Package),
    SourceChoice(usize),
    BatchUpdate,
    Key(iced::keyboard::Key, iced::keyboard::Modifiers),
    Captured(iced::window::Screenshot),
    WindowChanged(iced::window::Id),
    WindowObserved(u64, iced::window::Mode, bool, iced::Size),
    InstallFilesConfirmed,
    Refresh,
    Loaded(Result<Inventory, String>),
    Tick,
    Search(String),
    Select(String),
    Toggle(String, bool),
    Batch(bool),
    SelectAll,
    Copy(String),
    Page(Page),
    Filters,
    Applications(bool),
    Runtimes(bool),
    Eol(bool),
    Origin(usize),
    Runtime(usize),
    Installation(usize),
    Sort(usize),
    Leftovers(bool),
    Details(String, Result<String, String>),
    History,
    HistoryLoaded(String, Result<String, String>),
    Field(Field, String),
    Hold(bool),
    Ask(Operation),
    Confirm,
    CloseDialog,
    Finished(Result<String, String>),
    Cancel,
    ClearNotice,
    BatchUninstall,
    BatchTrash,
    BatchSnapshot,
    BatchRestore,
    BatchSnapshotTrash,
    BatchInstall,
    UninstallData(bool),
    Launch(Package),
    Open(PathBuf),
    AddRemote,
    AddRemoteFile,
    PopularRemote(usize),
    ShowDisabledRemotes(bool),
    RemoteMenu(Option<(String, String)>),
    RemoteDetails(Remote),
    BrowseRemote(Remote),
    InstallSearch,
    Results(Result<Vec<(String, String)>, String>),
    InstallRemote(usize),
    InstallScope(usize),
    Queue(String, bool),
    InstallQueue,
    ChooseFile(FilePurpose),
    FilePicked(FilePurpose, Result<PathBuf, String>),
    FilesPicked(Result<Vec<PathBuf>, String>),
    NewSnapshot,
    SnapshotFor(String),
    FindPackage(String),
    ImportSnapshot,
    RenameSnapshot(Snapshot),
    About,
    ToggleSidebar,
    Settings,
    CloseSettings,
    Theme(usize),
    BusyExit,
}
#[derive(Default)]
pub struct Flags {
    pub preview: Option<String>,
    pub capture: Option<PathBuf>,
    pub open_files: Vec<PathBuf>,
    pub window_state: WindowState,
    pub activation_action: Option<String>,
}
impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;
    fn action(&self) -> Option<&String> {
        self.activation_action.as_ref()
    }
    fn args(&self) -> Vec<&str> {
        self.open_files
            .iter()
            .filter_map(|path| path.to_str())
            .collect()
    }
}
#[derive(Debug, Clone)]
pub enum FilePurpose {
    Install,
    Remote,
    Snapshot,
    Export(Snapshot),
}
#[derive(Debug, Clone)]
enum Dialog {
    Source(Package),
    Updates(Package, Result<backend::UpdateStatus, String>),
    InstallIds(Vec<String>),
    InstallFiles(Vec<PathBuf>),
    Confirm(Operation),
    Remote,
    RemoteDetails(Remote),
    NewSnapshot,
    ImportSnapshot,
    Rename(Snapshot),
    Version(Package),
    About,
}
pub struct App {
    flags: Flags,
    capture_ticks: u8,
    search_id: widget::Id,
    source_choice: usize,
    checking_updates: bool,
    core: cosmic::Core,
    nav: nav_bar::Model,
    page: Page,
    paths: Paths,
    inventory: Inventory,
    loading: bool,
    query: String,
    selected: Option<String>,
    selection: HashSet<String>,
    batch: bool,
    filters: bool,
    apps: bool,
    runtimes: bool,
    eol: bool,
    origin: usize,
    runtime: usize,
    installation: usize,
    sort: usize,
    leftovers: bool,
    detail: String,
    detail_loading: bool,
    history: String,
    dialog: Option<Dialog>,
    notice: Option<String>,
    job: Option<Job>,
    job_title: String,
    uninstall_data: bool,
    remote_name: String,
    remote_url: String,
    show_disabled_remotes: bool,
    remote_menu: Option<(String, String)>,
    install_query: String,
    install_scope: usize,
    install_remote: usize,
    results: Vec<(String, String)>,
    queue: BTreeSet<String>,
    searching: bool,
    snapshot_name: String,
    import_id: String,
    import_path: Option<PathBuf>,
    commit: String,
    hold: bool,
    preferences: Preferences,
    window_state: WindowState,
    window_request: u64,
    window_observed: u64,
    initial_window_mode: Option<window_state::Mode>,
    window_started: std::time::Instant,
    capture_instance: bool,
    ui_instance: String,
    ui_pointer: Option<[f32; 2]>,
    ui_pointer_serial: u64,
}
fn background<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    map: impl FnOnce(T) -> Message + Send + 'static,
) -> Task<Message> {
    cosmic::task::future(async move {
        match tokio::task::spawn_blocking(f).await {
            Ok(value) => map(value),
            Err(e) => Message::Finished(Err(format!("Worker failed: {e}"))),
        }
    })
}

fn button<'a>(label: &str, message: Message) -> Element<'a, Message> {
    widget::button::standard(label.to_string())
        .on_press(message)
        .into()
}
fn package_row_style(selected: bool) -> cosmic::theme::Button {
    use cosmic::widget::button::Catalog;
    let highlighted = move |focused, theme: &cosmic::Theme, alpha| {
        let mut style = theme.active(
            focused,
            selected,
            &cosmic::theme::Button::ListItem([10.; 4]),
        );
        let mut tint = iced::Color::from(theme.cosmic().background(theme.transparent).on);
        tint.a = alpha;
        style.overlay = Some(tint.into());
        style
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| {
            theme.active(
                focused,
                selected,
                &cosmic::theme::Button::ListItem([10.; 4]),
            )
        }),
        disabled: Box::new(|theme| theme.disabled(&cosmic::theme::Button::ListItem([10.; 4]))),
        hovered: Box::new(move |focused, theme| {
            highlighted(focused, theme, if selected { 0.06 } else { 0.10 })
        }),
        pressed: Box::new(move |focused, theme| highlighted(focused, theme, 0.16)),
    }
}
fn ic(name: &str, size: u16) -> widget::Icon {
    icon::from_name(name).size(size).icon()
}
fn package_icon(id: &str, size: u16) -> widget::Icon {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let roots = [
        home.join(".local/share/flatpak/exports/share/icons/hicolor"),
        PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor"),
        PathBuf::from("/usr/share/icons/hicolor"),
    ];
    for root in roots {
        for res in [
            "128x128", "64x64", "256x256", "512x512", "scalable", "48x48",
        ] {
            for ext in ["svg", "png"] {
                let p = root.join(res).join("apps").join(format!("{id}.{ext}"));
                if p.is_file() {
                    return widget::icon(icon::from_path(p)).size(size);
                }
            }
        }
    }
    ic("application-x-executable", size)
}
fn panel<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::container(content)
        .padding(14)
        .class(cosmic::theme::Container::Card)
        .width(Length::Fill)
        .into()
}
fn info_row<'a>(label: &str, value: &str) -> Element<'a, Message> {
    widget::row::with_capacity(2)
        .spacing(12)
        .align_y(Alignment::Center)
        .push(
            widget::column::with_capacity(2)
                .spacing(3)
                .push(widget::text::caption(label.to_string()))
                .push(widget::text(value.to_string()))
                .width(Length::Fill),
        )
        .push(
            widget::button::icon(icon::from_name("edit-copy-symbolic"))
                .on_press(Message::Copy(value.into()))
                .tooltip("Copy"),
        )
        .into()
}
impl App {
    fn refresh(&mut self) -> Task<Message> {
        self.remote_menu = None;
        self.loading = true;
        let p = self.paths.clone();
        background(move || backend::load(&p), Message::Loaded)
    }
    fn origins(&self) -> Vec<String> {
        std::iter::once("All remotes".into())
            .chain(
                self.inventory
                    .remotes
                    .iter()
                    .map(|r| r.name.clone())
                    .collect::<BTreeSet<_>>(),
            )
            .collect()
    }
    fn runtimes(&self) -> Vec<String> {
        std::iter::once("Any runtime".into())
            .chain(
                self.inventory
                    .packages
                    .iter()
                    .map(|p| p.runtime.clone())
                    .filter(|r| !r.is_empty())
                    .collect::<BTreeSet<_>>(),
            )
            .collect()
    }
    fn installations(&self) -> Vec<String> {
        std::iter::once("All installations".into())
            .chain(self.inventory.installations.clone())
            .collect()
    }
    fn visible_packages(&self) -> Vec<&Package> {
        let origins = self.origins();
        let runtimes = self.runtimes();
        let installations = self.installations();
        let q = self.query.to_lowercase();
        let mut list: Vec<_> = self
            .inventory
            .packages
            .iter()
            .filter(|p| {
                ((self.apps && !p.is_runtime()) || (self.runtimes && p.is_runtime()))
                    && (!self.eol || p.eol())
                    && (self.origin == 0 || origins.get(self.origin) == Some(&p.origin))
                    && (self.runtime == 0 || runtimes.get(self.runtime) == Some(&p.runtime))
                    && (self.installation == 0
                        || installations.get(self.installation) == Some(&p.installation))
                    && format!(
                        "{} {} {} {} {}",
                        p.name, p.id, p.branch, p.origin, p.version
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .collect();
        match self.sort {
            1 => list.sort_by_key(|p| p.id.to_lowercase()),
            2 => list.sort_by_key(|p| (p.origin.clone(), p.name.to_lowercase())),
            _ => (),
        };
        list
    }
    fn package(&self) -> Option<&Package> {
        let key = self.selected.as_ref()?;
        self.inventory.packages.iter().find(|p| p.key() == *key)
    }
    fn selected_packages(&self) -> Vec<Package> {
        if self.batch {
            self.inventory
                .packages
                .iter()
                .filter(|p| self.selection.contains(&p.key()))
                .cloned()
                .collect()
        } else {
            self.package().cloned().into_iter().collect()
        }
    }
    fn selected_data(&self) -> Vec<String> {
        if self.batch {
            self.inventory
                .data
                .iter()
                .filter(|d| self.selection.contains(&d.id))
                .map(|d| d.id.clone())
                .collect()
        } else {
            self.selected.clone().into_iter().collect()
        }
    }
    fn selected_ids(&self) -> Vec<String> {
        if self.page == Page::Snapshots {
            return self
                .selected_snapshots()
                .into_iter()
                .map(|s| s.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
        }
        if self.page == Page::Packages {
            self.selected_packages()
                .iter()
                .filter(|p| !p.is_runtime())
                .map(|p| p.id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            self.selected_data()
        }
    }
    fn select_package(&mut self, key: String) -> Task<Message> {
        self.selected = Some(key.clone());
        self.detail.clear();
        self.history.clear();
        if let Some(p) = self.package().cloned() {
            self.detail_loading = true;
            return background(
                move || (key, backend::details(&p)),
                |(k, r)| Message::Details(k, r),
            );
        }
        Task::none()
    }
    fn switch(&mut self, page: Page) -> Task<Message> {
        self.remote_menu = None;
        self.page = page;
        self.query.clear();
        self.selected = None;
        self.selection.clear();
        self.batch = false;
        let id = self
            .nav
            .iter()
            .find(|id| self.nav.data::<Page>(*id) == Some(&page));
        if let Some(id) = id {
            self.nav.activate(id);
        }
        self.set_header_title(page.name().into());
        if page == Page::Packages
            && let Some(p) = self.visible_packages().first()
        {
            return self.select_package(p.key());
        }
        Task::none()
    }
    fn set_notice(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.notice = Some(e);
        }
    }
    fn run(&mut self, op: Operation) -> Task<Message> {
        if self.job.is_some() {
            return Task::none();
        }
        self.job_title = op.title().into();
        let job = Job::default();
        let work = job.clone();
        self.job = Some(job);
        let paths = self.paths.clone();
        background(
            move || {
                backend::execute(op, &paths, &work).and_then(|r| {
                    let body = format!(
                        "{}\n{}{}\nReport: {}",
                        r.title,
                        r.steps.join("\n"),
                        if r.errors.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "\nErrors:\n{}\n{}",
                                r.errors.join("\n"),
                                r.output
                                    .iter()
                                    .rev()
                                    .take(12)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .rev()
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            )
                        },
                        r.log.display()
                    );
                    if r.errors.is_empty() && !r.cancelled {
                        Ok(body)
                    } else {
                        Err(body)
                    }
                })
            },
            Message::Finished,
        )
    }
    fn copy_selection(&self) -> String {
        if self.page == Page::Snapshots {
            return self.selected_ids().join("\n");
        }
        if self.page == Page::Packages {
            self.selected_packages()
                .iter()
                .map(|p| p.id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            self.selected_data().join("\n")
        }
    }
    fn visible_data(&self) -> Vec<&DataFolder> {
        self.inventory
            .data
            .iter()
            .filter(|d| {
                (d.installed != self.leftovers)
                    && d.id.to_lowercase().contains(&self.query.to_lowercase())
            })
            .collect()
    }
    fn filtered_snapshots(&self) -> Vec<&Snapshot> {
        self.inventory
            .snapshots
            .iter()
            .filter(|s| {
                format!("{} {}", s.id, s.name)
                    .to_lowercase()
                    .contains(&self.query.to_lowercase())
            })
            .collect()
    }
    fn selected_snapshots(&self) -> Vec<Snapshot> {
        self.inventory
            .snapshots
            .iter()
            .filter(|s| self.selection.contains(&s.path.display().to_string()))
            .cloned()
            .collect()
    }
    fn filters_view(&self) -> Element<'_, Message> {
        let origins = self.origins();
        let runtimes = self.runtimes();
        let installations = self.installations();
        panel(
            widget::column::with_capacity(4)
                .spacing(12)
                .push(
                    widget::row::with_capacity(3)
                        .spacing(16)
                        .push(
                            widget::checkbox(self.apps)
                                .label("Applications")
                                .on_toggle(Message::Applications),
                        )
                        .push(
                            widget::checkbox(self.runtimes)
                                .label("Runtimes")
                                .on_toggle(Message::Runtimes),
                        )
                        .push(
                            widget::checkbox(self.eol)
                                .label("End of life only")
                                .on_toggle(Message::Eol),
                        ),
                )
                .push(
                    widget::row::with_capacity(2)
                        .spacing(8)
                        .push(widget::dropdown(
                            origins,
                            Some(self.origin),
                            Message::Origin,
                        ))
                        .push(widget::dropdown(
                            installations,
                            Some(self.installation),
                            Message::Installation,
                        )),
                )
                .push(widget::dropdown(
                    runtimes,
                    Some(self.runtime),
                    Message::Runtime,
                ))
                .push(widget::dropdown(
                    vec!["Name", "Application ID", "Remote"],
                    Some(self.sort),
                    Message::Sort,
                )),
        )
    }
    fn packages_view(&self) -> Element<'_, Message> {
        let packages = self.visible_packages();
        let mut list = widget::column::with_capacity(packages.len() + 2).spacing(4);
        for p in &packages {
            let key = p.key();
            let mut row = widget::row::with_capacity(3)
                .spacing(12)
                .align_y(Alignment::Center);
            if self.batch {
                let k = key.clone();
                row = row.push(
                    widget::checkbox(self.selection.contains(&key))
                        .label("")
                        .on_toggle(move |v| Message::Toggle(k.clone(), v)),
                );
            }
            let status = format!(
                "{}{}{}",
                if p.masked { " · Updates blocked" } else { "" },
                if p.pinned { " · Pinned" } else { "" },
                if p.eol() { " · End of life" } else { "" }
            );
            row = row.push(package_icon(&p.id, 36)).push(
                widget::column::with_capacity(2)
                    .spacing(3)
                    .push(widget::text::body(p.name.clone()))
                    .push(widget::text::caption(format!(
                        "{}\n{} · {}{status}",
                        p.id, p.branch, p.installation
                    )))
                    .width(Length::Fill),
            );
            let selected = if self.batch {
                self.selection.contains(&key)
            } else {
                self.selected.as_ref() == Some(&key)
            };
            let b = widget::button::custom(row)
                .id(widget::Id::new(format!("package:{key}")))
                .padding(10)
                .width(Length::Fill)
                .class(package_row_style(selected))
                .selected(selected)
                .on_press(if self.batch {
                    Message::Toggle(key.clone(), !self.selection.contains(&key))
                } else {
                    Message::Select(key)
                });
            list = list.push(b);
        }
        if packages.is_empty() {
            list = list.push(widget::text("No matching packages"));
        }
        let mut left = widget::column::with_capacity(4)
            .spacing(12)
            .push(widget::text::caption(format!(
                "{} packages",
                packages.len()
            )));
        if self.filters {
            left = left.push(self.filters_view());
        }
        left = left.push(widget::scrollable(list).spacing(8).height(Length::Fill));
        let detail = if self.batch {
            self.batch_view()
        } else if let Some(p) = self.package() {
            self.package_detail(p)
        } else {
            self.empty(
                "Select a package",
                "View its information and manage its data.",
                "package-x-generic-symbolic",
            )
        };
        widget::row::with_capacity(3)
            .spacing(20)
            .push(widget::container(left).width(340).height(Length::Fill))
            .push(widget::divider::vertical::default())
            .push(
                widget::container(detail)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .into()
    }
    fn package_detail<'a>(&'a self, p: &'a Package) -> Element<'a, Message> {
        let mut actions = widget::row::with_capacity(2)
            .spacing(8)
            .align_y(Alignment::Center);
        if !p.is_runtime() {
            actions = actions
                .push(widget::button::suggested("Open").on_press(Message::Launch(p.clone())));
        }
        actions = actions.push(
            widget::button::destructive("Uninstall").on_press(Message::Ask(Operation::Uninstall(
                vec![p.clone()],
                self.uninstall_data,
            ))),
        );
        let mut col = widget::column::with_capacity(16)
            .spacing(20)
            .align_x(Alignment::Center)
            .push(package_icon(&p.id, 96))
            .push(widget::text::title2(&p.name))
            .push(widget::text::caption(&p.id))
            .push(actions);
        col = col.push(
            widget::checkbox(self.uninstall_data)
                .label("Also move app data to Trash when uninstalling")
                .on_toggle(Message::UninstallData),
        );
        let mut settings = widget::column::with_capacity(6).spacing(14);
        if !p.is_runtime() {
            let data = self.inventory.data.iter().find(|d| d.id == p.id);
            let mut row = widget::row::with_capacity(4)
                .spacing(8)
                .align_y(Alignment::Center)
                .push(
                    widget::column::with_capacity(2)
                        .push(widget::text::caption("User Data"))
                        .push(widget::text(
                            data.map(|d| backend::size(d.bytes))
                                .unwrap_or_else(|| "No user data".into()),
                        ))
                        .width(Length::Fill),
                );
            if data.is_some() {
                row = row
                    .push(
                        widget::button::icon(icon::from_name("folder-open-symbolic"))
                            .on_press(Message::Open(
                                self.paths.app_data(&p.id).unwrap_or_default(),
                            ))
                            .tooltip("Open user data"),
                    )
                    .push(
                        widget::button::icon(icon::from_name("edit-delete-symbolic"))
                            .on_press(Message::Ask(Operation::TrashData(vec![p.id.clone()])))
                            .tooltip("Move data to Trash"),
                    );
            }
            settings = settings
                .push(row)
                .push(widget::divider::horizontal::default());
        }
        settings = settings
            .push(info_row(
                "Version",
                if p.version.is_empty() {
                    "Not provided"
                } else {
                    &p.version
                },
            ))
            .push(
                widget::flex_row(vec![
                    button("Change Version", Message::History),
                    button("Check for Updates", Message::CheckUpdates(p.clone())),
                    button("Change Source", Message::ChooseSource(p.clone())),
                ])
                .spacing(8),
            );
        if !p.runtime.is_empty() {
            settings = settings.push(info_row("Runtime", &p.runtime));
        }
        let p2 = p.clone();
        settings = settings.push(
            widget::checkbox(p.masked)
                .label("Block automatic updates")
                .on_toggle(move |on| Message::Ask(Operation::Mask(p2.clone(), on))),
        );
        if p.is_runtime() {
            let p2 = p.clone();
            settings = settings.push(
                widget::checkbox(p.pinned)
                    .label("Pin runtime")
                    .on_toggle(move |on| Message::Ask(Operation::Pin(p2.clone(), on))),
            );
        }
        col = col.push(panel(settings));
        let mut information = widget::column::with_capacity(14)
            .spacing(14)
            .push(widget::text::heading("Package Information"));
        for (label, value) in [
            ("Application ID", p.id.clone()),
            ("Ref", p.full_ref()),
            ("Architecture", p.arch.clone()),
            ("Branch", p.branch.clone()),
            ("Origin", p.origin.clone()),
            ("Installation", p.installation.clone()),
            ("Installed Size", p.size.clone()),
            ("Options", p.options.clone()),
        ] {
            information = information.push(info_row(label, &value));
        }
        if self.detail_loading {
            information = information.push(widget::text("Loading package details…"));
        }
        for line in self.detail.lines() {
            if let Some((k, v)) = line.trim().split_once(':')
                && !v.trim().is_empty()
            {
                information = information.push(info_row(k, v.trim()));
            }
        }
        col = col.push(panel(information));
        if !p.is_runtime() {
            col = col.push(
                widget::row::with_capacity(2)
                    .spacing(8)
                    .push(button("New Snapshot", Message::NewSnapshot))
                    .push(button("View Snapshots", Message::Page(Page::Snapshots))),
            );
        }
        col = col.push(button(
            "Copy launch command",
            Message::Copy(format!(
                "flatpak run {} {}",
                backend::scope(&p.installation),
                p.full_ref()
            )),
        ));
        widget::scrollable(widget::container(col).padding([12, 6]))
            .spacing(8)
            .height(Length::Fill)
            .into()
    }
    fn batch_view(&self) -> Element<'_, Message> {
        if self.page == Page::Snapshots {
            return panel(
                widget::column::with_capacity(5)
                    .spacing(12)
                    .push(widget::text::heading(format!(
                        "{} selected",
                        self.selection.len()
                    )))
                    .push(
                        widget::row::with_capacity(5)
                            .spacing(8)
                            .push(button("Select All", Message::SelectAll))
                            .push(button(
                                "Copy IDs",
                                Message::Copy(self.selected_ids().join("\n")),
                            ))
                            .push(button("Restore", Message::BatchRestore))
                            .push(button("Install Apps", Message::BatchInstall))
                            .push(
                                widget::button::destructive("Trash")
                                    .on_press(Message::BatchSnapshotTrash),
                            ),
                    ),
            );
        }
        let count = self.selection.len();
        let mut col = widget::column::with_capacity(8)
            .spacing(16)
            .push(widget::text::title2(format!("{count} selected")))
            .push(button("Select All Visible", Message::SelectAll))
            .push(button("Copy App IDs", Message::Copy(self.copy_selection())));
        if count > 0 {
            if self.page == Page::Packages {
                col = col
                    .push(button("Update Selected", Message::BatchUpdate))
                    .push(
                        widget::checkbox(self.uninstall_data)
                            .label("Also move user data to Trash")
                            .on_toggle(Message::UninstallData),
                    )
                    .push(
                        widget::button::destructive("Uninstall Selected")
                            .on_press(Message::BatchUninstall),
                    );
            }
            col = col
                .push(button("Create Snapshots", Message::BatchSnapshot))
                .push(
                    widget::button::destructive("Trash Selected Data")
                        .on_press(Message::BatchTrash),
                );
        }
        panel(col)
    }
    fn empty<'a>(&self, title: &str, description: &str, name: &str) -> Element<'a, Message> {
        widget::container(
            widget::column::with_capacity(3)
                .spacing(16)
                .align_x(Alignment::Center)
                .push(ic(name, 64))
                .push(widget::text::title2(title.to_string()))
                .push(widget::text(description.to_string())),
        )
        .center(Length::Fill)
        .into()
    }
    fn remotes_view(&self) -> Element<'_, Message> {
        let disabled = self
            .inventory
            .remotes
            .iter()
            .filter(|r| r.disabled())
            .count();
        let mut current_header = widget::row::with_capacity(2)
            .align_y(Alignment::Center)
            .spacing(12)
            .push(
                widget::column::with_capacity(2)
                    .spacing(4)
                    .push(widget::text::heading("Current remotes"))
                    .push(widget::text::caption("Remotes available on your system"))
                    .width(Length::Fill),
            );
        if disabled > 0 {
            current_header = current_header.push(
                widget::button::text(format!(
                    "{} disabled ({disabled})",
                    if self.show_disabled_remotes {
                        "Hide"
                    } else {
                        "Show"
                    }
                ))
                .leading_icon(icon::from_name(if self.show_disabled_remotes {
                    "view-reveal-symbolic"
                } else {
                    "view-conceal-symbolic"
                }))
                .on_press(Message::ShowDisabledRemotes(!self.show_disabled_remotes)),
            );
        }
        let mut current = widget::list_column().list_item_padding([8, 12]);
        let mut count = 0;
        for r in self
            .inventory
            .remotes
            .iter()
            .filter(|r| remotes::visible(r, &self.query, self.show_disabled_remotes))
        {
            count += 1;
            let key = (r.installation.clone(), r.name.clone());
            let menu_button = widget::button::icon(icon::from_name("view-more-symbolic"))
                .id(widget::Id::new(format!(
                    "remote:{}:{}:menu",
                    r.installation, r.name
                )))
                .name(format!(
                    "Actions for {} ({})",
                    remotes::title(r),
                    r.installation
                ))
                .on_press(Message::RemoteMenu(
                    if self.remote_menu.as_ref() == Some(&key) {
                        None
                    } else {
                        Some(key.clone())
                    },
                ));
            let mut menu = widget::popover(menu_button)
                .position(widget::popover::Position::Bottom)
                .on_close(Message::RemoteMenu(None))
                .modal(true);
            if self.remote_menu.as_ref() == Some(&key) {
                let actions = widget::column::with_capacity(6)
                    .spacing(2)
                    .push(
                        widget::button::text("Details")
                            .width(Length::Fill)
                            .on_press(Message::RemoteDetails(r.clone())),
                    )
                    .push(
                        widget::button::text("Copy name")
                            .width(Length::Fill)
                            .on_press(Message::Copy(r.name.clone())),
                    )
                    .push(
                        widget::button::text("Copy URL")
                            .width(Length::Fill)
                            .on_press_maybe(
                                (!r.url.is_empty()).then(|| Message::Copy(r.url.clone())),
                            ),
                    )
                    .push(widget::divider::horizontal::default())
                    .push(
                        widget::button::text(if r.disabled() { "Enable" } else { "Disable" })
                            .width(Length::Fill)
                            .on_press(Message::Ask(Operation::RemoteEnabled(
                                r.clone(),
                                r.disabled(),
                            ))),
                    )
                    .push(
                        widget::button::text("Remove remote…")
                            .width(Length::Fill)
                            .on_press(Message::Ask(Operation::RemoteRemove(r.clone()))),
                    );
                menu = menu.popup(
                    widget::container(actions)
                        .width(220)
                        .padding(6)
                        .class(cosmic::theme::Container::Dropdown),
                );
            }
            current = current.add(
                widget::row::with_capacity(4)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .push(
                        widget::column::with_capacity(2)
                            .spacing(2)
                            .push(
                                widget::text::body(remotes::title(r))
                                    .width(Length::Fill)
                                    .wrapping(iced::advanced::text::Wrapping::None)
                                    .ellipsize(iced::advanced::text::Ellipsize::End(
                                        iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                                    )),
                            )
                            .push(widget::text::caption(format!(
                                "Installation: {}{}",
                                r.installation,
                                if r.disabled() { " · Disabled" } else { "" }
                            )))
                            .width(Length::Fill),
                    )
                    .push(
                        widget::text::caption(&r.name)
                            .width(130)
                            .wrapping(iced::advanced::text::Wrapping::None)
                            .ellipsize(iced::advanced::text::Ellipsize::End(
                                iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                            )),
                    )
                    .push(
                        widget::tooltip(
                            widget::button::icon(icon::from_name("view-filter-symbolic"))
                                .name(format!(
                                    "Show packages from {} ({})",
                                    r.name, r.installation
                                ))
                                .on_press(Message::BrowseRemote(r.clone())),
                            widget::text("Show installed packages from this remote"),
                            widget::tooltip::Position::Bottom,
                        )
                        .gap(8),
                    )
                    .push(menu),
            );
        }
        if count == 0 {
            current = current.add(widget::text(if self.inventory.remotes.is_empty() {
                "No configured remotes. Add one below to get started."
            } else if self.query.is_empty() {
                "No enabled remotes. Show disabled remotes to manage them."
            } else {
                "No matching remotes. Try another search or show disabled remotes."
            }));
        }
        let mut popular = widget::list_column().list_item_padding([8, 12]);
        for (index, preset) in remotes::POPULAR.iter().enumerate() {
            popular = popular.add(
                widget::list::button(
                    widget::settings::item::builder(&preset.title)
                        .description(&preset.description)
                        .control(ic("list-add-symbolic", 20)),
                )
                .on_press(Message::PopularRemote(index)),
            );
        }
        let other = widget::list_column()
            .list_item_padding([8, 12])
            .add(
                widget::list::button(
                    widget::settings::item::builder("Add a repo file")
                        .description("Choose a downloaded .flatpakrepo file")
                        .control(ic("list-add-symbolic", 20)),
                )
                .on_press(Message::AddRemoteFile),
            )
            .add(
                widget::list::button(
                    widget::settings::item::builder("Add a custom remote")
                        .description("Enter a repository URL and name")
                        .control(ic("list-add-symbolic", 20)),
                )
                .on_press(Message::AddRemote),
            );
        let content = widget::column::with_capacity(4)
            .spacing(28)
            .push(
                widget::search_input("Search remotes", &self.query)
                    .id(self.search_id.clone())
                    .on_input(Message::Search),
            )
            .push(
                widget::column::with_capacity(2)
                    .spacing(12)
                    .push(current_header)
                    .push(current),
            )
            .push(
                widget::column::with_capacity(2)
                    .spacing(12)
                    .push(
                        widget::column::with_capacity(2)
                            .spacing(4)
                            .push(widget::text::heading("Add popular remotes"))
                            .push(widget::text::caption("Add remotes to get more software")),
                    )
                    .push(popular),
            )
            .push(
                widget::column::with_capacity(2)
                    .spacing(12)
                    .push(widget::text::heading("Add other remotes"))
                    .push(other),
            );
        widget::scrollable(
            widget::container(
                widget::container(content)
                    .max_width(760)
                    .width(Length::Fill),
            )
            .center_x(Length::Fill)
            .padding([12, 16]),
        )
        .spacing(8)
        .height(Length::Fill)
        .into()
    }
    fn prepare_remote_add(&mut self) {
        self.remote_menu = None;
        self.remote_name.clear();
        self.remote_url.clear();
        self.install_scope = self
            .inventory
            .installations
            .iter()
            .position(|name| name == "user")
            .unwrap_or(0);
    }
    fn review_install_files(&mut self, files: Vec<PathBuf>) {
        let mut selection = if let Some(Dialog::InstallFiles(existing)) = &self.dialog {
            existing.clone()
        } else {
            self.install_scope = self
                .inventory
                .installations
                .iter()
                .position(|name| name == "user")
                .unwrap_or(0);
            Vec::new()
        };
        for file in files {
            if !selection.contains(&file) {
                selection.push(file);
            }
        }
        self.remote_menu = None;
        self.set_show_context(false);
        let _ = self.switch(Page::Install);
        self.dialog = Some(Dialog::InstallFiles(selection));
    }
    fn save_window(&self) {
        if !self.capture_instance
            && let Err(error) = window_state::save(&self.window_state)
        {
            eprintln!("Cannot save window state: {error}");
        }
    }
    fn present_window(&self) -> Task<Message> {
        let Some(window) = self.core.main_window_id() else {
            return Task::none();
        };
        let native = iced::platform_specific::shell::commands::activation::request_token(
            Some(Self::APP_ID.into()),
            Some(window),
        )
        .then(move |token| match token {
            Some(token) => {
                iced::platform_specific::shell::commands::activation::activate(window, token)
            }
            None => iced::window::request_user_attention(
                window,
                Some(iced::window::UserAttention::Informational),
            ),
        });
        if !std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("KDE"))
        {
            return native;
        }
        let instance = self.ui_instance.clone();
        let kde = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let connection = warehouse_cosmic::ui_client::connection()?;
                    warehouse_cosmic::ui_pointer::present(&connection, &instance)
                })
                .await
            },
            |result| {
                match result {
                    Ok(Ok(())) => (),
                    Ok(Err(error)) => eprintln!("Cannot present the KDE window: {error}"),
                    Err(error) => eprintln!("Window activation task failed: {error}"),
                }
                cosmic::Action::None
            },
        );
        Task::batch([native, kde])
    }
    fn data_view(&self) -> Element<'_, Message> {
        let data = self.visible_data();
        let total: u64 = data.iter().map(|d| d.bytes).sum();
        let filter = |label: &'static str, leftovers: bool, id: &'static str| {
            let selected = self.leftovers == leftovers;
            let control = if selected {
                widget::button::suggested(label)
                    .leading_icon(icon::from_name("object-select-symbolic"))
            } else {
                widget::button::standard(label)
            };
            control
                .width(160)
                .id(widget::Id::new(id))
                .name(if selected {
                    format!("{label} (selected)")
                } else {
                    label.into()
                })
                .on_press(Message::Leftovers(leftovers))
        };
        let mut col = widget::column::with_capacity(4).spacing(16).push(
            widget::row::with_capacity(4)
                .spacing(8)
                .align_y(Alignment::Center)
                .push(filter("Active Data", false, "data.active"))
                .push(filter("Leftover Data", true, "data.leftover"))
                .push(widget::Space::new().width(Length::Fill))
                .push(widget::text(format!(
                    "{} folders · {}",
                    data.len(),
                    backend::size(total)
                ))),
        );
        if self.batch {
            col = col.push(self.batch_view());
        }
        if data.is_empty() {
            col = col.push(self.empty(
                if self.leftovers {
                    "No leftover data"
                } else {
                    "No active app data"
                },
                "App data folders will appear here.",
                "folder-symbolic",
            ));
        } else {
            col = col.push(widget::responsive(move |size| self.data_grid(size.width)));
        }
        col.into()
    }
    fn data_grid(&self, width: f32) -> Element<'_, Message> {
        let width = (width - 20.).max(1.);
        let columns = ((width + 12.) / 290.).floor().max(1.);
        let card_width = ((width - (columns - 1.) * 12.) / columns).floor();
        let mut cards = Vec::new();
        for d in self.visible_data() {
            let name = self
                .inventory
                .packages
                .iter()
                .find(|p| p.id == d.id && !p.is_runtime())
                .map(|p| p.name.clone())
                .unwrap_or_else(|| d.id.rsplit('.').next().unwrap_or(&d.id).to_string());
            let mut top = widget::row::with_capacity(3)
                .spacing(12)
                .align_y(Alignment::Center)
                .push(package_icon(&d.id, 40))
                .push(
                    widget::column::with_capacity(2)
                        .spacing(3)
                        .push(
                            widget::text::heading(name)
                                .width(Length::Fill)
                                .wrapping(iced::advanced::text::Wrapping::None)
                                .ellipsize(iced::advanced::text::Ellipsize::End(
                                    iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                                )),
                        )
                        .push(widget::tooltip(
                            widget::text::caption(d.id.clone())
                                .width(Length::Fill)
                                .wrapping(iced::advanced::text::Wrapping::None)
                                .ellipsize(iced::advanced::text::Ellipsize::End(
                                    iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                                )),
                            widget::text::caption(d.id.clone())
                                .width(450)
                                .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                            widget::tooltip::Position::Top,
                        ))
                        .width(Length::Fill),
                );
            if self.batch {
                let id = d.id.clone();
                top = top.push(
                    widget::checkbox(self.selection.contains(&id))
                        .on_toggle(move |on| Message::Toggle(id.clone(), on)),
                );
            }
            let mut actions = widget::row::with_capacity(5)
                .spacing(6)
                .align_y(Alignment::Center)
                .push(widget::text::caption(backend::size(d.bytes)).width(Length::Fill))
                .push(
                    widget::button::icon(icon::from_name("edit-copy-symbolic"))
                        .on_press(Message::Copy(d.id.clone()))
                        .tooltip("Copy ID"),
                )
                .push(
                    widget::button::icon(icon::from_name("folder-open-symbolic"))
                        .on_press(Message::Open(
                            self.paths.app_data(&d.id).unwrap_or_default(),
                        ))
                        .tooltip("Open folder"),
                )
                .push(
                    widget::button::icon(icon::from_name("document-open-recent-symbolic"))
                        .on_press(Message::SnapshotFor(d.id.clone()))
                        .tooltip("New snapshot"),
                )
                .push(
                    widget::button::icon(icon::from_name("edit-delete-symbolic"))
                        .on_press(Message::Ask(Operation::TrashData(vec![d.id.clone()])))
                        .tooltip("Move data to Trash"),
                );
            if !d.installed {
                actions = actions.push(
                    widget::button::icon(icon::from_name("download-symbolic"))
                        .on_press(Message::FindPackage(d.id.clone()))
                        .tooltip("Find matching package"),
                );
            }
            let mut body = widget::column::with_capacity(3)
                .spacing(24)
                .push(top)
                .push(actions);
            if let Some(error) = &d.error {
                body = body.push(widget::text::caption(error));
            }
            cards.push(widget::container(panel(body)).width(card_width).into());
        }
        widget::scrollable(widget::flex_row(cards).spacing(12).width(Length::Fill))
            .spacing(8)
            .height(Length::Fill)
            .into()
    }
    fn snapshots_view(&self) -> Element<'_, Message> {
        let header = widget::row::with_capacity(5)
            .spacing(8)
            .push(widget::text::title2("Snapshots"))
            .push(widget::Space::new().width(Length::Fill))
            .push(button(
                "Open Library",
                Message::Open(self.paths.snapshots()),
            ))
            .push(button("Import", Message::ImportSnapshot))
            .push(widget::button::suggested("New Snapshot").on_press(Message::NewSnapshot));
        let mut col = widget::column::with_capacity(20).spacing(16);
        if self.batch {
            col = col.push(self.batch_view());
        }
        for s in self.filtered_snapshots() {
            let key = s.path.display().to_string();
            let selection: Element<'_, Message> = if self.batch {
                let key2 = key.clone();
                widget::checkbox(self.selection.contains(&key))
                    .label("Select snapshot")
                    .on_toggle(move |v| Message::Toggle(key2.clone(), v))
                    .into()
            } else {
                widget::Space::new().into()
            };
            let mut actions = widget::row::with_capacity(5)
                .spacing(8)
                .push(button(
                    "Restore",
                    Message::Ask(Operation::Restore(s.clone())),
                ))
                .push(button(
                    "Export",
                    Message::ChooseFile(FilePurpose::Export(s.clone())),
                ))
                .push(button("Install App", Message::FindPackage(s.id.clone())));
            if s.official {
                actions = actions.push(button(
                    "Import Copy",
                    Message::Ask(Operation::ImportSnapshot {
                        id: s.id.clone(),
                        path: s.path.clone(),
                        name: s.name.clone(),
                    }),
                ));
            } else {
                actions = actions
                    .push(button("Rename", Message::RenameSnapshot(s.clone())))
                    .push(
                        widget::button::destructive("Trash")
                            .on_press(Message::Ask(Operation::TrashSnapshot(s.clone()))),
                    );
            }
            col = col.push(panel(
                widget::column::with_capacity(4)
                    .spacing(12)
                    .push(selection)
                    .push(
                        widget::row::with_capacity(2)
                            .spacing(12)
                            .push(package_icon(&s.id, 40))
                            .push(
                                widget::column::with_capacity(2)
                                    .push(widget::text::heading(s.name.clone()))
                                    .push(widget::text::caption(format!(
                                        "{} · {}",
                                        s.id,
                                        snapshots::date(s.timestamp)
                                    ))),
                            ),
                    )
                    .push(widget::text::caption(format!(
                        "{} · {}{}",
                        backend::size(s.bytes),
                        s.version,
                        if s.official {
                            " · Official Warehouse library (read-only)"
                        } else {
                            ""
                        }
                    )))
                    .push(actions),
            ));
        }
        if self.inventory.snapshots.is_empty() {
            col = col.push(self.empty(
                "No snapshots yet",
                "Create a backup of an app's data before making changes.",
                "document-open-recent-symbolic",
            ));
        }
        widget::column::with_capacity(2)
            .spacing(16)
            .push(header)
            .push(widget::scrollable(col).spacing(8).height(Length::Fill))
            .into()
    }
    fn install_remotes(&self) -> Vec<&Remote> {
        let installation = self.inventory.installations.get(self.install_scope);
        self.inventory
            .remotes
            .iter()
            .filter(|r| Some(&r.installation) == installation && !r.disabled())
            .collect()
    }
    fn install_view(&self) -> Element<'_, Message> {
        let remotes = self.install_remotes();
        let remote_names: Vec<_> = remotes
            .iter()
            .map(|r| format!("{} ({})", r.title, r.name))
            .collect();
        let mut col = widget::column::with_capacity(8)
            .spacing(16)
            .push(widget::text::title2("Install Packages"))
            .push(
                widget::row::with_capacity(3)
                    .spacing(8)
                    .push(widget::dropdown(
                        self.inventory.installations.clone(),
                        Some(self.install_scope),
                        Message::InstallScope,
                    ))
                    .push(widget::dropdown(
                        remote_names,
                        Some(self.install_remote),
                        Message::InstallRemote,
                    ))
                    .push(button(
                        "Open Package File",
                        Message::ChooseFile(FilePurpose::Install),
                    )),
            )
            .push(
                widget::row::with_capacity(2)
                    .spacing(8)
                    .push(
                        widget::text_input("Search remote packages", &self.install_query)
                            .on_input(|s| Message::Field(Field::InstallQuery, s))
                            .on_submit(|_| Message::InstallSearch),
                    )
                    .push(button(
                        if self.searching {
                            "Searching…"
                        } else {
                            "Search"
                        },
                        Message::InstallSearch,
                    )),
            );
        if !self.queue.is_empty() {
            col = col.push(panel(
                widget::row::with_capacity(2)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(
                        widget::text(format!("{} pending packages", self.queue.len()))
                            .width(Length::Fill),
                    )
                    .push(
                        widget::button::suggested("Review and Install")
                            .on_press(Message::InstallQueue),
                    ),
            ));
        }
        let mut results = widget::column::with_capacity(self.results.len() + 1).spacing(4);
        for (name, reference) in &self.results {
            let reference2 = reference.clone();
            results = results.push(panel(
                widget::row::with_capacity(2)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(
                        widget::column::with_capacity(2)
                            .push(widget::text(name.clone()))
                            .push(widget::text::caption(reference.clone()))
                            .width(Length::Fill),
                    )
                    .push(
                        widget::checkbox(self.queue.contains(reference))
                            .label("Add")
                            .on_toggle(move |v| Message::Queue(reference2.clone(), v)),
                    ),
            ));
        }
        if self.results.is_empty() && !self.searching {
            results = results.push(widget::text(
                "Search a configured remote, or open a .flatpak or .flatpakref file.",
            ));
        }
        col = col.push(widget::scrollable(results).spacing(8).height(Length::Fill));
        col.into()
    }
}
impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = "dev.nilltadios.WarehouseCosmic";
    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }
    fn init(core: cosmic::Core, flags: Flags) -> (Self, Task<Message>) {
        let mut nav = nav_bar::Model::default();
        for (page, name) in [
            (Page::Packages, "package-x-generic-symbolic"),
            (Page::Remotes, "network-server-symbolic"),
            (Page::Data, "folder-symbolic"),
            (Page::Snapshots, "document-open-recent-symbolic"),
            (Page::Install, "folder-download-symbolic"),
        ] {
            nav.insert()
                .text(page.name())
                .icon(icon::from_name(name))
                .data(page);
        }
        nav.activate_position(0);
        let paths = Paths::discover().expect("A home directory is required");
        let (preferences, preferences_error) = match preferences::load() {
            Ok(settings) => (settings, None),
            Err(error) => (Preferences::default(), Some(error)),
        };
        let state = flags.window_state;
        let capture_instance = flags.capture.is_some();
        let mut app = Self {
            flags,
            capture_ticks: 0,
            search_id: widget::Id::unique(),
            source_choice: 0,
            checking_updates: false,
            core,
            nav,
            page: Page::Packages,
            paths,
            inventory: Inventory::default(),
            loading: true,
            query: String::new(),
            selected: None,
            selection: HashSet::new(),
            batch: false,
            filters: false,
            apps: true,
            runtimes: false,
            eol: false,
            origin: 0,
            runtime: 0,
            installation: 0,
            sort: 0,
            leftovers: false,
            detail: String::new(),
            detail_loading: false,
            history: String::new(),
            dialog: None,
            notice: preferences_error,
            job: None,
            job_title: String::new(),
            uninstall_data: false,
            remote_name: String::new(),
            remote_url: String::new(),
            show_disabled_remotes: false,
            remote_menu: None,
            install_query: String::new(),
            install_scope: 0,
            install_remote: 0,
            results: vec![],
            queue: BTreeSet::new(),
            searching: false,
            snapshot_name: String::new(),
            import_id: String::new(),
            import_path: None,
            commit: String::new(),
            hold: true,
            preferences,
            window_state: state,
            window_request: 0,
            window_observed: 0,
            initial_window_mode: Some(state.mode),
            window_started: std::time::Instant::now(),
            capture_instance,
            ui_instance: std::env::var("WAREHOUSE_COSMIC_UI_INSTANCE")
                .ok()
                .filter(|name| {
                    name.starts_with(warehouse_cosmic::ui_protocol::PREFIX)
                        && zbus::names::WellKnownName::try_from(name.as_str()).is_ok()
                })
                .unwrap_or_else(warehouse_cosmic::ui_client::new_instance),
            ui_pointer: None,
            ui_pointer_serial: 0,
        };
        app.set_header_title("Packages".into());
        let title = app.set_window_title(
            "Warehouse COSMIC".into(),
            app.core.main_window_id().unwrap(),
        );
        let load = app.refresh();
        let window = app.core.main_window_id().unwrap();
        let restore: iced::Task<Message> = window_state::restore(state.mode, window);
        (
            app,
            Task::batch([title, load, restore.map(cosmic::Action::App)]),
        )
    }
    fn nav_bar(&self) -> Option<Element<'_, cosmic::Action<Message>>> {
        if !self.core.nav_bar_active() {
            return None;
        }
        let sidebar = widget::nav_bar(&self.nav, |id| {
            cosmic::Action::Cosmic(cosmic::app::Action::NavBar(id))
        })
        .window_id_maybe(self.core.main_window_id())
        .on_surface_action(|action| cosmic::Action::Surface(action.flatten()))
        .into_container()
        .width(Length::Shrink)
        .height(Length::Fill);
        Some(if self.core.is_condensed() {
            sidebar.into()
        } else {
            sidebar.max_width(280).into()
        })
    }
    fn context_drawer(&self) -> Option<cosmic::app::context_drawer::ContextDrawer<'_, Message>> {
        if !self.core.window.show_context {
            return None;
        }
        let appearance = widget::settings::section().title("Appearance").add(
            widget::settings::item::builder("Theme").control(widget::dropdown(
                vec!["Match desktop", "Dark", "Light"],
                Some(self.preferences.theme.index()),
                Message::Theme,
            )),
        );
        Some(
            cosmic::app::context_drawer::context_drawer(appearance, Message::CloseSettings)
                .title("Settings"),
        )
    }
    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        if self.job.is_some() {
            return Task::none();
        }
        if let Some(page) = self.nav.data::<Page>(id).copied() {
            self.switch(page)
        } else {
            Task::none()
        }
    }
    fn dbus_activation(&mut self, message: cosmic::dbus_activation::Message) -> Task<Message> {
        use cosmic::dbus_activation::Details;
        let files = match message.msg {
            Details::Open { url } => url
                .into_iter()
                .filter_map(|url| url.to_file_path().ok())
                .collect::<Vec<_>>(),
            Details::ActivateAction { action, args } if action == "open-files" => {
                args.into_iter().map(PathBuf::from).collect()
            }
            _ => vec![],
        };
        let mut tasks = Vec::new();
        if !files.is_empty() {
            if self.job.is_some() {
                self.flags.open_files.extend(files);
                self.notice = Some(
                    "Package files are queued for review after the current operation finishes."
                        .into(),
                );
            } else {
                tasks.push(self.update(Message::FilesPicked(Ok(files))));
            }
        }
        tasks.push(self.present_window());
        Task::batch(tasks)
    }
    fn header_start(&self) -> Vec<Element<'_, Message>> {
        vec![
            header_tooltip(
                widget::button::icon(icon::from_svg_bytes(
                    include_bytes!("../resources/dev.nilltadios.WarehouseCosmic.svg").as_slice(),
                ))
                .icon_size(24)
                .padding(4)
                .width(32)
                .height(32)
                .on_press(Message::About)
                .id(widget::Id::new("header.logo"))
                .name("Warehouse COSMIC"),
                "Warehouse COSMIC",
            ),
            header_tooltip(
                widget::button::icon(icon::from_name(if self.core.nav_bar_active() {
                    "navbar-open-symbolic"
                } else {
                    "navbar-closed-symbolic"
                }))
                .icon_size(16)
                .padding(8)
                .width(32)
                .height(32)
                .on_press(Message::ToggleSidebar)
                .id(widget::Id::new("header.sidebar"))
                .name("Toggle sidebar"),
                "Toggle sidebar",
            ),
            header_tooltip(
                widget::button::icon(icon::from_name("view-refresh-symbolic"))
                    .icon_size(16)
                    .padding(8)
                    .width(32)
                    .height(32)
                    .on_press_maybe(
                        (!self.loading && self.job.is_none()).then_some(Message::Refresh),
                    )
                    .id(widget::Id::new("header.refresh"))
                    .name("Refresh"),
                "Refresh",
            ),
        ]
    }
    fn header_end(&self) -> Vec<Element<'_, Message>> {
        vec![
            header_tooltip(
                widget::button::icon(icon::from_name("preferences-system-symbolic"))
                    .icon_size(16)
                    .padding(8)
                    .width(32)
                    .height(32)
                    .name("Settings")
                    .id(widget::Id::new("header.settings"))
                    .on_press(Message::Settings),
                "Settings",
            ),
            header_tooltip(
                widget::button::icon(icon::from_name("help-about-symbolic"))
                    .on_press(Message::About)
                    .id(widget::Id::new("header.about"))
                    .name("About Warehouse COSMIC"),
                "About Warehouse COSMIC",
            ),
        ]
    }
    fn header_center(&self) -> Vec<Element<'_, Message>> {
        vec![widget::text::heading(self.page.name()).into()]
    }
    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            crate::ui_bridge::subscription(&self.ui_instance),
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::UiPointer(position))
                }
                _ => None,
            }),
            iced::event::listen_with(|event, _, id| match event {
                iced::Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(
                    iced::event::wayland::Event::Window(
                        iced::event::wayland::WindowEvent::WindowState(_),
                    ),
                )) => Some(Message::WindowChanged(id)),
                _ => None,
            }),
            iced::window::events().filter_map(|(id, event)| match event {
                iced::window::Event::Opened { .. }
                | iced::window::Event::Resized(_)
                | iced::window::Event::Focused
                | iced::window::Event::Unfocused => Some(Message::WindowChanged(id)),
                _ => None,
            }),
            if self.job.is_some() || self.flags.capture.is_some() {
                iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick)
            } else {
                Subscription::none()
            },
            iced::keyboard::listen().filter_map(|event| match event {
                iced::keyboard::Event::KeyPressed { key, modifiers, .. } => {
                    Some(Message::Key(key, modifiers))
                }
                _ => None,
            }),
        ])
    }
    fn on_escape(&mut self) -> Task<Message> {
        if self.dialog.take().is_some() {
            return Task::none();
        }
        if self.remote_menu.take().is_some() {
            return Task::none();
        }
        if self.core.window.show_context {
            self.set_show_context(false);
            return Task::none();
        }
        if self.batch {
            self.batch = false;
            self.selection.clear();
        } else {
            self.query.clear();
        }
        Task::none()
    }
    fn on_app_exit(&mut self) -> Option<Message> {
        if self.job.is_none() {
            self.save_window();
        }
        self.job.as_ref().map(|_| Message::BusyExit)
    }
    fn on_close_requested(&self, _: iced::window::Id) -> Option<Message> {
        if self.job.is_none() {
            self.save_window();
        }
        self.job.as_ref().map(|_| Message::BusyExit)
    }
    fn view(&self) -> Element<'_, Message> {
        let mut col = widget::column::with_capacity(5).spacing(16);
        if let Some(job) = &self.job {
            let (completed, total, lines) = job
                .progress
                .lock()
                .map(|p| {
                    (
                        p.completed,
                        p.total,
                        p.lines
                            .iter()
                            .rev()
                            .take(9)
                            .cloned()
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                })
                .unwrap_or_default();
            return widget::container(
                widget::column::with_capacity(6)
                    .spacing(20)
                    .push(widget::text::title2(&self.job_title))
                    .push(widget::text(
                        "Working… Completed changes will appear after the operation finishes.",
                    ))
                    .push(widget::progress_bar::determinate_linear(
                        completed as f32 / total.max(1) as f32,
                    ))
                    .push(widget::scrollable(widget::text(lines)).height(Length::Fill))
                    .push(button("Cancel Operation", Message::Cancel)),
            )
            .padding(24)
            .height(Length::Fill)
            .into();
        }
        if let Some(notice) = &self.notice {
            col = col.push(panel(
                widget::row::with_capacity(2)
                    .spacing(12)
                    .width(Length::Fill)
                    .align_y(Alignment::Start)
                    .push(
                        widget::container(
                            widget::scrollable(
                                widget::text(notice.clone())
                                    .width(Length::Fill)
                                    .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                            )
                            .spacing(8)
                            .width(Length::Fill)
                            .height(Length::Shrink),
                        )
                        .width(Length::Fill)
                        .max_height(160),
                    )
                    .push(
                        widget::button::icon(icon::from_name("window-close-symbolic"))
                            .width(32)
                            .height(32)
                            .padding(8)
                            .tooltip("Dismiss report")
                            .on_press(Message::ClearNotice),
                    ),
            ));
        }
        if self.loading {
            return widget::container(col.push(self.empty(
                "Loading Flatpaks…",
                "Reading packages, remotes, user data and snapshots.",
                "view-refresh-symbolic",
            )))
            .padding(20)
            .height(Length::Fill)
            .into();
        }
        if !matches!(self.page, Page::Install | Page::Remotes) {
            let mut tools = widget::row::with_capacity(4)
                .spacing(8)
                .align_y(Alignment::Center)
                .push(
                    widget::search_input("Search", &self.query)
                        .id(self.search_id.clone())
                        .on_input(Message::Search),
                );
            if self.page == Page::Packages {
                tools = tools.push(
                    widget::button::icon(icon::from_name("view-filter-symbolic"))
                        .on_press(Message::Filters)
                        .tooltip("Filters"),
                );
            }
            if matches!(self.page, Page::Packages | Page::Data | Page::Snapshots) {
                tools = tools.push(button(
                    if self.batch { "Done" } else { "Select" },
                    Message::Batch(!self.batch),
                ));
            }
            col = col.push(tools);
        }
        col = col.push(match self.page {
            Page::Packages => self.packages_view(),
            Page::Remotes => self.remotes_view(),
            Page::Data => self.data_view(),
            Page::Snapshots => self.snapshots_view(),
            Page::Install => self.install_view(),
        });
        widget::container(col)
            .id(widget::Id::new("content"))
            .padding(20)
            .height(Length::Fill)
            .into()
    }
    fn dialog(&self) -> Option<Element<'_, Message>> {
        let dialog = self.dialog.as_ref()?;
        let base = widget::dialog().secondary_action(
            widget::button::standard("Cancel")
                .id(widget::Id::new("dialog.cancel"))
                .on_press(Message::CloseDialog),
        );
        Some(match dialog {
            Dialog::Source(p) => {
                let remotes:Vec<_>=self.inventory.remotes.iter().filter(|r|r.installation==p.installation&&!r.disabled()).collect();
                let names:Vec<_>=remotes.iter().map(|r|format!("{} ({})",r.title,r.name)).collect();
                let op=remotes.get(self.source_choice).map(|r|Operation::Reinstall{package:p.clone(),remote:(*r).clone()});
                base.title(format!("Change Source for {}",p.name)).body(format!("Current source: {}\nInstallation: {}\nApp data is kept.",p.origin,p.installation))
                    .control(widget::dropdown(names,Some(self.source_choice),Message::SourceChoice))
                    .primary_action(widget::button::suggested("Review").on_press_maybe(op.map(Message::Ask))).into()
            },
            Dialog::Updates(p,result) => {
                let body=match result {Ok(s)=>format!("{}\nSource: {}\nInstalled commit: {}\nAvailable commit: {}{}",if s.update_available {"An update is available"}else{"Up to date"},s.origin,s.installed_commit,s.available_commit,if s.masked {"\nUpdates are blocked. Unblock this package before updating."}else{""}),Err(e)=>e.clone()};
                base.title(format!("Updates for {}",p.name)).body(body)
                    .primary_action(widget::button::suggested("Update").on_press_maybe(result.as_ref().ok().filter(|s|s.update_available&&!s.masked).map(|_|Message::Ask(Operation::Update(vec![p.clone()]))))).into()
            },
            Dialog::InstallIds(ids) => {
                let remotes=self.install_remotes();
                let names:Vec<_>=remotes.iter().map(|r|r.name.clone()).collect();
                let operation=remotes.get(self.install_remote).map(|r|Operation::Install{installation:r.installation.clone(),remote:r.name.clone(),refs:ids.clone()});
                base.title("Install Apps from Snapshots").body(ids.join("\n"))
                    .control(widget::dropdown(self.inventory.installations.clone(),Some(self.install_scope),Message::InstallScope))
                    .control(widget::dropdown(names,Some(self.install_remote),Message::InstallRemote))
                    .primary_action(widget::button::suggested("Review").on_press_maybe(operation.map(Message::Ask))).into()
            },
            Dialog::Confirm(op)=>base.title(format!("{}?",op.title())).control(widget::container(widget::scrollable(widget::text(op.description())).height(Length::Shrink)).max_height(260)).primary_action(widget::button::suggested("Continue").on_press(Message::Confirm)).into(),
            Dialog::InstallFiles(files) => {
                let mut packages = widget::list_column().list_item_padding([10, 12]);
                for path in files {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    let kind = if path.extension().is_some_and(|ext| ext == "flatpakref") { "Flatpak reference" } else { "Flatpak bundle" };
                    let size = std::fs::metadata(path).map(|m| backend::size(m.len())).unwrap_or_default();
                    packages = packages.add(widget::row::with_capacity(2).spacing(12).align_y(Alignment::Center)
                        .push(ic("package-x-generic-symbolic", 24))
                        .push(widget::column::with_capacity(2).spacing(3).width(Length::Fill)
                            .push(widget::text(filename).width(Length::Fill).wrapping(iced::advanced::text::Wrapping::WordOrGlyph))
                            .push(widget::text::caption(format!("{kind} · {size}")))));
                }
                let mut installations: Vec<_> = self.inventory.installations.iter().enumerate().collect();
                installations.sort_by_key(|(_, name)| match name.as_str() { "user" => 0, "system" => 1, _ => 2 });
                let mut choices = widget::list_column().list_item_padding([8, 12]);
                for (index, name) in installations {
                    let (title, description) = match name.as_str() {
                        "user" => ("User".to_string(), "Available only to you".to_string()),
                        "system" => ("System".to_string(), "Available to everyone on this computer".to_string()),
                        _ => (name.clone(), format!("Use the {name} installation")),
                    };
                    choices = choices.add(widget::settings::item::builder(title).description(description)
                        .radio(index, Some(self.install_scope), Message::InstallScope));
                }
                base.title(if files.len() == 1 { "Install a package" } else { "Install packages" }).width(Length::Fixed(480.))
                    .control(widget::text::heading("Review selection"))
                    .control(widget::container(widget::scrollable(packages).height(Length::Shrink).spacing(8)).max_height(200))
                    .control(widget::text::heading("Choose installation"))
                    .control(choices)
                    .primary_action(widget::button::suggested("Install").id(widget::Id::new("install.confirm")).on_press_maybe((!files.is_empty()).then_some(Message::InstallFilesConfirmed))).into()
            },
            Dialog::Remote=>base.title("Add Remote").body("Enter a name and a repository URL, or choose a .flatpakrepo file.")
                .control(widget::dropdown(self.inventory.installations.clone(),Some(self.install_scope),Message::InstallScope))
                .control(widget::text_input("Remote name",&self.remote_name).on_input(|v|Message::Field(Field::RemoteName,v)))
                .control(widget::text_input("https://… or /path/to/remote.flatpakrepo",&self.remote_url).on_input(|v|Message::Field(Field::RemoteUrl,v)))
                .control(button("Choose File",Message::ChooseFile(FilePurpose::Remote)))
                .primary_action(button("Review",Message::Ask(Operation::RemoteAdd{installation:self.inventory.installations.get(self.install_scope).cloned().unwrap_or_else(||"user".into()),name:self.remote_name.trim().into(),location:self.remote_url.trim().into()}))).into(),
            Dialog::RemoteDetails(r)=>base.title(remotes::title(r).to_string())
                .control(info_row("Name", &r.name))
                .control(info_row("Installation", &r.installation))
                .control(info_row("Status", if r.disabled() { "Disabled" } else { "Enabled" }))
                .control(info_row("URL", if r.url.is_empty() { "Local bundle origin (no repository URL)" } else { &r.url }))
                .secondary_action(button("Close", Message::CloseDialog)).into(),
            Dialog::NewSnapshot=>base.title("New Snapshot").body("Select app data to back up. Close the app yourself before continuing.")
                .control(widget::text_input("Application ID (or comma-separated IDs)",&self.import_id).on_input(|v|Message::Field(Field::ImportId,v)))
                .control(widget::text_input("Snapshot name",&self.snapshot_name).on_input(|v|Message::Field(Field::SnapshotName,v)))
                .primary_action(button("Review",Message::Ask(Operation::Snapshot{ids:self.import_id.split(',').map(str::trim).filter(|s|!s.is_empty()).map(str::to_string).collect(),name:self.snapshot_name.clone()}))).into(),
            Dialog::ImportSnapshot=>base.title("Import Snapshot").body("Import a Warehouse .tar.zst snapshot for this application ID.")
                .control(widget::text_input("Application ID",&self.import_id).on_input(|v|Message::Field(Field::ImportId,v)))
                .control(widget::text_input("Snapshot name",&self.snapshot_name).on_input(|v|Message::Field(Field::SnapshotName,v)))
                .control(button("Choose .tar.zst File",Message::ChooseFile(FilePurpose::Snapshot)))
                .control(widget::text(self.import_path.as_ref().map(|p|p.display().to_string()).unwrap_or_default()))
                .primary_action(widget::button::suggested("Review").on_press_maybe(self.import_path.clone().map(|path|Message::Ask(Operation::ImportSnapshot{id:self.import_id.trim().into(),path,name:self.snapshot_name.clone()})))).into(),
            Dialog::Rename(s)=>base.title("Rename Snapshot").control(widget::text_input("Name",&self.snapshot_name).on_input(|v|Message::Field(Field::SnapshotName,v)))
                .primary_action(button("Review",Message::Ask(Operation::RenameSnapshot(s.clone(),self.snapshot_name.clone())))).into(),
            Dialog::Version(p)=>base.title(format!("Change Version of {}",p.name)).body("Available history depends on the remote. Some publishers retain only the current commit.").control(widget::scrollable(widget::text(self.history.clone())).height(Length::Fixed(250.)))
                .control(widget::text_input("Paste a commit from the history (64 hex characters)",&self.commit).on_input(|v|Message::Field(Field::Commit,v)))
                .control(widget::checkbox(self.hold).label("Block future updates after changing version").on_toggle(Message::Hold))
                .primary_action(button("Review",Message::Ask(Operation::Version(p.clone(),self.commit.trim().into(),self.hold)))).into(),
            Dialog::About=>widget::dialog().title("Warehouse COSMIC").body("Manage Flatpak apps with a native graphical interface and CLI. Inspired by flattool/Warehouse and built with Rust and libcosmic.\n\nGPL-3.0-or-later · Version 0.1.0")
                .control(widget::text::caption("Shortcuts: Ctrl+R refresh · Ctrl+F search · Escape close dialog"))
                .primary_action(button("Close",Message::CloseDialog)).into(),
        })
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UiRequest(request) => return self.ui_request(request),
            Message::UiTree(request, nodes, resolve) => self.ui_tree(request, nodes, resolve),
            Message::UiCaptured(request, path, overwrite, shot) => {
                self.ui_capture(request, path, overwrite, shot)
            }
            Message::UiPointer(point) => {
                self.ui_pointer = Some([point.x, point.y]);
                self.ui_pointer_serial += 1;
            }
            Message::UiCloseNow => return iced::exit(),
            Message::ToggleSidebar => {
                if self.core.is_condensed() {
                    self.core.nav_bar_toggle_condensed();
                } else {
                    self.core.nav_bar_toggle();
                }
            }
            Message::UiActivationToken(pending, token) => {
                pending.respond(warehouse_cosmic::ui_protocol::success(
                    serde_json::json!({"token":token}),
                ));
            }
            Message::WindowChanged(id) => {
                if self.core.main_window_id() != Some(id) {
                    return Task::none();
                }
                self.window_request += 1;
                let request = self.window_request;
                return iced::window::mode(id)
                    .then(move |mode| {
                        iced::window::is_maximized(id).then(move |maximized| {
                            iced::window::size(id).map(move |size| {
                                Message::WindowObserved(request, mode, maximized, size)
                            })
                        })
                    })
                    .map(cosmic::Action::App);
            }
            Message::WindowObserved(request, mode, maximized, size) => {
                if request < self.window_observed || mode == iced::window::Mode::Hidden {
                    return Task::none();
                }
                self.window_observed = request;
                let mode = if mode == iced::window::Mode::Fullscreen {
                    window_state::Mode::Fullscreen
                } else if maximized {
                    window_state::Mode::Maximized
                } else {
                    window_state::Mode::Windowed
                };
                if self
                    .initial_window_mode
                    .is_some_and(|expected| expected != mode)
                    && self.window_started.elapsed() < Duration::from_secs(1)
                {
                    if let (Some(expected), Some(window)) =
                        (self.initial_window_mode, self.core.main_window_id())
                    {
                        return window_state::restore(expected, window).map(cosmic::Action::App);
                    }
                    return Task::none();
                }
                let initial = self.initial_window_mode.take();
                if initial == Some(window_state::Mode::Windowed)
                    && mode == window_state::Mode::Windowed
                    && self.window_state.size() != size
                    && let Some(window) = self.core.main_window_id()
                {
                    return iced::window::resize(window, self.window_state.size())
                        .map(cosmic::Action::App);
                }
                self.initial_window_mode = None;
                self.window_state.observe(mode, size.width, size.height);
            }
            Message::InstallFilesConfirmed => {
                if let Some(Dialog::InstallFiles(paths)) = self.dialog.as_ref() {
                    let operation = Operation::InstallFiles {
                        installation: self
                            .inventory
                            .installations
                            .get(self.install_scope)
                            .cloned()
                            .unwrap_or_else(|| "user".into()),
                        paths: paths.clone(),
                    };
                    match operation.commands() {
                        Ok(_) => {
                            self.dialog = None;
                            return self.run(operation);
                        }
                        Err(error) => self.notice = Some(error),
                    }
                }
            }
            Message::Refresh => {
                if self.job.is_none() {
                    return self.refresh();
                }
            }
            Message::Loaded(result) => {
                self.loading = false;
                match result {
                    Ok(inv) => {
                        if self.inventory.installations.is_empty() {
                            self.install_scope = inv
                                .installations
                                .iter()
                                .position(|i| i == "user")
                                .unwrap_or(0);
                        }
                        self.inventory = inv;
                        self.origin = self.origin.min(self.origins().len().saturating_sub(1));
                        self.runtime = self.runtime.min(self.runtimes().len().saturating_sub(1));
                        self.installation = self
                            .installation
                            .min(self.installations().len().saturating_sub(1));
                        self.selection.clear();
                        if !self.inventory.warnings.is_empty() {
                            self.notice = Some(self.inventory.warnings.join("\n"));
                        }
                        // Previews navigate without executing package or data changes.
                        if !self.flags.open_files.is_empty() {
                            let files = std::mem::take(&mut self.flags.open_files);
                            return self.update(Message::FilesPicked(Ok(files)));
                        }
                        if let Some(preview) = self.flags.preview.take() {
                            let page = match preview.as_str() {
                                "remotes" | "remote-dialog" | "remotes-disabled"
                                | "remotes-menu" | "remotes-details" | "remotes-preset"
                                | "remotes-search" | "remotes-no-results" | "remotes-narrow"
                                | "remote-filter" => Page::Remotes,
                                "data" | "leftovers" => Page::Data,
                                "snapshots" | "new-snapshot" | "import" => Page::Snapshots,
                                "install" => Page::Install,
                                _ => Page::Packages,
                            };
                            self.leftovers = preview == "leftovers";
                            self.filters = preview == "filters";
                            let mut task = self.switch(page);
                            self.show_disabled_remotes = preview == "remotes-disabled";
                            if preview == "remotes-search" {
                                self.query = "flathub".into();
                            }
                            if preview == "remotes-no-results" {
                                self.query = "no-such-remote-fixture".into();
                            }
                            if matches!(
                                preview.as_str(),
                                "remotes-menu" | "remotes-details" | "remote-filter"
                            ) && let Some(remote) = self
                                .inventory
                                .remotes
                                .iter()
                                .find(|r| {
                                    !r.disabled()
                                        && (preview != "remote-filter"
                                            || r.name == "parsec-autobuild")
                                })
                                .cloned()
                            {
                                match preview.as_str() {
                                    "remotes-menu" => {
                                        self.remote_menu = Some((remote.installation, remote.name))
                                    }
                                    "remotes-details" => {
                                        self.dialog = Some(Dialog::RemoteDetails(remote));
                                        return task;
                                    }
                                    _ => return self.update(Message::BrowseRemote(remote)),
                                }
                            }
                            if preview == "remotes-preset" {
                                return self.update(Message::PopularRemote(0));
                            }
                            if matches!(preview.as_str(), "parsec" | "source" | "updates")
                                && let Some(p) = self
                                    .inventory
                                    .packages
                                    .iter()
                                    .find(|p| p.id == "com.parsecgaming.parsec" && !p.is_runtime())
                                    .cloned()
                            {
                                task = self.select_package(p.key());
                                if preview == "source" {
                                    return Task::batch([
                                        task,
                                        self.update(Message::ChooseSource(p)),
                                    ]);
                                }
                                if preview == "updates" {
                                    return Task::batch([
                                        task,
                                        self.update(Message::CheckUpdates(p)),
                                    ]);
                                }
                            }
                            self.batch = preview == "selection";
                            if matches!(
                                preview.as_str(),
                                "appearance" | "settings" | "settings-dark" | "settings-light"
                            ) {
                                self.set_show_context(true);
                                let theme = match preview.as_str() {
                                    "settings-dark" => Some(AppTheme::Dark),
                                    "settings-light" => Some(AppTheme::Light),
                                    _ => None,
                                };
                                if let Some(theme) = theme {
                                    self.preferences.theme = theme;
                                    task = Task::batch([
                                        task,
                                        cosmic::command::set_theme(theme.theme()),
                                    ]);
                                }
                            }
                            self.dialog = match preview.as_str() {
                                "about" => Some(Dialog::About),
                                "remote-dialog" => Some(Dialog::Remote),
                                "new-snapshot" => Some(Dialog::NewSnapshot),
                                "import" => Some(Dialog::ImportSnapshot),
                                _ => None,
                            };
                            return task;
                        }
                        if self.page == Page::Packages {
                            let p = self
                                .selected
                                .as_ref()
                                .and_then(|key| {
                                    self.inventory.packages.iter().find(|p| p.key() == *key)
                                })
                                .or_else(|| self.visible_packages().first().copied())
                                .cloned();
                            if let Some(p) = p {
                                return self.select_package(p.key());
                            }
                        }
                    }
                    Err(e) => self.notice = Some(e),
                }
            }
            Message::Tick => {
                if self.flags.capture.is_some()
                    && !self.loading
                    && !self.detail_loading
                    && !self.searching
                    && !self.checking_updates
                {
                    self.capture_ticks += 1;
                    if self.capture_ticks == 8
                        && let Some(id) = self.core.main_window_id()
                    {
                        return iced::window::screenshot(id)
                            .map(Message::Captured)
                            .map(cosmic::Action::App);
                    }
                }
            }
            Message::Captured(shot) => {
                if let Some(path) = self.flags.capture.take() {
                    match image::save_buffer(
                        &path,
                        &shot.rgba,
                        shot.size.width,
                        shot.size.height,
                        image::ColorType::Rgba8,
                    ) {
                        Ok(()) => println!(
                            "Saved app render to {} ({}x{})",
                            path.display(),
                            shot.size.width,
                            shot.size.height
                        ),
                        Err(e) => {
                            eprintln!("Capture failed: {e}");
                            std::process::exit(1);
                        }
                    }
                    return iced::exit();
                }
            }
            Message::BusyExit => {
                if let Some(job) = &self.job {
                    job.line(
                        "Wait for this operation to finish, or cancel it before closing.".into(),
                    );
                }
            }
            Message::Key(key, modifiers) => {
                use iced::keyboard::{Key, key::Named};
                if key == Key::Named(Named::F5) && self.job.is_none() {
                    return self.refresh();
                }
                if modifiers.control()
                    && self.job.is_none()
                    && let Key::Character(k) = key.as_ref()
                {
                    match k.to_lowercase().as_str() {
                        "r" => return self.refresh(),
                        "," => self.set_show_context(true),
                        "f" => {
                            return widget::text_input::focus(self.search_id.clone())
                                .map(cosmic::Action::App);
                        }
                        "p" => return self.switch(Page::Packages),
                        "m" => return self.switch(Page::Remotes),
                        "d" => return self.switch(Page::Data),
                        "s" => return self.switch(Page::Snapshots),
                        "i" => return self.switch(Page::Install),
                        "b" => {
                            self.batch = !self.batch;
                            self.selection.clear();
                        }
                        "a" if self.batch => return self.update(Message::SelectAll),
                        "t" if self.page == Page::Packages => self.filters = !self.filters,
                        "o" => return pick_file(FilePurpose::Install),
                        _ => (),
                    }
                }
            }
            Message::Search(q) => {
                self.remote_menu = None;
                self.query = q;
            }
            Message::Select(key) => {
                if self.page == Page::Packages {
                    return self.select_package(key);
                }
                self.selected = Some(key);
            }
            Message::Toggle(k, on) => {
                if on {
                    self.selection.insert(k);
                } else {
                    self.selection.remove(&k);
                }
            }
            Message::Batch(on) => {
                self.batch = on;
                self.selection.clear();
            }
            Message::SelectAll => {
                if self.page == Page::Packages {
                    self.selection = self.visible_packages().iter().map(|p| p.key()).collect();
                } else if self.page == Page::Snapshots {
                    self.selection = self
                        .filtered_snapshots()
                        .iter()
                        .map(|s| s.path.display().to_string())
                        .collect();
                } else {
                    self.selection = self.visible_data().iter().map(|d| d.id.clone()).collect();
                }
            }
            Message::Copy(text) => {
                self.remote_menu = None;
                return cosmic::iced::clipboard::write(text).map(cosmic::Action::App);
            }
            Message::Page(page) => return self.switch(page),
            Message::Filters => self.filters = !self.filters,
            Message::Applications(v) => self.apps = v,
            Message::Runtimes(v) => self.runtimes = v,
            Message::Eol(v) => self.eol = v,
            Message::Origin(v) => self.origin = v,
            Message::Runtime(v) => self.runtime = v,
            Message::Installation(v) => self.installation = v,
            Message::Sort(v) => self.sort = v,
            Message::Leftovers(v) => self.leftovers = v,
            Message::Details(k, result) => {
                if self.selected.as_ref() == Some(&k) {
                    self.detail_loading = false;
                    match result {
                        Ok(text) => self.detail = text,
                        Err(e) => self.notice = Some(e),
                    }
                }
            }
            Message::History => {
                if let Some(p) = self.package().cloned() {
                    self.history = "Loading version history…".into();
                    self.commit.clear();
                    self.hold = true;
                    self.dialog = Some(Dialog::Version(p.clone()));
                    let key = p.key();
                    return background(
                        move || backend::history(&p),
                        move |r| Message::HistoryLoaded(key, r),
                    );
                }
            }
            Message::CheckUpdates(p) => {
                self.checking_updates = true;
                self.dialog = Some(Dialog::Updates(
                    p.clone(),
                    Err("Checking for updates…".into()),
                ));
                return background(
                    move || {
                        let result = backend::check_update(&p);
                        (p, result)
                    },
                    |(p, r)| Message::UpdateChecked(p, r),
                );
            }
            Message::UpdateChecked(p, result) => {
                self.checking_updates = false;
                if matches!(&self.dialog,Some(Dialog::Updates(previous,_)) if previous.key()==p.key())
                {
                    self.dialog = Some(Dialog::Updates(p, result));
                }
            }
            Message::ChooseSource(p) => {
                self.source_choice = self
                    .inventory
                    .remotes
                    .iter()
                    .filter(|r| r.installation == p.installation && !r.disabled())
                    .position(|r| r.name == p.origin)
                    .unwrap_or(0);
                self.dialog = Some(Dialog::Source(p));
            }
            Message::SourceChoice(i) => self.source_choice = i,
            Message::BatchUpdate => {
                return self.update(Message::Ask(Operation::Update(self.selected_packages())));
            }
            Message::HistoryLoaded(k, result) => {
                if matches!(&self.dialog,Some(Dialog::Version(p)) if p.key()==k) {
                    self.history = result.unwrap_or_else(|e| e);
                }
            }
            Message::Field(f, text) => match f {
                Field::RemoteName => self.remote_name = text,
                Field::RemoteUrl => self.remote_url = text,
                Field::InstallQuery => self.install_query = text,
                Field::SnapshotName => self.snapshot_name = text,
                Field::ImportId => self.import_id = text,
                Field::Commit => self.commit = text,
            },
            Message::Hold(v) => self.hold = v,
            Message::Ask(op) => {
                self.remote_menu = None;
                if self.job.is_none() {
                    match op.commands() {
                        Ok(_) => self.dialog = Some(Dialog::Confirm(op)),
                        Err(e) => self.notice = Some(e),
                    }
                }
            }
            Message::Confirm => {
                if let Some(Dialog::Confirm(op)) = self.dialog.take() {
                    return self.run(op);
                }
            }
            Message::CloseDialog => self.dialog = None,
            Message::Finished(result) => {
                self.job = None;
                self.dialog = None;
                self.notice = Some(result.unwrap_or_else(|e| e));
                return self.refresh();
            }
            Message::Cancel => {
                if let Some(job) = &self.job {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            }
            Message::ClearNotice => self.notice = None,
            Message::BatchUninstall => {
                let packages = self.selected_packages();
                if !packages.is_empty() {
                    self.dialog = Some(Dialog::Confirm(Operation::Uninstall(
                        packages,
                        self.uninstall_data,
                    )));
                }
            }
            Message::BatchTrash => {
                let ids = self.selected_ids();
                if !ids.is_empty() {
                    self.dialog = Some(Dialog::Confirm(Operation::TrashData(ids)));
                }
            }
            Message::BatchSnapshot | Message::NewSnapshot => {
                let ids = self.selected_ids();
                self.import_id = ids.join(", ");
                self.snapshot_name = format!("Backup {}", snapshots::date(backend::now()));
                self.dialog = Some(Dialog::NewSnapshot);
            }
            Message::SnapshotFor(id) => {
                self.import_id = id;
                self.snapshot_name = format!("Backup {}", snapshots::date(backend::now()));
                self.dialog = Some(Dialog::NewSnapshot);
            }
            Message::FindPackage(id) => {
                self.install_query = id;
                return self.switch(Page::Install);
            }
            Message::UninstallData(v) => self.uninstall_data = v,
            Message::BatchRestore => {
                return self.update(Message::Ask(Operation::RestoreMany(
                    self.selected_snapshots(),
                )));
            }
            Message::BatchSnapshotTrash => {
                return self.update(Message::Ask(Operation::TrashSnapshots(
                    self.selected_snapshots(),
                )));
            }
            Message::BatchInstall => self.dialog = Some(Dialog::InstallIds(self.selected_ids())),
            Message::Launch(p) => {
                let result = backend::launch(&p);
                self.set_notice(result);
            }
            Message::Open(p) => {
                let result = backend::open(&p);
                self.set_notice(result);
            }
            Message::AddRemote => {
                self.prepare_remote_add();
                self.dialog = Some(Dialog::Remote);
            }
            Message::AddRemoteFile => {
                self.prepare_remote_add();
                return pick_file(FilePurpose::Remote);
            }
            Message::PopularRemote(index) => {
                if let Some(preset) = remotes::POPULAR.get(index) {
                    self.prepare_remote_add();
                    self.remote_name = preset.name.clone();
                    self.remote_url = preset.url.clone();
                    self.dialog = Some(Dialog::Remote);
                }
            }
            Message::ShowDisabledRemotes(show) => {
                self.show_disabled_remotes = show;
                self.remote_menu = None;
            }
            Message::RemoteMenu(key) => self.remote_menu = key,
            Message::RemoteDetails(remote) => {
                self.remote_menu = None;
                self.dialog = Some(Dialog::RemoteDetails(remote));
            }
            Message::BrowseRemote(remote) => {
                let _ = self.switch(Page::Packages);
                self.origin = self
                    .origins()
                    .iter()
                    .position(|name| name == &remote.name)
                    .unwrap_or(0);
                self.installation = self
                    .installations()
                    .iter()
                    .position(|name| name == &remote.installation)
                    .unwrap_or(0);
                self.apps = true;
                self.runtimes = true;
                self.runtime = 0;
                self.eol = false;
                self.filters = true;
                self.selected = None;
                self.detail_loading = false;
                if let Some(package) = self.visible_packages().first() {
                    return self.select_package(package.key());
                }
            }
            Message::InstallSearch => {
                if !self.searching {
                    if let Some(r) = self
                        .install_remotes()
                        .get(self.install_remote)
                        .map(|r| (*r).clone())
                    {
                        self.searching = true;
                        let query = self.install_query.clone();
                        return background(
                            move || backend::remote_packages(&r, &query),
                            Message::Results,
                        );
                    } else {
                        self.notice = Some("Choose an enabled remote first".into());
                    }
                }
            }
            Message::Results(r) => {
                self.searching = false;
                match r {
                    Ok(results) => self.results = results,
                    Err(e) => self.notice = Some(e),
                }
            }
            Message::InstallScope(i) => {
                self.install_scope = i;
                self.install_remote = 0;
                self.results.clear();
                self.queue.clear();
            }
            Message::InstallRemote(i) => {
                self.install_remote = i;
                self.results.clear();
                self.queue.clear();
            }
            Message::Queue(reference, on) => {
                if on {
                    self.queue.insert(reference);
                } else {
                    self.queue.remove(&reference);
                }
            }
            Message::InstallQueue => {
                if let Some(r) = self.install_remotes().get(self.install_remote) {
                    self.dialog = Some(Dialog::Confirm(Operation::Install {
                        installation: r.installation.clone(),
                        remote: r.name.clone(),
                        refs: self.queue.iter().cloned().collect(),
                    }));
                }
            }
            Message::ChooseFile(purpose) => return pick_file(purpose),
            Message::FilePicked(purpose, result) => match result {
                Err(e) => {
                    if !e.to_lowercase().contains("cancel") {
                        self.notice = Some(e);
                    }
                }
                Ok(path) => match purpose {
                    FilePurpose::Install => self.review_install_files(vec![path]),
                    FilePurpose::Remote => {
                        if self.remote_name.is_empty() {
                            self.remote_name = path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                        }
                        self.remote_url = path.display().to_string();
                        self.dialog = Some(Dialog::Remote);
                    }
                    FilePurpose::Snapshot => self.import_path = Some(path),
                    FilePurpose::Export(s) => {
                        return self.run(Operation::ExportSnapshot(s, path));
                    }
                },
            },
            Message::FilesPicked(result) => match result {
                Ok(files) => {
                    if files.len() == 1 && files[0].extension().is_some_and(|e| e == "flatpakrepo")
                    {
                        self.prepare_remote_add();
                        self.remote_name = files[0]
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        self.remote_url = files[0].display().to_string();
                        self.dialog = Some(Dialog::Remote);
                    } else {
                        self.review_install_files(files);
                    }
                }
                Err(e) => {
                    if !e.to_lowercase().contains("cancel") {
                        self.notice = Some(e);
                    }
                }
            },
            Message::ImportSnapshot => {
                if self.page == Page::Data {
                    self.install_query = self.selected.clone().unwrap_or_default();
                    return self.switch(Page::Install);
                }
                self.import_id.clear();
                self.import_path = None;
                self.snapshot_name = "Imported snapshot".into();
                self.dialog = Some(Dialog::ImportSnapshot);
            }
            Message::RenameSnapshot(s) => {
                self.snapshot_name = s.name.clone();
                self.dialog = Some(Dialog::Rename(s));
            }
            Message::About => {
                self.remote_menu = None;
                self.set_show_context(false);
                self.dialog = Some(Dialog::About);
            }
            Message::Settings => {
                self.remote_menu = None;
                self.set_show_context(!self.core.window.show_context);
            }
            Message::CloseSettings => self.set_show_context(false),
            Message::Theme(i) => {
                if let Some(theme) = AppTheme::ALL.get(i).copied() {
                    if self.preferences.theme == theme {
                        return Task::none();
                    }
                    self.preferences.theme = theme;
                    if let Err(error) = preferences::save(&self.preferences) {
                        self.notice = Some(format!("{error}\nThe theme applies for this session."));
                    }
                    return cosmic::command::set_theme(theme.theme());
                }
            }
        }
        Task::none()
    }
}
fn pick_file(purpose: FilePurpose) -> Task<Message> {
    cosmic::task::future(async move {
        use cosmic::dialog::file_chooser::{self, FileFilter};
        if matches!(purpose, FilePurpose::Install) {
            let result = file_chooser::open::Dialog::new()
                .title("Open Flatpak Files")
                .filter(
                    FileFilter::new("Flatpak Packages and Remotes")
                        .glob("*.flatpak")
                        .glob("*.flatpakref")
                        .glob("*.flatpakrepo"),
                )
                .open_files()
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| {
                    r.urls()
                        .iter()
                        .map(|url| url.to_file_path().map_err(|_| "Select local files".into()))
                        .collect()
                });
            return Message::FilesPicked(result);
        }
        let result = if let FilePurpose::Export(s) = &purpose {
            file_chooser::save::Dialog::new()
                .title("Export Snapshot".into())
                .file_name(
                    s.path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                )
                .save_file()
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| {
                    r.url()
                        .ok_or_else(|| "No output file selected".to_string())?
                        .to_file_path()
                        .map_err(|_| "Select a local file".into())
                })
        } else {
            let filter = match purpose {
                FilePurpose::Install => FileFilter::new("Flatpak Packages")
                    .glob("*.flatpak")
                    .glob("*.flatpakref"),
                FilePurpose::Remote => FileFilter::new("Flatpak Remotes").glob("*.flatpakrepo"),
                _ => FileFilter::new("Snapshots").glob("*.tar.zst"),
            };
            file_chooser::open::Dialog::new()
                .title("Open File")
                .filter(filter)
                .open_file()
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| {
                    r.url()
                        .to_file_path()
                        .map_err(|_| "Select a local file".into())
                })
        };
        Message::FilePicked(purpose, result)
    })
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    use cosmic::Application;
    fn app() -> App {
        let mut core = cosmic::Core::default();
        core.set_main_window_id(Some(iced::window::Id::unique()));
        // Tasks are deliberately dropped: these tests never launch host work.
        let (mut app, _) = App::init(core, Flags::default());
        app.inventory.installations = vec!["system".into(), "user".into()];
        app
    }
    #[test]
    fn opening_a_bundle_reviews_it_for_the_user_without_starting_an_install() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("example.flatpak");
        std::fs::write(&file, "UI fixture; not an installable bundle").unwrap();
        let mut app = app();
        let _ = app.update(Message::FilesPicked(Ok(vec![file.clone()])));
        assert!(matches!(&app.dialog, Some(Dialog::InstallFiles(paths)) if paths == &[file]));
        assert!(matches!(app.page, Page::Install));
        assert_eq!(app.inventory.installations[app.install_scope], "user");
        assert!(app.job.is_none());
        assert!(app.ui_activate("install.confirm", false, false).is_err());
        assert!(matches!(
            app.ui_activate("install.confirm", true, false),
            Ok(Message::InstallFilesConfirmed)
        ));
        let _ = app.update(Message::InstallScope(0));
        assert_eq!(app.inventory.installations[app.install_scope], "system");
        assert!(app.ui_activate("install.confirm", true, false).is_err());
        assert!(matches!(
            app.ui_activate("install.confirm", true, true),
            Ok(Message::InstallFilesConfirmed)
        ));
        let _ = app.update(Message::CloseDialog);
        assert!(app.dialog.is_none() && app.job.is_none());
    }
    #[test]
    fn remote_shortcut_filters_the_installation_even_when_names_are_shared() {
        let mut app = app();
        let remote = Remote {
            name: "flathub".into(),
            title: "Flathub".into(),
            url: "https://dl.flathub.org/repo/".into(),
            options: String::new(),
            installation: "user".into(),
        };
        app.inventory.remotes = vec![remote.clone()];
        let package = Package {
            name: "Example".into(),
            id: "org.example.App".into(),
            version: "1".into(),
            branch: "stable".into(),
            arch: "x86_64".into(),
            origin: "flathub".into(),
            installation: "system".into(),
            reference: "app/org.example.App/x86_64/stable".into(),
            size: String::new(),
            options: String::new(),
            runtime: String::new(),
            masked: false,
            pinned: false,
            masks: vec![],
            pins: vec![],
        };
        app.inventory.packages = vec![
            package.clone(),
            Package {
                installation: "user".into(),
                ..package
            },
        ];
        app.query = "an earlier search".into();
        app.eol = true;
        app.apps = false;
        let _ = app.update(Message::BrowseRemote(remote));
        let visible = app.visible_packages();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].installation, "user");
        assert!(app.filters && app.query.is_empty() && !app.eol);
    }
    #[test]
    fn file_activation_reuses_the_review_and_preserves_the_chosen_installation() {
        let mut app = app();
        let identity = app.ui_instance.clone();
        let activation = || cosmic::dbus_activation::Message {
            activation_token: None,
            desktop_startup_id: None,
            msg: cosmic::dbus_activation::Details::ActivateAction {
                action: "open-files".into(),
                args: vec!["/tmp/example.flatpak".into()],
            },
        };
        let _ = app.dbus_activation(activation());
        let _ = app.update(Message::InstallScope(0));
        let _ = app.dbus_activation(activation());
        assert!(matches!(&app.dialog, Some(Dialog::InstallFiles(paths)) if paths.len() == 1));
        assert_eq!(app.ui_instance, identity);
        assert_eq!(app.inventory.installations[app.install_scope], "system");
        assert!(app.job.is_none());
    }
}
