// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    backend::{self, DataFolder, InstallGroup, Inventory, Job, Operation, Package, Paths, Remote},
    preferences::{self, AppTheme, Preferences},
    remotes,
    snapshots::{self, Snapshot},
    view_state::{self, DataSort, PackageSort, ViewState},
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
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
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
    pub const ALL: [Self; 5] = [
        Self::Packages,
        Self::Remotes,
        Self::Data,
        Self::Snapshots,
        Self::Install,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Packages => "Packages",
            Self::Remotes => "Remotes",
            Self::Data => "User Data",
            Self::Snapshots => "Snapshots",
            Self::Install => "Install Packages",
        }
    }
    /// The stable identifier used by `ui navigate` and the saved view state.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Packages => "packages",
            Self::Remotes => "remotes",
            Self::Data => "data",
            Self::Snapshots => "snapshots",
            Self::Install => "install",
        }
    }
    fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|page| page.slug() == slug)
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Field {
    RemoteName,
    RemoteUrl,
    InstallQuery,
    SnapshotName,
    SnapshotSearch,
    ImportId,
    Commit,
}
/// A queued remote package: installation, remote and full ref.
pub type QueueKey = (String, String, String);
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
    RemoteChoice(usize),
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
    FilterRemote(String, bool),
    FilterRuntime(String, bool),
    FilterInstallation(String),
    ResetFilters,
    PackageSort(PackageSort),
    Leftovers(bool),
    DataSort(DataSort),
    DataDescending(bool),
    Details(String, Result<String, String>),
    History,
    HistoryLoaded(String, Result<String, String>),
    CommitChoice(String),
    Field(Field, String),
    Hold(bool),
    Ask(Operation),
    Confirm,
    ShowCommands(bool),
    CloseDialog,
    Finished(Box<Outcome>),
    ShowOutput(bool),
    ShowReport,
    OpenReportFolder,
    ClearOutcome,
    Cancel,
    ClearNotice,
    Uninstall(Vec<Package>),
    UninstallData(bool),
    BatchUninstall,
    BatchTrash,
    BatchSnapshot,
    BatchRestore,
    BatchSnapshotTrash,
    BatchInstall,
    Launch(Package),
    Open(PathBuf),
    ShowRuntime(String),
    ViewSnapshots(String),
    AddRemote,
    AddRemoteFile,
    PopularRemote(usize),
    ShowDisabledRemotes(bool),
    RemoteMenu(Option<(String, String)>),
    RemoteDetails(Remote),
    BrowseRemote(Remote),
    InstallBrowse(Option<(String, String)>),
    InstallSearch,
    Results((String, String), Result<Vec<(String, String)>, String>),
    InstallScope(usize),
    Queue(QueueKey, bool),
    ClearQueue,
    InstallQueue,
    ChooseFile(FilePurpose),
    FilePicked(FilePurpose, Result<PathBuf, String>),
    FilesPicked(Result<Vec<PathBuf>, String>),
    Dropped(String, Vec<u8>),
    DragOver(bool),
    FileTransfer(String),
    NewSnapshot,
    SnapshotFor(String),
    SnapshotPick(String, bool),
    SnapshotPickAll(bool),
    CreateSnapshot,
    FindPackage(String),
    ImportSnapshot,
    RenameSnapshot(Snapshot),
    CloseToast(widget::ToastId),
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
    pub view_state: ViewState,
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
    Uninstall(Vec<Package>),
    Remote,
    RemoteDetails(Remote),
    NewSnapshot,
    ImportSnapshot,
    Rename(Snapshot),
    Version(Package),
    Message(String, String),
    Report,
    About,
}
/// The result of one backend operation, kept for the result panel, the
/// details dialog and GUI automation.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub ok: bool,
    pub cancelled: bool,
    pub steps: Vec<String>,
    pub errors: Vec<String>,
    pub output: Vec<String>,
    pub report: Option<PathBuf>,
}
impl Outcome {
    fn failed(error: String) -> Self {
        Self {
            ok: false,
            cancelled: false,
            steps: vec![],
            errors: vec![error],
            output: vec![],
            report: None,
        }
    }
    fn from_report(report: backend::Report) -> Self {
        Self {
            ok: report.errors.is_empty() && !report.cancelled,
            cancelled: report.cancelled,
            steps: report.steps,
            errors: report.errors,
            output: report.output,
            report: Some(report.log),
        }
    }
}
/// Plain-language wording for an operation while it runs and when it ends.
#[derive(Debug, Clone, Default)]
pub struct JobView {
    pub working: String,
    pub done: String,
    pub failed: String,
    pub cancelled: String,
    pub icon: Option<String>,
}
/// Flatpak progress output reduced to one readable line and a percentage.
pub fn progress_status(
    lines: &[String],
    name: impl Fn(&str) -> String,
) -> (Option<String>, Option<f32>) {
    for line in lines.iter().rev() {
        let line = line.rsplit('\r').next().unwrap_or(line);
        // Commands and Flatpak's side notes stay in the details view.
        if line.starts_with("$ ") || line.starts_with("F: ") || line.starts_with("Warning:") {
            continue;
        }
        let cleaned: String = line
            .chars()
            .filter(|c| !('\u{2580}'..='\u{259f}').contains(c))
            .collect();
        let words: Vec<String> = cleaned
            .split_whitespace()
            .map(|word| {
                let mut parts = word.split('/');
                match (parts.next(), parts.next()) {
                    (Some("app" | "runtime"), Some(id)) if word.matches('/').count() >= 2 => {
                        name(id)
                    }
                    _ => word.to_string(),
                }
            })
            .collect();
        if words.is_empty() {
            continue;
        }
        let percent = words.iter().rev().find_map(|w| {
            w.strip_suffix('%')
                .and_then(|n| n.parse::<f32>().ok())
                .filter(|n| (0.0..=100.0).contains(n))
        });
        let text = words
            .into_iter()
            .filter(|w| !w.ends_with('%'))
            .collect::<Vec<_>>()
            .join(" ");
        return (
            (!text.is_empty()).then_some(text),
            percent.map(|p| p / 100.0),
        );
    }
    (None, None)
}
pub struct App {
    flags: Flags,
    capture_ticks: u8,
    search_id: widget::Id,
    remote_choice: usize,
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
    view: ViewState,
    restored_page: bool,
    leftovers: bool,
    detail: String,
    detail_loading: bool,
    history: Option<Result<Vec<backend::Commit>, String>>,
    dialog: Option<Dialog>,
    show_commands: bool,
    notice: Option<String>,
    job: Option<Job>,
    job_view: JobView,
    show_output: bool,
    job_started: Option<std::time::Instant>,
    outcome: Option<(JobView, Outcome)>,
    uninstall_data: bool,
    remote_name: String,
    remote_url: String,
    show_disabled_remotes: bool,
    remote_menu: Option<(String, String)>,
    install_query: String,
    install_scope: usize,
    install_target: Option<(String, String)>,
    results: Vec<(String, String)>,
    queue: BTreeMap<QueueKey, String>,
    searching: bool,
    searched: bool,
    snapshot_name: String,
    snapshot_pick: BTreeSet<String>,
    snapshot_query: String,
    import_id: String,
    import_path: Option<PathBuf>,
    commit: String,
    hold: bool,
    drag_over: bool,
    /// Sizes of files under install review, looked up once (on the host when
    /// the sandbox cannot see them).
    file_sizes: HashMap<PathBuf, Option<u64>>,
    toasts: widget::Toasts<Message>,
    last_toast: Option<String>,
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
            Err(e) => Message::Finished(Box::new(Outcome::failed(format!(
                "The background worker stopped unexpectedly: {e}"
            )))),
        }
    })
}

fn button<'a>(label: &str, message: Message) -> Element<'a, Message> {
    widget::button::standard(label.to_string())
        .on_press(message)
        .into()
}
/// A compact toggle: the selected choice keeps the accent background and a checkmark.
fn chip<'a>(
    label: impl Into<Cow<'a, str>>,
    selected: bool,
    message: Message,
) -> widget::button::TextButton<'a, Message> {
    let control = if selected {
        widget::button::suggested(label).leading_icon(icon::from_name("object-select-symbolic"))
    } else {
        widget::button::standard(label)
    };
    control.on_press(message)
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
fn single_line<'a>(text: widget::Text<'a, cosmic::Theme>) -> widget::Text<'a, cosmic::Theme> {
    text.width(Length::Fill)
        .wrapping(iced::advanced::text::Wrapping::None)
        .ellipsize(iced::advanced::text::Ellipsize::End(
            iced::advanced::text::EllipsizeHeightLimit::Lines(1),
        ))
}
fn section<'a>(
    title: &'a str,
    description: &'a str,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    widget::column::with_capacity(2)
        .spacing(12)
        .push(
            widget::column::with_capacity(2)
                .spacing(4)
                .push(widget::text::heading(title))
                .push(widget::text::caption(description)),
        )
        .push(body)
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
                .push(
                    widget::text(value.to_string())
                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                )
                .width(Length::Fill),
        )
        .push(
            widget::button::icon(icon::from_name("edit-copy-symbolic"))
                .on_press(Message::Copy(value.into()))
                .tooltip("Copy"),
        )
        .into()
}
fn file_kind(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if name.ends_with(".tar.zst") {
        "snapshot"
    } else if name.ends_with(".flatpak") || name.ends_with(".flatpakref") {
        "package"
    } else if name.ends_with(".flatpakrepo") {
        "remote"
    } else {
        "other"
    }
}
/// Parse a `text/uri-list` drop. Comments and non-file URIs are ignored.
pub fn dropped_paths(data: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(data)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| url::Url::parse(line).ok())
        .filter_map(|url| url.to_file_path().ok())
        .collect()
}
/// The most useful line for a person: Flatpak's own "error:" message when
/// there is one, otherwise the backend error without the command line.
fn elapsed_text(seconds: u64) -> String {
    match seconds {
        0 => "Just started".into(),
        s if s < 60 => format!("{s} s"),
        s => format!("{} min {} s", s / 60, s % 60),
    }
}
fn plain_error(error: &str, output: &[String]) -> String {
    if let Some(line) = output.iter().rev().find_map(|l| {
        l.trim()
            .strip_prefix("error:")
            .or_else(|| l.trim().strip_prefix("Error:"))
    }) {
        let line = line.trim();
        // Capitalize prose, not app IDs such as "org.example.App".
        let first = line.split_whitespace().next().unwrap_or_default();
        let mut text = if first.contains('.') {
            line.to_string()
        } else {
            line[..1].to_uppercase() + &line[1..]
        };
        if !text.ends_with('.') {
            text.push('.');
        }
        return text;
    }
    if error.contains(" exited with ") {
        return "Flatpak reported a problem. Open Details for its output.".into();
    }
    error.to_string()
}
impl App {
    fn refresh(&mut self) -> Task<Message> {
        self.remote_menu = None;
        self.loading = true;
        let p = self.paths.clone();
        background(move || backend::load(&p), Message::Loaded)
    }
    fn toast(&mut self, text: impl Into<String>) -> Task<Message> {
        let text = text.into();
        self.last_toast = Some(text.clone());
        self.toasts
            .push(widget::Toast::new(text))
            .map(cosmic::Action::App)
    }
    fn app_name(&self, id: &str) -> String {
        self.inventory
            .packages
            .iter()
            .find(|p| p.id == id && !p.is_runtime())
            .map(|p| p.name.clone())
            .unwrap_or_else(|| id.rsplit('.').next().unwrap_or(id).to_string())
    }
    fn app_installed(&self, id: &str) -> bool {
        self.inventory
            .packages
            .iter()
            .any(|p| p.id == id && !p.is_runtime())
    }
    /// Remote filter choices: configured remotes plus origins only used by packages.
    fn remote_filter_options(&self) -> Vec<(String, String)> {
        let mut options = BTreeMap::new();
        for r in &self.inventory.remotes {
            options.insert(
                format!("{}:{}", r.installation, r.name),
                format!("{} ({})", remotes::title(r), r.installation),
            );
        }
        for p in &self.inventory.packages {
            options
                .entry(format!("{}:{}", p.installation, p.origin))
                .or_insert_with(|| format!("{} ({})", p.origin, p.installation));
        }
        options.into_iter().collect()
    }
    fn runtime_options(&self) -> Vec<String> {
        self.inventory
            .packages
            .iter()
            .map(|p| p.runtime.clone())
            .filter(|r| !r.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    fn visible_packages(&self) -> Vec<&Package> {
        let v = &self.view;
        let q = self.query.to_lowercase();
        let mut list: Vec<_> = self
            .inventory
            .packages
            .iter()
            .filter(|p| {
                ((v.applications && !p.is_runtime()) || (v.runtimes && p.is_runtime()))
                    && (!v.eol_only || p.eol())
                    && (v.remotes.is_empty()
                        || v.remotes
                            .contains(&format!("{}:{}", p.installation, p.origin)))
                    && (v.runtime_refs.is_empty() || v.runtime_refs.contains(&p.runtime))
                    && (v.installation.is_empty() || v.installation == p.installation)
                    && format!(
                        "{} {} {} {} {}",
                        p.name, p.id, p.branch, p.origin, p.version
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .collect();
        match v.package_sort {
            PackageSort::Name => list.sort_by_key(|p| (p.name.to_lowercase(), p.id.to_lowercase())),
            PackageSort::Id => list.sort_by_key(|p| p.id.to_lowercase()),
            PackageSort::Remote => list.sort_by_key(|p| (p.origin.clone(), p.name.to_lowercase())),
        };
        list
    }
    fn package(&self) -> Option<&Package> {
        let key = self.selected.as_ref()?;
        self.inventory.packages.iter().find(|p| p.key() == *key)
    }
    fn runtime_package(&self, runtime: &str, installation: &str) -> Option<&Package> {
        self.inventory
            .packages
            .iter()
            .filter(|p| p.is_runtime() && p.full_ref().strip_prefix("runtime/") == Some(runtime))
            .min_by_key(|p| p.installation != installation)
    }
    fn origin_remote(&self, p: &Package) -> Option<&Remote> {
        self.inventory
            .remotes
            .iter()
            .find(|r| r.installation == p.installation && r.name == p.origin && !r.disabled())
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
        self.history = None;
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
        self.view.page = page.slug().into();
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
    fn package_name(&self, id: &str) -> String {
        self.inventory
            .packages
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| id.to_string())
    }
    /// Plain-language titles for an operation, naming the apps it touches.
    fn job_view(&self, op: &Operation) -> JobView {
        let names = |packages: &[Package]| match packages {
            [p] => p.name.clone(),
            list => format!("{} packages", list.len()),
        };
        let apps = |ids: &[String]| match ids {
            [id] => self.app_name(id),
            list => format!("{} apps", list.len()),
        };
        let file = |path: &Path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .trim_end_matches(".flatpakref")
                .trim_end_matches(".flatpak")
                .to_string()
        };
        let refs = |refs: &[String]| match refs {
            [r] => self.package_name(r.split('/').nth(1).unwrap_or(r)),
            list => format!("{} packages", list.len()),
        };
        let first_id = |packages: &[Package]| packages.first().map(|p| p.id.clone());
        let (working, done, failed, icon) = match op {
            Operation::Update(p) => (
                format!("Updating {}", names(p)),
                format!("{} updated", names(p)),
                format!("Couldn't update {}", names(p)),
                first_id(p),
            ),
            Operation::Reinstall { package, remote } if package.origin == remote.name => (
                format!("Reinstalling {}", package.name),
                format!("{} reinstalled", package.name),
                format!("Couldn't reinstall {}", package.name),
                Some(package.id.clone()),
            ),
            Operation::Reinstall { package, remote } => (
                format!("Switching {} to {}", package.name, remotes::title(remote)),
                format!("{} now comes from {}", package.name, remotes::title(remote)),
                format!(
                    "Couldn't switch {} to {}",
                    package.name,
                    remotes::title(remote)
                ),
                Some(package.id.clone()),
            ),
            Operation::Uninstall(p, _) => (
                format!("Uninstalling {}", names(p)),
                format!("{} uninstalled", names(p)),
                format!("Couldn't uninstall {}", names(p)),
                first_id(p),
            ),
            Operation::Mask(p, true) => (
                format!("Blocking updates for {}", p.name),
                format!("Updates blocked for {}", p.name),
                format!("Couldn't block updates for {}", p.name),
                Some(p.id.clone()),
            ),
            Operation::Mask(p, false) => (
                format!("Allowing updates for {}", p.name),
                format!("Updates allowed for {}", p.name),
                format!("Couldn't allow updates for {}", p.name),
                Some(p.id.clone()),
            ),
            Operation::Pin(p, on) => (
                format!("{} {}", if *on { "Pinning" } else { "Unpinning" }, p.name),
                format!("{} {}", p.name, if *on { "pinned" } else { "unpinned" }),
                format!("Couldn't {} {}", if *on { "pin" } else { "unpin" }, p.name),
                Some(p.id.clone()),
            ),
            Operation::Version(p, ..) => (
                format!("Changing the version of {}", p.name),
                format!("{} changed to the selected version", p.name),
                format!("Couldn't change the version of {}", p.name),
                Some(p.id.clone()),
            ),
            Operation::RemoteEnabled(r, on) => (
                format!(
                    "{} {}",
                    if *on { "Enabling" } else { "Disabling" },
                    remotes::title(r)
                ),
                format!(
                    "{} {}",
                    remotes::title(r),
                    if *on { "enabled" } else { "disabled" }
                ),
                format!(
                    "Couldn't {} {}",
                    if *on { "enable" } else { "disable" },
                    remotes::title(r)
                ),
                None,
            ),
            Operation::RemoteRemove(r) => (
                format!("Removing {}", remotes::title(r)),
                format!("{} removed", remotes::title(r)),
                format!("Couldn't remove {}", remotes::title(r)),
                None,
            ),
            Operation::RemoteAdd { name, .. } => (
                format!("Adding {name}"),
                format!("{name} added"),
                format!("Couldn't add {name}"),
                None,
            ),
            Operation::Install { refs: r, .. } => (
                format!("Installing {}", refs(r)),
                format!("{} installed", refs(r)),
                format!("Couldn't install {}", refs(r)),
                None,
            ),
            Operation::InstallQueue(groups) => {
                let all: Vec<String> = groups.iter().flat_map(|g| g.refs.clone()).collect();
                (
                    format!("Installing {}", refs(&all)),
                    format!("{} installed", refs(&all)),
                    format!("Couldn't install {}", refs(&all)),
                    None,
                )
            }
            Operation::InstallFile { path, .. } => (
                format!("Installing {}", file(path)),
                format!("{} installed", file(path)),
                format!("Couldn't install {}", file(path)),
                None,
            ),
            Operation::InstallFiles { paths, .. } => {
                let what = match paths.as_slice() {
                    [path] => file(path),
                    list => format!("{} packages", list.len()),
                };
                (
                    format!("Installing {what}"),
                    format!("{what} installed"),
                    format!("Couldn't install {what}"),
                    None,
                )
            }
            Operation::TrashData(ids) => (
                format!("Moving data of {} to Trash", apps(ids)),
                format!("Data of {} moved to Trash", apps(ids)),
                format!("Couldn't move data of {} to Trash", apps(ids)),
                ids.first().cloned(),
            ),
            Operation::Snapshot { ids, .. } => (
                format!("Backing up {}", apps(ids)),
                format!("Snapshot of {} created", apps(ids)),
                format!("Couldn't back up {}", apps(ids)),
                ids.first().cloned(),
            ),
            Operation::Restore(s) => (
                format!("Restoring {}", self.app_name(&s.id)),
                format!("{} restored from “{}”", self.app_name(&s.id), s.name),
                format!("Couldn't restore {}", self.app_name(&s.id)),
                Some(s.id.clone()),
            ),
            Operation::RestoreMany(list) => (
                format!("Restoring {} snapshots", list.len()),
                format!("{} snapshots restored", list.len()),
                "Couldn't restore the snapshots".to_string(),
                None,
            ),
            Operation::TrashSnapshot(s) => (
                format!("Moving “{}” to Trash", s.name),
                format!("“{}” moved to Trash", s.name),
                format!("Couldn't move “{}” to Trash", s.name),
                Some(s.id.clone()),
            ),
            Operation::TrashSnapshots(list) => (
                format!("Moving {} snapshots to Trash", list.len()),
                format!("{} snapshots moved to Trash", list.len()),
                "Couldn't move the snapshots to Trash".to_string(),
                None,
            ),
            Operation::ExportSnapshot(s, _) => (
                format!("Exporting “{}”", s.name),
                format!("“{}” exported", s.name),
                format!("Couldn't export “{}”", s.name),
                Some(s.id.clone()),
            ),
            Operation::RenameSnapshot(s, name) => (
                format!("Renaming “{}”", s.name),
                format!("Renamed to “{name}”"),
                format!("Couldn't rename “{}”", s.name),
                Some(s.id.clone()),
            ),
            Operation::ImportSnapshot { id, .. } => (
                format!("Importing a snapshot of {}", self.app_name(id)),
                format!("Snapshot of {} imported", self.app_name(id)),
                format!("Couldn't import the snapshot of {}", self.app_name(id)),
                Some(id.clone()),
            ),
        };
        JobView {
            cancelled: format!("{working} was cancelled"),
            working: format!("{working}…"),
            done,
            failed,
            icon,
        }
    }
    fn run(&mut self, op: Operation) -> Task<Message> {
        if self.job.is_some() {
            return Task::none();
        }
        self.job_view = self.job_view(&op);
        self.show_output = false;
        self.outcome = None;
        let job = Job::default();
        let work = job.clone();
        self.job = Some(job);
        self.job_started = Some(std::time::Instant::now());
        let paths = self.paths.clone();
        background(
            move || match backend::execute(op, &paths, &work) {
                Ok(report) => Outcome::from_report(report),
                Err(error) => Outcome::failed(error),
            },
            |outcome| Message::Finished(Box::new(outcome)),
        )
    }
    fn copy_packages(&self, field: fn(&Package) -> String) -> String {
        self.selected_packages()
            .iter()
            .map(field)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn data_name(&self, d: &DataFolder) -> String {
        self.app_name(&d.id)
    }
    fn visible_data(&self) -> Vec<&DataFolder> {
        let q = self.query.to_lowercase();
        let mut list: Vec<_> = self
            .inventory
            .data
            .iter()
            .filter(|d| {
                (d.installed != self.leftovers)
                    && format!("{} {}", d.id, self.data_name(d))
                        .to_lowercase()
                        .contains(&q)
            })
            .collect();
        match self.view.data_sort {
            DataSort::Name => {
                list.sort_by_key(|d| (self.data_name(d).to_lowercase(), d.id.clone()))
            }
            DataSort::Id => list.sort_by_key(|d| d.id.to_lowercase()),
            DataSort::Size => list.sort_by_key(|d| d.bytes),
        }
        if self.view.data_descending {
            list.reverse();
        }
        list
    }
    fn filtered_snapshots(&self) -> Vec<&Snapshot> {
        let q = self.query.to_lowercase();
        self.inventory
            .snapshots
            .iter()
            .filter(|s| {
                format!("{} {} {}", s.id, s.name, self.app_name(&s.id))
                    .to_lowercase()
                    .contains(&q)
            })
            .collect()
    }
    /// Snapshots grouped by application, newest first within each group.
    fn snapshot_groups(&self) -> Vec<(String, Vec<&Snapshot>)> {
        let mut groups: BTreeMap<String, Vec<&Snapshot>> = BTreeMap::new();
        for s in self.filtered_snapshots() {
            groups.entry(s.id.clone()).or_default().push(s);
        }
        let mut groups: Vec<_> = groups.into_iter().collect();
        for (_, items) in &mut groups {
            items.sort_by_key(|s| std::cmp::Reverse(s.timestamp));
        }
        groups.sort_by_key(|(id, _)| (self.app_name(id).to_lowercase(), id.clone()));
        groups
    }
    fn selected_snapshots(&self) -> Vec<Snapshot> {
        self.inventory
            .snapshots
            .iter()
            .filter(|s| self.selection.contains(&s.path.display().to_string()))
            .cloned()
            .collect()
    }
    /// Apps whose data can be snapshotted, filtered by the dialog's search.
    fn snapshot_candidates(&self) -> Vec<&DataFolder> {
        let q = self.snapshot_query.to_lowercase();
        let mut list: Vec<_> = self
            .inventory
            .data
            .iter()
            .filter(|d| {
                d.error.is_none()
                    && format!("{} {}", d.id, self.data_name(d))
                        .to_lowercase()
                        .contains(&q)
            })
            .collect();
        list.sort_by_key(|d| (!d.installed, self.data_name(d).to_lowercase()));
        list
    }
    fn enabled_remotes(&self) -> Vec<&Remote> {
        let mut list: Vec<_> = self
            .inventory
            .remotes
            .iter()
            .filter(|r| !r.disabled())
            .collect();
        list.sort_by_key(|r| {
            (
                match r.installation.as_str() {
                    "user" => 0,
                    "system" => 1,
                    _ => 2,
                },
                r.installation.clone(),
                remotes::title(r).to_lowercase(),
            )
        });
        list
    }
    fn target_remote(&self) -> Option<&Remote> {
        let (installation, name) = self.install_target.as_ref()?;
        self.inventory
            .remotes
            .iter()
            .find(|r| &r.installation == installation && &r.name == name)
    }
    fn is_installed(&self, installation: &str, reference: &str) -> bool {
        self.inventory
            .packages
            .iter()
            .any(|p| p.installation == installation && p.full_ref() == reference)
    }
    fn queue_groups(&self) -> Vec<InstallGroup> {
        let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for (installation, remote, reference) in self.queue.keys() {
            groups
                .entry((installation.clone(), remote.clone()))
                .or_default()
                .push(reference.clone());
        }
        groups
            .into_iter()
            .map(|((installation, remote), refs)| InstallGroup {
                installation,
                remote,
                refs,
            })
            .collect()
    }
    fn installation_choices(&self) -> widget::ListColumn<'_, Message> {
        let mut installations: Vec<_> = self.inventory.installations.iter().enumerate().collect();
        installations.sort_by_key(|(_, name)| match name.as_str() {
            "user" => 0,
            "system" => 1,
            _ => 2,
        });
        let mut choices = widget::list_column().list_item_padding([8, 12]);
        for (index, name) in installations {
            let (title, description) = match name.as_str() {
                "user" => ("User".to_string(), "Available only to you".to_string()),
                "system" => (
                    "System".to_string(),
                    "Available to everyone on this computer".to_string(),
                ),
                _ => (name.clone(), format!("Use the {name} installation")),
            };
            choices = choices.add(
                widget::settings::item::builder(title)
                    .description(description)
                    .radio(index, Some(self.install_scope), Message::InstallScope),
            );
        }
        choices
    }
    fn filters_view(&self) -> Element<'_, Message> {
        let v = &self.view;
        let mut col = widget::column::with_capacity(24)
            .spacing(10)
            .push(widget::text::heading("Show"))
            .push(
                widget::checkbox(v.applications)
                    .label("Applications")
                    .on_toggle(Message::Applications),
            )
            .push(
                widget::checkbox(v.runtimes)
                    .label("Runtimes")
                    .on_toggle(Message::Runtimes),
            )
            .push(
                widget::checkbox(v.eol_only)
                    .label("End of life only")
                    .on_toggle(Message::Eol),
            );
        let mut installations: Vec<Element<'_, Message>> = vec![
            chip(
                "All",
                v.installation.is_empty(),
                Message::FilterInstallation(String::new()),
            )
            .into(),
        ];
        for name in &self.inventory.installations {
            installations.push(
                chip(
                    name.clone(),
                    &v.installation == name,
                    Message::FilterInstallation(name.clone()),
                )
                .into(),
            );
        }
        col = col
            .push(widget::text::heading("Installation"))
            .push(widget::flex_row(installations).spacing(6).row_spacing(6))
            .push(widget::text::heading("Remotes"))
            .push(widget::text::caption(if v.remotes.is_empty() {
                "Showing every remote".to_string()
            } else {
                format!("{} selected", v.remotes.len())
            }));
        for (key, label) in self.remote_filter_options() {
            let k = key.clone();
            col = col.push(
                widget::checkbox(v.remotes.contains(&key))
                    .label(label)
                    .on_toggle(move |on| Message::FilterRemote(k.clone(), on)),
            );
        }
        let runtimes = self.runtime_options();
        if !runtimes.is_empty() {
            col = col
                .push(widget::text::heading("Runtimes"))
                .push(widget::text::caption(if v.runtime_refs.is_empty() {
                    "Showing apps using any runtime".to_string()
                } else {
                    format!("{} selected", v.runtime_refs.len())
                }));
            for runtime in runtimes {
                let r = runtime.clone();
                col = col.push(
                    widget::checkbox(v.runtime_refs.contains(&runtime))
                        .label(runtime)
                        .on_toggle(move |on| Message::FilterRuntime(r.clone(), on)),
                );
            }
        }
        col = col
            .push(widget::text::heading("Sort by"))
            .push(
                widget::flex_row(vec![
                    chip(
                        "Name",
                        v.package_sort == PackageSort::Name,
                        Message::PackageSort(PackageSort::Name),
                    )
                    .into(),
                    chip(
                        "App ID",
                        v.package_sort == PackageSort::Id,
                        Message::PackageSort(PackageSort::Id),
                    )
                    .into(),
                    chip(
                        "Remote",
                        v.package_sort == PackageSort::Remote,
                        Message::PackageSort(PackageSort::Remote),
                    )
                    .into(),
                ])
                .spacing(6)
                .row_spacing(6),
            )
            .push(
                widget::button::text("Reset Filters")
                    .leading_icon(icon::from_name("edit-clear-symbolic"))
                    .id(widget::Id::new("filters.reset"))
                    .on_press_maybe((!v.filters_are_default()).then_some(Message::ResetFilters)),
            );
        widget::container(panel(
            widget::scrollable(widget::container(col).padding([0, 12, 0, 0]))
                .height(Length::Shrink),
        ))
        .max_height(300)
        .into()
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
                    .push(single_line(widget::text::body(p.name.clone())))
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
            // No custom Id: libcosmic restores named widget state by name on
            // every rebuild, and named rows in a list that shrinks (for
            // example after Reset Filters) were handed empty state and
            // panicked while drawing. Automation selects rows by control ID.
            let b = widget::button::custom(row)
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
            list = list.push(widget::text(if self.inventory.packages.is_empty() {
                "No installed packages"
            } else {
                "No matching packages. Change the search or reset the filters."
            }));
        }
        let mut left = widget::column::with_capacity(4)
            .spacing(12)
            .push(widget::text::caption(format!(
                "{} of {} packages{}",
                packages.len(),
                self.inventory.packages.len(),
                if self.view.filters_are_default() {
                    ""
                } else {
                    " · filtered"
                }
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
            actions = actions.push(
                widget::button::suggested("Open")
                    .id(widget::Id::new("package.open"))
                    .on_press(Message::Launch(p.clone())),
            );
        }
        actions = actions.push(
            widget::button::standard("Uninstall")
                .id(widget::Id::new("package.uninstall"))
                .on_press(Message::Uninstall(vec![p.clone()])),
        );
        let mut col = widget::column::with_capacity(16)
            .spacing(20)
            .align_x(Alignment::Center)
            .push(package_icon(&p.id, 96))
            .push(widget::text::title2(&p.name))
            .push(widget::text::caption(&p.id))
            .push(actions);
        if p.eol() {
            let reason = p
                .options
                .split(',')
                .find_map(|o| o.strip_prefix("eol="))
                .unwrap_or_default()
                .replace('_', " ");
            col = col.push(panel(
                widget::row::with_capacity(2)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(ic("dialog-warning-symbolic", 20))
                    .push(
                        widget::text(format!(
                            "This package has reached its end of life and will not receive updates. {reason}"
                        ))
                        .width(Length::Fill),
                    ),
            ));
        }
        let mut settings = widget::column::with_capacity(8).spacing(14);
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
                    widget::button::standard("Change Version")
                        .id(widget::Id::new("package.change-version"))
                        .on_press(Message::History)
                        .into(),
                    widget::button::standard("Check for Updates")
                        .id(widget::Id::new("package.check-updates"))
                        .on_press(Message::CheckUpdates(p.clone()))
                        .into(),
                    widget::button::standard("Change Source")
                        .id(widget::Id::new("package.change-source"))
                        .on_press(Message::ChooseSource(p.clone()))
                        .into(),
                ])
                .spacing(8)
                .row_spacing(8),
            );
        if !p.runtime.is_empty() {
            let runtime = self
                .runtime_package(&p.runtime, &p.installation)
                .map(Package::key);
            settings = settings.push(
                widget::row::with_capacity(2)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .push(widget::container(info_row("Runtime", &p.runtime)).width(Length::Fill))
                    .push(
                        widget::button::text("Show")
                            .trailing_icon(icon::from_name("go-next-symbolic"))
                            .id(widget::Id::new("package.show-runtime"))
                            .on_press_maybe(runtime.map(Message::ShowRuntime)),
                    ),
            );
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
                    .label("Pin runtime (prevent automatic removal)")
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
        let shown = [
            "ID",
            "Ref",
            "Arch",
            "Branch",
            "Origin",
            "Installation",
            "Installed",
            "Runtime",
            "Version",
        ];
        for line in self.detail.lines() {
            if let Some((k, v)) = line.trim().split_once(':')
                && !v.trim().is_empty()
                && !shown.contains(&k.trim())
            {
                information = information.push(info_row(k.trim(), v.trim()));
            }
        }
        col = col.push(panel(information));
        let mut more = Vec::new();
        if !p.is_runtime() {
            more.push(
                widget::button::standard("New Snapshot")
                    .id(widget::Id::new("package.snapshot"))
                    .on_press(Message::SnapshotFor(p.id.clone()))
                    .into(),
            );
            more.push(
                widget::button::standard("View Snapshots")
                    .id(widget::Id::new("package.view-snapshots"))
                    .on_press(Message::ViewSnapshots(p.id.clone()))
                    .into(),
            );
            more.push(
                widget::button::standard("Copy Launch Command")
                    .id(widget::Id::new("package.copy-launch"))
                    .on_press(Message::Copy(format!(
                        "flatpak run {} {}",
                        backend::scope(&p.installation),
                        p.full_ref()
                    )))
                    .into(),
            );
        }
        more.push(
            widget::button::standard("Reinstall")
                .id(widget::Id::new("package.reinstall"))
                .on_press_maybe(self.origin_remote(p).map(|remote| {
                    Message::Ask(Operation::Reinstall {
                        package: p.clone(),
                        remote: remote.clone(),
                    })
                }))
                .into(),
        );
        col = col.push(widget::flex_row(more).spacing(8).row_spacing(8));
        widget::scrollable(widget::container(col).padding([12, 6]))
            .spacing(8)
            .height(Length::Fill)
            .into()
    }
    fn batch_view(&self) -> Element<'_, Message> {
        let count = self.selection.len();
        if self.page == Page::Snapshots {
            return panel(
                widget::column::with_capacity(2)
                    .spacing(12)
                    .push(widget::text::heading(format!("{count} selected")))
                    .push(
                        widget::flex_row(vec![
                            button("Select All", Message::SelectAll),
                            widget::button::standard("Copy IDs")
                                .on_press_maybe(
                                    (count > 0)
                                        .then(|| Message::Copy(self.selected_ids().join("\n"))),
                                )
                                .into(),
                            widget::button::standard("Restore")
                                .on_press_maybe((count > 0).then_some(Message::BatchRestore))
                                .into(),
                            widget::button::standard("Install Apps")
                                .on_press_maybe((count > 0).then_some(Message::BatchInstall))
                                .into(),
                            widget::button::standard("Trash")
                                .on_press_maybe((count > 0).then_some(Message::BatchSnapshotTrash))
                                .into(),
                        ])
                        .spacing(8)
                        .row_spacing(8),
                    ),
            );
        }
        let mut col = widget::column::with_capacity(8)
            .spacing(16)
            .push(widget::text::title2(format!("{count} selected")))
            .push(widget::text::caption(
                "Click items to select them. Delete removes the selection after confirmation; Escape leaves selection mode.",
            ));
        let mut copy = vec![button("Select All Visible", Message::SelectAll)];
        if count > 0 {
            if self.page == Page::Packages {
                copy.push(button(
                    "Copy Names",
                    Message::Copy(self.copy_packages(|p| p.name.clone())),
                ));
                copy.push(button(
                    "Copy IDs",
                    Message::Copy(self.copy_packages(|p| p.id.clone())),
                ));
                copy.push(button(
                    "Copy Refs",
                    Message::Copy(self.copy_packages(Package::full_ref)),
                ));
            } else {
                copy.push(button(
                    "Copy IDs",
                    Message::Copy(self.selected_data().join("\n")),
                ));
            }
        }
        col = col.push(widget::flex_row(copy).spacing(8).row_spacing(8));
        if count > 0 {
            let mut actions = Vec::new();
            if self.page == Page::Packages {
                actions.push(button("Update Selected", Message::BatchUpdate));
            }
            actions.push(button("Create Snapshot", Message::BatchSnapshot));
            if self.page == Page::Data && self.leftovers {
                actions.push(button(
                    "Find in a Remote",
                    Message::FindPackage(self.selected_data().join(" ")),
                ));
            }
            actions.push(
                widget::button::standard("Trash Data")
                    .on_press(Message::BatchTrash)
                    .into(),
            );
            if self.page == Page::Packages {
                actions.push(
                    widget::button::standard("Uninstall")
                        .on_press(Message::BatchUninstall)
                        .into(),
                );
            }
            col = col.push(widget::flex_row(actions).spacing(8).row_spacing(8));
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
                let actions = widget::column::with_capacity(7)
                    .spacing(2)
                    .push(
                        widget::button::text("Details")
                            .width(Length::Fill)
                            .on_press(Message::RemoteDetails(r.clone())),
                    )
                    .push(
                        widget::button::text("Search packages")
                            .width(Length::Fill)
                            .on_press_maybe((!r.disabled()).then(|| {
                                Message::InstallBrowse(Some((
                                    r.installation.clone(),
                                    r.name.clone(),
                                )))
                            })),
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
                        widget::button::text("Remove remote")
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
                            .push(single_line(widget::text::body(remotes::title(r))))
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
            let item =
                widget::settings::item::builder(&preset.title).description(&preset.description);
            popular = if remotes::added(preset, &self.inventory.remotes) {
                popular.add(item.control(widget::text::caption("Added")))
            } else {
                popular.add(
                    widget::list::button(item.control(ic("list-add-symbolic", 20)))
                        .on_press(Message::PopularRemote(index)),
                )
            };
        }
        let other = widget::list_column()
            .list_item_padding([8, 12])
            .add(
                widget::list::button(
                    widget::settings::item::builder("Add a repo file")
                        .description("Choose or drop a downloaded .flatpakrepo file")
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
            .push(section(
                "Add popular remotes",
                "Add remotes to get more software",
                popular,
            ))
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
    /// Route opened, dropped or forwarded files the way Warehouse does: package
    /// files to install review, one repo file to Add Remote, and (as an
    /// addition) one snapshot archive to Import. Nothing is applied here.
    fn open_files(&mut self, files: Vec<PathBuf>) {
        self.drag_over = false;
        if files.is_empty() {
            return;
        }
        if self.job.is_some() {
            self.flags.open_files.extend(files);
            self.notice = Some(
                "Opened files are queued for review after the current operation finishes.".into(),
            );
            return;
        }
        let kinds: BTreeSet<_> = files.iter().map(|path| file_kind(path)).collect();
        let message = |title: &str, body: &str| Some(Dialog::Message(title.into(), body.into()));
        if kinds.contains("other") {
            self.dialog = message(
                "Unsupported File Type",
                "Only .flatpak, .flatpakref, .flatpakrepo and .tar.zst snapshot files can be opened.",
            );
            return;
        }
        if kinds.len() > 1 {
            self.dialog = message(
                "Mixed File Types",
                "Packages, remotes and snapshots cannot be opened together. Open each kind separately.",
            );
            return;
        }
        match kinds.first().copied() {
            Some("remote") if files.len() > 1 => {
                self.dialog = message("Too Many Remotes", "Add one remote at a time.");
            }
            Some("remote") => {
                let _ = self.switch(Page::Remotes);
                self.prepare_remote_add();
                self.remote_name = files[0]
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.remote_url = files[0].display().to_string();
                self.dialog = Some(Dialog::Remote);
            }
            Some("snapshot") if files.len() > 1 => {
                self.dialog = message("Too Many Snapshots", "Import one snapshot at a time.");
            }
            Some("snapshot") => {
                let _ = self.switch(Page::Snapshots);
                self.import_id = files[0]
                    .parent()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .filter(|id| backend::validate_id(id).is_ok())
                    .unwrap_or_default();
                self.import_path = files.into_iter().next();
                self.snapshot_name = "Imported snapshot".into();
                self.dialog = Some(Dialog::ImportSnapshot);
            }
            _ => self.review_install_files(files),
        }
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
                self.file_sizes
                    .entry(file.clone())
                    .or_insert_with(|| warehouse_cosmic::host::file_size(&file));
                selection.push(file);
            }
        }
        self.remote_menu = None;
        self.set_show_context(false);
        let _ = self.switch(Page::Install);
        self.dialog = Some(Dialog::InstallFiles(selection));
    }
    fn save_window(&self) {
        if self.capture_instance {
            return;
        }
        if let Err(error) = window_state::save(&self.window_state) {
            eprintln!("Cannot save window state: {error}");
        }
        if let Err(error) = view_state::save(&self.view) {
            eprintln!("Cannot save filters: {error}");
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
        // Standard Wayland activation only: app behavior is the same on every desktop.
        native
    }
    fn data_view(&self) -> Element<'_, Message> {
        let data = self.visible_data();
        let total: u64 = data.iter().map(|d| d.bytes).sum();
        let filter = |label: &'static str, leftovers: bool, id: &'static str| {
            let selected = self.leftovers == leftovers;
            chip(label, selected, Message::Leftovers(leftovers))
                .id(widget::Id::new(id))
                .name(if selected {
                    format!("{label} (selected)")
                } else {
                    label.into()
                })
        };
        let sort = |label: &'static str, value: DataSort| {
            chip(
                label,
                self.view.data_sort == value,
                Message::DataSort(value),
            )
            .id(widget::Id::new(format!(
                "data.sort:{}",
                label.to_lowercase()
            )))
        };
        let descending = self.view.data_descending;
        let mut col = widget::column::with_capacity(5)
            .spacing(14)
            .push(
                widget::row::with_capacity(4)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .push(filter("Active Data", false, "data.active"))
                    .push(filter("Leftover Data", true, "data.leftover"))
                    .push(widget::Space::new().width(Length::Fill))
                    .push(widget::text(format!(
                        "{} folder{} · {}",
                        data.len(),
                        if data.len() == 1 { "" } else { "s" },
                        backend::size(total)
                    ))),
            )
            .push(
                widget::row::with_capacity(6)
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .push(widget::text::caption("Sort by"))
                    .push(sort("Name", DataSort::Name))
                    .push(sort("ID", DataSort::Id))
                    .push(sort("Size", DataSort::Size))
                    .push(
                        widget::button::icon(icon::from_name(if descending {
                            "view-sort-descending-symbolic"
                        } else {
                            "view-sort-ascending-symbolic"
                        }))
                        .id(widget::Id::new("data.sort-direction"))
                        .name(if descending {
                            "Descending order"
                        } else {
                            "Ascending order"
                        })
                        .tooltip(if descending {
                            "Descending: switch to ascending"
                        } else {
                            "Ascending: switch to descending"
                        })
                        .on_press(Message::DataDescending(!descending)),
                    ),
            );
        if self.batch {
            col = col.push(self.batch_view());
        }
        if data.is_empty() {
            col = col.push(self.empty(
                if !self.query.is_empty() {
                    "No matching data"
                } else if self.leftovers {
                    "No leftover data"
                } else {
                    "No active app data"
                },
                if self.leftovers {
                    "Data left behind by uninstalled apps appears here."
                } else {
                    "Data folders of installed apps appear here."
                },
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
            let name = self.data_name(d);
            let mut top = widget::row::with_capacity(3)
                .spacing(12)
                .align_y(Alignment::Center)
                .push(package_icon(&d.id, 40))
                .push(
                    widget::column::with_capacity(2)
                        .spacing(3)
                        .push(single_line(widget::text::heading(name)))
                        .push(widget::tooltip(
                            single_line(widget::text::caption(d.id.clone())),
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
                    widget::button::icon(icon::from_name("folder-download-symbolic"))
                        .on_press(Message::FindPackage(d.id.clone()))
                        .tooltip("Find this app in a remote"),
                );
            }
            let mut body = widget::column::with_capacity(3)
                .spacing(24)
                .push(top)
                .push(actions);
            if let Some(error) = &d.error {
                body = body.push(widget::text::caption(error));
            }
            let id = d.id.clone();
            let selected = self.selection.contains(&id);
            let card: Element<'_, Message> = if self.batch {
                widget::button::custom(panel(body))
                    .padding(0)
                    .class(package_row_style(selected))
                    .selected(selected)
                    .on_press(Message::Toggle(id, !selected))
                    .into()
            } else {
                panel(body)
            };
            cards.push(widget::container(card).width(card_width).into());
        }
        widget::scrollable(widget::flex_row(cards).spacing(12).width(Length::Fill))
            .spacing(8)
            .height(Length::Fill)
            .into()
    }
    fn snapshot_secondary_actions(&self) -> Vec<Element<'_, Message>> {
        vec![
            widget::button::text("Import")
                .leading_icon(icon::from_name("document-open-symbolic"))
                .id(widget::Id::new("snapshots.import"))
                .on_press(Message::ImportSnapshot)
                .into(),
            widget::button::text("Open Snapshot Folder")
                .leading_icon(icon::from_name("folder-open-symbolic"))
                .id(widget::Id::new("snapshots.open-library"))
                .on_press(Message::Open(self.paths.snapshots()))
                .into(),
        ]
    }
    fn snapshots_view(&self) -> Element<'_, Message> {
        if self.inventory.snapshots.is_empty() {
            return widget::container(
                widget::column::with_capacity(5)
                    .spacing(16)
                    .align_x(Alignment::Center)
                    .max_width(460)
                    .push(ic("document-open-recent-symbolic", 64))
                    .push(widget::text::title2("No Snapshots"))
                    .push(
                        widget::text(
                            "Snapshots back up an app's settings and content so you can restore them later.",
                        )
                        .align_x(iced::advanced::text::Alignment::Center),
                    )
                    .push(
                        widget::button::suggested("New Snapshot")
                            .leading_icon(icon::from_name("list-add-symbolic"))
                            .id(widget::Id::new("snapshots.new"))
                            .on_press(Message::NewSnapshot),
                    )
                    .push(
                        widget::row::with_children(self.snapshot_secondary_actions()).spacing(8),
                    ),
            )
            .center(Length::Fill)
            .into();
        }
        let mut col = widget::column::with_capacity(20).spacing(24);
        if self.batch {
            col = col.push(self.batch_view());
        }
        let groups = self.snapshot_groups();
        if groups.is_empty() {
            col = col.push(self.empty(
                "No matching snapshots",
                "Try another app name, ID or snapshot name.",
                "edit-find-symbolic",
            ));
        }
        for (id, items) in groups {
            let installed = self.app_installed(&id);
            let has_data = self.inventory.data.iter().any(|d| d.id == id);
            let mut header = widget::row::with_capacity(5)
                .spacing(12)
                .align_y(Alignment::Center)
                .push(package_icon(&id, 36))
                .push(
                    widget::column::with_capacity(2)
                        .spacing(2)
                        .push(single_line(widget::text::heading(self.app_name(&id))))
                        .push(single_line(widget::text::caption(format!(
                            "{id} · {} snapshot{}{}",
                            items.len(),
                            if items.len() == 1 { "" } else { "s" },
                            if installed { "" } else { " · Not installed" }
                        ))))
                        .width(Length::Fill),
                );
            if !installed {
                header = header.push(
                    widget::button::text("Install App")
                        .leading_icon(icon::from_name("folder-download-symbolic"))
                        .on_press(Message::FindPackage(id.clone())),
                );
            }
            if has_data {
                header = header.push(
                    widget::button::text("New Snapshot")
                        .leading_icon(icon::from_name("list-add-symbolic"))
                        .on_press(Message::SnapshotFor(id.clone())),
                );
            }
            let mut list = widget::list_column().list_item_padding([10, 12]);
            for s in items {
                let key = s.path.display().to_string();
                let mut top = widget::row::with_capacity(2)
                    .spacing(12)
                    .align_y(Alignment::Center);
                if self.batch {
                    let key2 = key.clone();
                    top = top.push(
                        widget::checkbox(self.selection.contains(&key))
                            .on_toggle(move |v| Message::Toggle(key2.clone(), v)),
                    );
                }
                top = top.push(
                    widget::column::with_capacity(2)
                        .spacing(2)
                        .push(single_line(widget::text::body(s.name.clone())))
                        .push(widget::text::caption(format!(
                            "{} · {} · Version {}{}",
                            snapshots::date(s.timestamp),
                            backend::size(s.bytes),
                            s.version,
                            if s.official {
                                " · Official Warehouse library (read-only)"
                            } else {
                                ""
                            }
                        )))
                        .width(Length::Fill),
                );
                let mut actions = vec![
                    widget::button::standard("Restore")
                        .on_press(Message::Ask(Operation::Restore(s.clone())))
                        .into(),
                    widget::button::standard("Export")
                        .on_press(Message::ChooseFile(FilePurpose::Export(s.clone())))
                        .into(),
                ];
                if s.official {
                    actions.push(
                        widget::button::standard("Import Copy")
                            .on_press(Message::Ask(Operation::ImportSnapshot {
                                id: s.id.clone(),
                                path: s.path.clone(),
                                name: s.name.clone(),
                            }))
                            .into(),
                    );
                } else {
                    actions.push(button("Rename", Message::RenameSnapshot(s.clone())));
                    actions.push(
                        widget::button::standard("Trash")
                            .on_press(Message::Ask(Operation::TrashSnapshot(s.clone())))
                            .into(),
                    );
                }
                list = list.add(
                    widget::column::with_capacity(2)
                        .spacing(10)
                        .push(top)
                        .push(widget::flex_row(actions).spacing(8).row_spacing(8)),
                );
            }
            col = col.push(
                widget::column::with_capacity(2)
                    .spacing(10)
                    .push(header)
                    .push(list),
            );
        }
        widget::scrollable(widget::container(col).padding([0, 12, 0, 0]))
            .spacing(8)
            .height(Length::Fill)
            .into()
    }
    fn install_view(&self) -> Element<'_, Message> {
        if self.install_target.is_some() {
            return self.install_search_view();
        }
        widget::responsive(move |size| self.install_main(size.width)).into()
    }
    fn install_main(&self, width: f32) -> Element<'_, Message> {
        let remotes = self.enabled_remotes();
        let mut list = widget::list_column().list_item_padding([8, 12]);
        for r in &remotes {
            list = list.add(
                widget::list::button(
                    widget::settings::item::builder(remotes::title(r).to_string())
                        .description(format!("Installation: {} · {}", r.installation, r.name))
                        .control(ic("go-next-symbolic", 16)),
                )
                .on_press(Message::InstallBrowse(Some((
                    r.installation.clone(),
                    r.name.clone(),
                )))),
            );
        }
        if remotes.is_empty() {
            list = list.add(
                widget::row::with_capacity(2)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(widget::text("No enabled remotes.").width(Length::Fill))
                    .push(button("Manage Remotes", Message::Page(Page::Remotes))),
            );
        }
        let left = widget::column::with_capacity(2)
            .spacing(32)
            .push(section(
                "Search in a Remote",
                "Choose a remote, search its packages, and add them to the pending list.",
                list,
            ))
            .push(section(
                "Add Packages",
                "Install .flatpak bundles or .flatpakref files from your computer. You can also drop them onto this window.",
                widget::button::suggested("Open Files")
                    .leading_icon(icon::from_name("document-open-symbolic"))
                    .id(widget::Id::new("install.choose-file"))
                    .on_press(Message::ChooseFile(FilePurpose::Install)),
            ));
        let body: Element<'_, Message> = if width >= 620. {
            widget::row::with_capacity(2)
                .spacing(28)
                .push(widget::container(left).width(Length::Fill))
                .push(
                    widget::container(self.pending_pane()).width(if width >= 900. {
                        340
                    } else {
                        280
                    }),
                )
                .into()
        } else {
            widget::column::with_capacity(2)
                .spacing(32)
                .push(left)
                .push(self.pending_pane())
                .into()
        };
        widget::scrollable(
            widget::container(widget::container(body).max_width(1040).width(Length::Fill))
                .center_x(Length::Fill)
                .padding([4, 12]),
        )
        .spacing(8)
        .height(Length::Fill)
        .into()
    }
    fn pending_pane(&self) -> Element<'_, Message> {
        let mut col = widget::column::with_capacity(8)
            .spacing(14)
            .push(widget::text::heading("Pending Packages"));
        if self.queue.is_empty() {
            col = col.push(
                widget::container(
                    widget::column::with_capacity(3)
                        .spacing(10)
                        .align_x(Alignment::Center)
                        .push(ic("folder-download-symbolic", 48))
                        .push(widget::text::heading("No Pending Packages"))
                        .push(
                            widget::text::caption(
                                "Packages you add from a remote search appear here for review before installing.",
                            )
                            .align_x(iced::advanced::text::Alignment::Center),
                        ),
                )
                .center_x(Length::Fill)
                .padding([20, 8]),
            );
            return panel(col);
        }
        let mut groups: BTreeMap<(String, String), Vec<(&QueueKey, &String)>> = BTreeMap::new();
        for (key, name) in &self.queue {
            groups
                .entry((key.0.clone(), key.1.clone()))
                .or_default()
                .push((key, name));
        }
        for ((installation, remote), items) in groups {
            let title = self
                .inventory
                .remotes
                .iter()
                .find(|r| r.installation == installation && r.name == remote)
                .map(|r| remotes::title(r).to_string())
                .unwrap_or_else(|| remote.clone());
            let mut list = widget::list_column().list_item_padding([6, 10]);
            for (key, name) in items {
                list = list.add(
                    widget::row::with_capacity(2)
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .push(
                            widget::column::with_capacity(2)
                                .push(single_line(widget::text::body(name.clone())))
                                .push(single_line(widget::text::caption(key.2.clone())))
                                .width(Length::Fill),
                        )
                        .push(
                            widget::button::icon(icon::from_name("list-remove-symbolic"))
                                .tooltip("Remove from pending")
                                .on_press(Message::Queue(key.clone(), false)),
                        ),
                );
            }
            col = col.push(
                widget::column::with_capacity(2)
                    .spacing(6)
                    .push(widget::text::caption(format!(
                        "{title} · Installation: {installation}"
                    )))
                    .push(list),
            );
        }
        col = col.push(
            widget::row::with_capacity(3)
                .spacing(8)
                .align_y(Alignment::Center)
                .push(
                    widget::button::text("Remove All")
                        .id(widget::Id::new("install.queue.clear"))
                        .on_press(Message::ClearQueue),
                )
                .push(widget::Space::new().width(Length::Fill))
                .push(
                    widget::button::suggested(format!("Install {}", self.queue.len()))
                        .id(widget::Id::new("install.queue.review"))
                        .on_press(Message::InstallQueue),
                ),
        );
        panel(col)
    }
    fn install_search_view(&self) -> Element<'_, Message> {
        let (installation, name) = self.install_target.clone().unwrap_or_default();
        let title = self
            .target_remote()
            .map(|r| remotes::title(r).to_string())
            .unwrap_or_else(|| name.clone());
        let header = widget::row::with_capacity(2)
            .spacing(12)
            .align_y(Alignment::Center)
            .push(
                widget::button::icon(icon::from_name("go-previous-symbolic"))
                    .id(widget::Id::new("install.back"))
                    .name("Back to Install Packages")
                    .tooltip("Back")
                    .on_press(Message::InstallBrowse(None)),
            )
            .push(
                widget::column::with_capacity(2)
                    .spacing(2)
                    .push(widget::text::title4(format!("Search {title}")))
                    .push(widget::text::caption(format!(
                        "Installation: {installation} · Remote: {name}"
                    ))),
            );
        let search = widget::row::with_capacity(2)
            .spacing(8)
            .push(
                widget::search_input("Search by name or application ID", &self.install_query)
                    .id(self.search_id.clone())
                    .on_input(|s| Message::Field(Field::InstallQuery, s))
                    .on_submit(|_| Message::InstallSearch),
            )
            .push(
                widget::button::suggested(if self.searching {
                    "Searching…"
                } else {
                    "Search"
                })
                .id(widget::Id::new("install.search"))
                .on_press_maybe((!self.searching).then_some(Message::InstallSearch)),
            );
        let mut results = widget::list_column().list_item_padding([8, 12]);
        const LIMIT: usize = 200;
        for (name, reference) in self.results.iter().take(LIMIT) {
            let key = (
                installation.clone(),
                self.install_target.clone().unwrap_or_default().1,
                reference.clone(),
            );
            let control: Element<'_, Message> = if self.is_installed(&installation, reference) {
                widget::text::caption("Installed").into()
            } else if self.queue.contains_key(&key) {
                widget::button::standard("Remove")
                    .leading_icon(icon::from_name("object-select-symbolic"))
                    .on_press(Message::Queue(key, false))
                    .into()
            } else {
                widget::button::standard("Add")
                    .leading_icon(icon::from_name("list-add-symbolic"))
                    .on_press(Message::Queue(key, true))
                    .into()
            };
            results = results.add(
                widget::row::with_capacity(3)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(ic("package-x-generic-symbolic", 28))
                    .push(
                        widget::column::with_capacity(2)
                            .spacing(2)
                            .push(single_line(widget::text::body(name.clone())))
                            .push(single_line(widget::text::caption(reference.clone())))
                            .width(Length::Fill),
                    )
                    .push(control),
            );
        }
        let status = if self.searching {
            Some("Searching… Large remotes can take a moment.".to_string())
        } else if !self.searched {
            Some(
                "Enter a name or application ID and press Search. An empty search lists every app."
                    .to_string(),
            )
        } else if self.results.is_empty() {
            Some("No packages match this search.".to_string())
        } else if self.results.len() > LIMIT {
            Some(format!(
                "Showing the first {LIMIT} of {} results. Refine the search to see more.",
                self.results.len()
            ))
        } else {
            Some(match self.results.len() {
                1 => "1 result".to_string(),
                n => format!("{n} results"),
            })
        };
        let mut col = widget::column::with_capacity(5)
            .spacing(16)
            .push(header)
            .push(search);
        if let Some(status) = status {
            col = col.push(widget::text::caption(status));
        }
        col = col.push(
            widget::scrollable(widget::container(results).padding([0, 12, 0, 0]))
                .spacing(8)
                .height(Length::Fill),
        );
        if !self.queue.is_empty() {
            col = col.push(panel(
                widget::row::with_capacity(3)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(
                        widget::text(format!(
                            "{} pending package{}",
                            self.queue.len(),
                            if self.queue.len() == 1 { "" } else { "s" }
                        ))
                        .width(Length::Fill),
                    )
                    .push(button("Review Pending", Message::InstallBrowse(None)))
                    .push(
                        widget::button::suggested(format!("Install {}", self.queue.len()))
                            .on_press(Message::InstallQueue),
                    ),
            ));
        }
        widget::container(col)
            .max_width(1040)
            .width(Length::Fill)
            .into()
    }
    fn toolbar(&self) -> Option<Element<'_, Message>> {
        if matches!(self.page, Page::Install | Page::Remotes)
            || (self.page == Page::Snapshots && self.inventory.snapshots.is_empty())
        {
            return None;
        }
        let mut tools = widget::row::with_capacity(8)
            .spacing(8)
            .align_y(Alignment::Center)
            .push(
                widget::search_input(
                    match self.page {
                        Page::Packages => "Search packages",
                        Page::Data => "Search app data",
                        _ => "Search snapshots",
                    },
                    &self.query,
                )
                .id(self.search_id.clone())
                .on_input(Message::Search),
            );
        if self.page == Page::Packages {
            let active = !self.view.filters_are_default();
            tools = tools.push(
                widget::button::icon(icon::from_name("view-filter-symbolic"))
                    .id(widget::Id::new("packages.filters"))
                    .name(if active {
                        "Filters (active)"
                    } else {
                        "Filters"
                    })
                    .selected(self.filters || active)
                    .on_press(Message::Filters)
                    .tooltip(if active {
                        "Filters (active)"
                    } else {
                        "Filters"
                    }),
            );
        }
        tools = tools.push(
            widget::button::standard(if self.batch { "Done" } else { "Select" })
                .id(widget::Id::new("packages.select"))
                .on_press(Message::Batch(!self.batch)),
        );
        if self.page == Page::Snapshots {
            for action in self.snapshot_secondary_actions() {
                tools = tools.push(action);
            }
            tools = tools.push(
                widget::button::suggested("New Snapshot")
                    .id(widget::Id::new("snapshots.new"))
                    .on_press(Message::NewSnapshot),
            );
        }
        Some(tools.into())
    }
    fn progress_view(&self, job: &Job) -> Element<'_, Message> {
        let (completed, total, lines) = job
            .progress
            .lock()
            .map(|p| (p.completed, p.total, p.lines.clone()))
            .unwrap_or_default();
        let (status, percent) = progress_status(&lines, |id| self.package_name(id));
        let cancelling = job.cancel.load(Ordering::Relaxed);
        let icon: Element<'_, Message> = match &self.job_view.icon {
            Some(id) => package_icon(id, 72).into(),
            None => ic("folder-download-symbolic", 72).into(),
        };
        // A step without a reported percentage animates instead of sitting at
        // an empty bar until it suddenly finishes.
        let bar: Element<'_, Message> = match (percent, total) {
            (Some(p), t) => {
                widget::progress_bar::determinate_linear((completed as f32 + p) / t.max(1) as f32)
                    .width(Length::Fill)
                    .girth(8)
                    .into()
            }
            (None, t) if t > 1 && completed > 0 => {
                widget::progress_bar::determinate_linear(completed as f32 / t as f32)
                    .width(Length::Fill)
                    .girth(8)
                    .into()
            }
            _ => widget::progress_bar::indeterminate_linear()
                .width(Length::Fill)
                .girth(8)
                .into(),
        };
        let elapsed = self.job_started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let title = self.job_view.working.trim_end_matches('…');
        let status = status.filter(|s| s != title);
        let mut col = widget::column::with_capacity(8)
            .spacing(16)
            .align_x(Alignment::Center)
            .max_width(560)
            .push(icon)
            .push(widget::text::title2(self.job_view.working.clone()))
            .push(
                widget::text(if cancelling {
                    "Cancelling… Steps already finished stay done.".to_string()
                } else {
                    status.unwrap_or_else(|| {
                        if lines.iter().all(|l| l.starts_with("$ ")) {
                            "Starting…".into()
                        } else {
                            "Working…".into()
                        }
                    })
                })
                .align_x(iced::advanced::text::Alignment::Center),
            )
            .push(widget::container(bar).width(Length::Fill).padding([4, 0]))
            .push(widget::text::caption(match total {
                t if t > 1 => format!(
                    "Step {} of {t} · {}",
                    (completed + 1).min(t),
                    elapsed_text(elapsed)
                ),
                _ => elapsed_text(elapsed),
            }));
        col = col
            .push(widget::text::caption(
                "You can keep using your computer. Changes appear here when this finishes.",
            ))
            .push(
                widget::row::with_capacity(2)
                    .spacing(8)
                    .push(
                        widget::button::text(if self.show_output {
                            "Hide details"
                        } else {
                            "Show details"
                        })
                        .trailing_icon(icon::from_name(if self.show_output {
                            "go-up-symbolic"
                        } else {
                            "go-down-symbolic"
                        }))
                        .id(widget::Id::new("operation.details"))
                        .on_press(Message::ShowOutput(!self.show_output)),
                    )
                    .push(
                        widget::button::standard("Cancel")
                            .id(widget::Id::new("operation.cancel"))
                            .on_press_maybe((!cancelling).then_some(Message::Cancel)),
                    ),
            );
        if self.show_output {
            let tail = lines
                .iter()
                .rev()
                .take(60)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            col = col.push(
                widget::container(
                    widget::scrollable(
                        widget::container(widget::text::monotext(tail).size(12))
                            .padding(12)
                            .width(Length::Fill),
                    )
                    .anchor_bottom()
                    .height(Length::Fixed(220.)),
                )
                .class(cosmic::theme::Container::Card)
                .width(Length::Fill),
            );
        }
        widget::container(col)
            .center(Length::Fill)
            .padding(24)
            .into()
    }
    fn outcome_panel<'a>(&self, view: &'a JobView, outcome: &'a Outcome) -> Element<'a, Message> {
        let title = if outcome.cancelled {
            view.cancelled.clone()
        } else {
            view.failed.clone()
        };
        let reason = outcome
            .errors
            .iter()
            .find(|e| !e.contains(" exited with "))
            .or(outcome.errors.first())
            .map(|e| plain_error(e, &outcome.output))
            .unwrap_or_default();
        let summary = if outcome.cancelled {
            format!(
                "{} of the steps finished before cancelling and stay done.",
                outcome.steps.len()
            )
        } else {
            reason
        };
        panel(
            widget::row::with_capacity(4)
                .spacing(12)
                .align_y(Alignment::Center)
                .push(ic(
                    if outcome.cancelled {
                        "dialog-information-symbolic"
                    } else {
                        "dialog-warning-symbolic"
                    },
                    24,
                ))
                .push(
                    widget::column::with_capacity(2)
                        .spacing(2)
                        .push(widget::text::heading(title))
                        .push(
                            widget::text(summary)
                                .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                        )
                        .width(Length::Fill),
                )
                .push(
                    widget::button::standard("Details")
                        .id(widget::Id::new("result.details"))
                        .on_press(Message::ShowReport),
                )
                .push(
                    widget::button::icon(icon::from_name("window-close-symbolic"))
                        .id(widget::Id::new("result.dismiss"))
                        .tooltip("Dismiss")
                        .on_press(Message::ClearOutcome),
                ),
        )
    }
    fn drop_banner(&self) -> Element<'_, Message> {
        widget::container(
            widget::row::with_capacity(2)
                .spacing(16)
                .align_y(Alignment::Center)
                .push(ic("folder-download-symbolic", 40))
                .push(
                    widget::column::with_capacity(2)
                        .spacing(4)
                        .push(widget::text::title4("Drop to Open"))
                        .push(widget::text::caption(
                            ".flatpak and .flatpakref files open install review · .flatpakrepo adds a remote · .tar.zst imports a snapshot. Nothing is changed until you confirm.",
                        )),
                ),
        )
        .padding(20)
        .width(Length::Fill)
        .class(cosmic::theme::Container::Primary)
        .into()
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
        let view = flags.view_state.clone();
        let capture_instance = flags.capture.is_some();
        let mut app = Self {
            flags,
            capture_ticks: 0,
            search_id: widget::Id::unique(),
            remote_choice: 0,
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
            view,
            restored_page: false,
            leftovers: false,
            detail: String::new(),
            detail_loading: false,
            history: None,
            dialog: None,
            show_commands: false,
            notice: preferences_error,
            job: None,
            job_view: JobView::default(),
            show_output: false,
            job_started: None,
            outcome: None,
            uninstall_data: false,
            remote_name: String::new(),
            remote_url: String::new(),
            show_disabled_remotes: false,
            remote_menu: None,
            install_query: String::new(),
            install_scope: 0,
            install_target: None,
            results: vec![],
            queue: BTreeMap::new(),
            searching: false,
            searched: false,
            snapshot_name: String::new(),
            snapshot_pick: BTreeSet::new(),
            snapshot_query: String::new(),
            import_id: String::new(),
            import_path: None,
            commit: String::new(),
            hold: true,
            drag_over: false,
            file_sizes: HashMap::new(),
            toasts: widget::Toasts::new(Message::CloseToast),
            last_toast: None,
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
        // Radio rows keep each choice's selection and hover inside its own row.
        let selected = Some(self.preferences.theme.index());
        let appearance = widget::settings::section()
            .title("Appearance")
            .add(
                widget::settings::item::builder("Match desktop")
                    .description("Follow the COSMIC light or dark setting")
                    .radio(0, selected, Message::Theme),
            )
            .add(widget::settings::item::builder("Dark").radio(1, selected, Message::Theme))
            .add(widget::settings::item::builder("Light").radio(2, selected, Message::Theme));
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
            tasks.push(self.update(Message::FilesPicked(Ok(files))));
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
            // The last cursor position the window saw, on any desktop.
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
        self.drag_over = false;
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
        } else if self.page == Page::Install && self.install_target.is_some() {
            self.install_target = None;
        } else if self.filters && self.query.is_empty() {
            self.filters = false;
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
        let mut col = widget::column::with_capacity(6).spacing(16);
        if let Some(job) = &self.job {
            return self.progress_view(job);
        }
        if let Some((view, outcome)) = &self.outcome
            && !outcome.ok
        {
            col = col.push(self.outcome_panel(view, outcome));
        }
        if self.drag_over {
            col = col.push(self.drop_banner());
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
                            .id(widget::Id::new("notice.dismiss"))
                            .tooltip("Dismiss report")
                            .on_press(Message::ClearNotice),
                    ),
            ));
        }
        let content: Element<'_, Message> = if self.loading {
            col.push(self.empty(
                "Loading Flatpaks…",
                "Reading packages, remotes, user data and snapshots.",
                "view-refresh-symbolic",
            ))
            .into()
        } else {
            if let Some(tools) = self.toolbar() {
                col = col.push(tools);
            }
            col.push(match self.page {
                Page::Packages => self.packages_view(),
                Page::Remotes => self.remotes_view(),
                Page::Data => self.data_view(),
                Page::Snapshots => self.snapshots_view(),
                Page::Install => self.install_view(),
            })
            .into()
        };
        let content = widget::toaster(
            &self.toasts,
            widget::container(content)
                .id(widget::Id::new("content"))
                .padding(20)
                .height(Length::Fill),
        );
        // File managers offer text/uri-list; sandboxed senders use the document
        // portal's file-transfer key, which is resolved to readable paths.
        widget::dnd_destination(content, vec!["text/uri-list".into()])
            .on_enter(|_, _, _| Message::DragOver(true))
            .on_leave(|| Message::DragOver(false))
            .on_finish(|mime, data, _, _, _| Message::Dropped(mime, data))
            .on_file_transfer(Message::FileTransfer)
            .into()
    }
    fn dialog(&self) -> Option<Element<'_, Message>> {
        let dialog = self.dialog.as_ref()?;
        let cancel = || {
            widget::button::standard("Cancel")
                .id(widget::Id::new("dialog.cancel"))
                .on_press(Message::CloseDialog)
        };
        let base = widget::dialog().secondary_action(cancel());
        Some(match dialog {
            Dialog::Source(p) => {
                let remotes: Vec<_> = self
                    .inventory
                    .remotes
                    .iter()
                    .filter(|r| r.installation == p.installation && !r.disabled())
                    .collect();
                let mut choices = widget::list_column().list_item_padding([8, 12]);
                for (index, r) in remotes.iter().enumerate() {
                    choices = choices.add(
                        widget::settings::item::builder(format!(
                            "{}{}",
                            remotes::title(r),
                            if r.name == p.origin { " (current)" } else { "" }
                        ))
                        .description(r.name.clone())
                        .radio(index, Some(self.remote_choice), Message::RemoteChoice),
                    );
                }
                let op = remotes.get(self.remote_choice).map(|r| Operation::Reinstall {
                    package: p.clone(),
                    remote: (*r).clone(),
                });
                base.title(format!("Change Source for {}", p.name))
                    .body(format!(
                        "Reinstall from another remote in the {} installation. App data is kept.",
                        p.installation
                    ))
                    .control(widget::container(widget::scrollable(choices).height(Length::Shrink)).max_height(280))
                    .primary_action(
                        widget::button::suggested("Review")
                            .id(widget::Id::new("dialog.review"))
                            .on_press_maybe(op.map(Message::Ask)),
                    )
                    .into()
            }
            Dialog::Updates(p, result) => {
                let body = match result {
                    Ok(s) => format!(
                        "{}\nSource: {}\nInstalled commit: {}\nAvailable commit: {}{}",
                        if s.update_available {
                            "An update is available"
                        } else {
                            "Up to date"
                        },
                        s.origin,
                        s.installed_commit,
                        s.available_commit,
                        if s.masked {
                            "\nUpdates are blocked. Unblock this package before updating."
                        } else {
                            ""
                        }
                    ),
                    Err(e) => e.clone(),
                };
                base.title(format!("Updates for {}", p.name))
                    .body(body)
                    .primary_action(
                        widget::button::suggested("Update")
                            .id(widget::Id::new("dialog.review"))
                            .on_press_maybe(
                                result
                                    .as_ref()
                                    .ok()
                                    .filter(|s| s.update_available && !s.masked)
                                    .map(|_| Message::Ask(Operation::Update(vec![p.clone()]))),
                            ),
                    )
                    .into()
            }
            Dialog::InstallIds(ids) => {
                let remotes = self.enabled_remotes();
                let mut choices = widget::list_column().list_item_padding([8, 12]);
                for (index, r) in remotes.iter().enumerate() {
                    choices = choices.add(
                        widget::settings::item::builder(remotes::title(r).to_string())
                            .description(format!("Installation: {} · {}", r.installation, r.name))
                            .radio(index, Some(self.remote_choice), Message::RemoteChoice),
                    );
                }
                let operation = remotes.get(self.remote_choice).map(|r| Operation::Install {
                    installation: r.installation.clone(),
                    remote: r.name.clone(),
                    refs: ids.clone(),
                });
                base.title("Install Apps from Snapshots")
                    .body(format!("Choose where to install:\n{}", ids.join("\n")))
                    .control(widget::container(widget::scrollable(choices).height(Length::Shrink)).max_height(280))
                    .primary_action(
                        widget::button::suggested("Review")
                            .id(widget::Id::new("dialog.review"))
                            .on_press_maybe(operation.map(Message::Ask)),
                    )
                    .into()
            }
            Dialog::Uninstall(packages) => {
                let apps = packages.iter().filter(|p| !p.is_runtime()).count();
                let title = match packages.as_slice() {
                    [p] => format!("Uninstall {}?", p.name),
                    _ => format!("Uninstall {} Packages?", packages.len()),
                };
                let body = match packages.as_slice() {
                    [p] if p.is_runtime() => format!(
                        "{} will be removed from the {} installation. Apps that use this runtime may stop working until it is reinstalled.",
                        p.name, p.installation
                    ),
                    [p] => format!(
                        "{} will be removed from the {} installation. You can reinstall it later.",
                        p.name, p.installation
                    ),
                    _ => format!(
                        "These packages will be removed:\n{}",
                        packages
                            .iter()
                            .map(|p| format!("{} ({})", p.name, p.installation))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ),
                };
                let body = if packages.iter().any(|p| p.id == Self::APP_ID) {
                    format!(
                        "{body}\n\nThis includes Warehouse COSMIC itself. This window keeps working until you close it, but the app won't open again until you reinstall it."
                    )
                } else {
                    body
                };
                let mut d = base
                    .title(title)
                    .body(body)
                    .width(Length::Fixed(480.))
                    .primary_action(
                        widget::button::destructive("Uninstall")
                            .id(widget::Id::new("dialog.confirm"))
                            .on_press(Message::Confirm),
                    );
                if apps > 0 {
                    let selected = Some(usize::from(self.uninstall_data));
                    d = d
                        .control(widget::text::heading("App Settings & Content"))
                        .control(
                            widget::list_column()
                                .list_item_padding([8, 12])
                                .add(
                                    widget::settings::item::builder("Keep")
                                        .description("Keep settings and content so they return if you reinstall")
                                        .radio(0, selected, |_| Message::UninstallData(false)),
                                )
                                .add(
                                    widget::settings::item::builder("Trash")
                                        .description(if apps == 1 {
                                            "Move settings and content to the desktop Trash. They are shared by every installation and branch of this app."
                                        } else {
                                            "Move each app's settings and content to the desktop Trash. Data is shared by every installation and branch of an app."
                                        })
                                        .radio(1, selected, |_| Message::UninstallData(true)),
                                ),
                        );
                }
                d.into()
            }
            Dialog::Confirm(op) => {
                let explanation = op.explanation();
                let mut d = base
                    .title(format!("{}?", op.title()))
                    .body(explanation.trim().to_string())
                    .width(Length::Fixed(520.))
                    .control(
                        widget::button::text(if self.show_commands {
                            "Hide command details"
                        } else {
                            "Show command details"
                        })
                        .trailing_icon(icon::from_name(if self.show_commands {
                            "go-up-symbolic"
                        } else {
                            "go-down-symbolic"
                        }))
                        .id(widget::Id::new("dialog.details"))
                        .on_press(Message::ShowCommands(!self.show_commands)),
                    );
                if self.show_commands {
                    let lines = op.command_lines();
                    d = d.control(
                        widget::container(
                            widget::scrollable(widget::text::caption(if lines.is_empty() {
                                "This change is made by Warehouse COSMIC directly, without a Flatpak command.".to_string()
                            } else {
                                lines
                            }))
                            .height(Length::Shrink),
                        )
                        .max_height(200),
                    );
                }
                let primary = if op.destructive() {
                    widget::button::destructive(op.verb())
                } else {
                    widget::button::suggested(op.verb())
                };
                d.primary_action(
                    primary
                        .id(widget::Id::new("dialog.confirm"))
                        .on_press(Message::Confirm),
                )
                .into()
            }
            Dialog::InstallFiles(files) => {
                let mut packages = widget::list_column().list_item_padding([10, 12]);
                for path in files {
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let kind = if path.extension().is_some_and(|ext| ext == "flatpakref") {
                        "Flatpak reference"
                    } else {
                        "Flatpak bundle"
                    };
                    let size = self
                        .file_sizes
                        .get(path)
                        .copied()
                        .flatten()
                        .map(backend::size)
                        .unwrap_or_else(|| "Not readable".into());
                    packages = packages.add(
                        widget::row::with_capacity(2)
                            .spacing(12)
                            .align_y(Alignment::Center)
                            .push(ic("package-x-generic-symbolic", 24))
                            .push(
                                widget::column::with_capacity(2)
                                    .spacing(3)
                                    .width(Length::Fill)
                                    .push(
                                        widget::text(filename)
                                            .width(Length::Fill)
                                            .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                                    )
                                    .push(widget::text::caption(format!("{kind} · {size}"))),
                            ),
                    );
                }
                base.title(if files.len() == 1 {
                    "Install a Package"
                } else {
                    "Install Packages"
                })
                .width(Length::Fixed(480.))
                .control(widget::text::heading("Review selection"))
                .control(
                    widget::container(
                        widget::scrollable(packages)
                            .height(Length::Shrink)
                            .spacing(8),
                    )
                    .max_height(200),
                )
                .control(widget::text::heading("Choose installation"))
                .control(self.installation_choices())
                .primary_action(
                    widget::button::suggested("Install")
                        .id(widget::Id::new("install.confirm"))
                        .on_press_maybe(
                            (!files.is_empty()).then_some(Message::InstallFilesConfirmed),
                        ),
                )
                .into()
            }
            Dialog::Remote => base
                .title("Add Remote")
                .body("Enter a name and a repository URL, or choose a .flatpakrepo file.")
                .width(Length::Fixed(480.))
                .control(
                    widget::text_input("Remote name", &self.remote_name)
                        .on_input(|v| Message::Field(Field::RemoteName, v)),
                )
                .control(
                    widget::text_input(
                        "https://… or /path/to/remote.flatpakrepo",
                        &self.remote_url,
                    )
                    .on_input(|v| Message::Field(Field::RemoteUrl, v)),
                )
                .control(button("Choose File", Message::ChooseFile(FilePurpose::Remote)))
                .control(widget::text::heading("Installation"))
                .control(self.installation_choices())
                .primary_action(
                    widget::button::suggested("Review")
                        .id(widget::Id::new("dialog.review"))
                        .on_press(Message::Ask(Operation::RemoteAdd {
                            installation: self
                                .inventory
                                .installations
                                .get(self.install_scope)
                                .cloned()
                                .unwrap_or_else(|| "user".into()),
                            name: self.remote_name.trim().into(),
                            location: self.remote_url.trim().into(),
                        })),
                )
                .into(),
            Dialog::RemoteDetails(r) => base
                .title(remotes::title(r).to_string())
                .control(info_row("Name", &r.name))
                .control(info_row("Installation", &r.installation))
                .control(info_row(
                    "Status",
                    if r.disabled() { "Disabled" } else { "Enabled" },
                ))
                .control(info_row(
                    "URL",
                    if r.url.is_empty() {
                        "Local bundle origin (no repository URL)"
                    } else {
                        &r.url
                    },
                ))
                .secondary_action(button("Close", Message::CloseDialog))
                .into(),
            Dialog::NewSnapshot => {
                let candidates = self.snapshot_candidates();
                let all_visible = !candidates.is_empty()
                    && candidates.iter().all(|d| self.snapshot_pick.contains(&d.id));
                let mut list = widget::list_column().list_item_padding([6, 10]);
                for d in &candidates {
                    let picked = self.snapshot_pick.contains(&d.id);
                    let id = d.id.clone();
                    list = list.add(
                        widget::button::custom(
                            widget::row::with_capacity(4)
                                .spacing(12)
                                .align_y(Alignment::Center)
                                .push(
                                    widget::checkbox(picked)
                                        .on_toggle(move |on| Message::SnapshotPick(id.clone(), on)),
                                )
                                .push(package_icon(&d.id, 32))
                                .push(
                                    widget::column::with_capacity(2)
                                        .spacing(2)
                                        .push(single_line(widget::text::body(self.data_name(d))))
                                        .push(single_line(widget::text::caption(format!(
                                            "{}{}",
                                            d.id,
                                            if d.installed { "" } else { " · Leftover data" }
                                        ))))
                                        .width(Length::Fill),
                                )
                                .push(widget::text::caption(backend::size(d.bytes))),
                        )
                        .padding(4)
                        .width(Length::Fill)
                        .class(package_row_style(picked))
                        .selected(picked)
                        .on_press(Message::SnapshotPick(d.id.clone(), !picked)),
                    );
                }
                if candidates.is_empty() {
                    list = list.add(widget::text(if self.inventory.data.is_empty() {
                        "No app data was found to back up."
                    } else {
                        "No apps match this search."
                    }));
                }
                let ready = !self.snapshot_pick.is_empty() && !self.snapshot_name.trim().is_empty();
                widget::dialog()
                    .title("New Snapshot")
                    .body("Choose the apps to back up. Close them first for a consistent snapshot.")
                    .width(Length::Fixed(560.))
                    .control(
                        widget::text_input("Snapshot name", &self.snapshot_name)
                            .label("Name")
                            .on_input(|v| Message::Field(Field::SnapshotName, v)),
                    )
                    .control(
                        widget::search_input("Search apps", &self.snapshot_query)
                            .on_input(|v| Message::Field(Field::SnapshotSearch, v)),
                    )
                    .control(
                        widget::row::with_capacity(3)
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .push(
                                widget::text::caption(format!(
                                    "{} selected",
                                    self.snapshot_pick.len()
                                ))
                                .width(Length::Fill),
                            )
                            .push(
                                widget::button::text(if all_visible {
                                    "Deselect All"
                                } else {
                                    "Select All"
                                })
                                .id(widget::Id::new("snapshot.select-all"))
                                .on_press_maybe(
                                    (!candidates.is_empty())
                                        .then_some(Message::SnapshotPickAll(!all_visible)),
                                ),
                            ),
                    )
                    .control(
                        widget::container(widget::scrollable(list).height(Length::Shrink).spacing(8))
                            .max_height(220),
                    )
                    .secondary_action(cancel())
                    .primary_action(
                        widget::button::suggested("Create")
                            .id(widget::Id::new("snapshot.create"))
                            .on_press_maybe(ready.then_some(Message::CreateSnapshot)),
                    )
                    .into()
            }
            Dialog::ImportSnapshot => base
                .title("Import Snapshot")
                .body("Import a Warehouse .tar.zst snapshot for an application ID.")
                .width(Length::Fixed(480.))
                .control(
                    widget::text_input("org.example.App", &self.import_id)
                        .label("Application ID")
                        .on_input(|v| Message::Field(Field::ImportId, v)),
                )
                .control(
                    widget::text_input("Snapshot name", &self.snapshot_name)
                        .label("Name")
                        .on_input(|v| Message::Field(Field::SnapshotName, v)),
                )
                .control(
                    widget::row::with_capacity(2)
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .push(button(
                            "Choose File",
                            Message::ChooseFile(FilePurpose::Snapshot),
                        ))
                        .push(
                            widget::text::caption(
                                self.import_path
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| "No file chosen".into()),
                            )
                            .wrapping(iced::advanced::text::Wrapping::WordOrGlyph)
                            .width(Length::Fill),
                        ),
                )
                .primary_action(
                    widget::button::suggested("Review")
                        .id(widget::Id::new("dialog.review"))
                        .on_press_maybe(self.import_path.clone().map(|path| {
                            Message::Ask(Operation::ImportSnapshot {
                                id: self.import_id.trim().into(),
                                path,
                                name: self.snapshot_name.clone(),
                            })
                        })),
                )
                .into(),
            Dialog::Rename(s) => base
                .title("Rename Snapshot")
                .control(
                    widget::text_input("Name", &self.snapshot_name)
                        .on_input(|v| Message::Field(Field::SnapshotName, v)),
                )
                .primary_action(
                    widget::button::suggested("Review")
                        .id(widget::Id::new("dialog.review"))
                        .on_press(Message::Ask(Operation::RenameSnapshot(
                            s.clone(),
                            self.snapshot_name.clone(),
                        ))),
                )
                .into(),
            Dialog::Version(p) => {
                let installed = self
                    .detail
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("Commit:"))
                    .map(str::trim)
                    .unwrap_or_default();
                let choices: Element<'_, Message> = match &self.history {
                    None => widget::text("Loading version history…").into(),
                    Some(Err(error)) => widget::text(error.clone()).into(),
                    Some(Ok(commits)) if commits.is_empty() => {
                        widget::text("This remote did not report any version history.").into()
                    }
                    Some(Ok(commits)) => {
                        let selected = commits.iter().position(|c| c.commit == self.commit.trim());
                        let mut list = widget::list_column().list_item_padding([8, 12]);
                        for (index, c) in commits.iter().enumerate() {
                            let commit = c.commit.clone();
                            list = list.add(
                                widget::settings::item::builder(if c.subject.is_empty() {
                                    format!("Commit {}", &c.commit[..12])
                                } else {
                                    c.subject.clone()
                                })
                                .description(format!(
                                    "{} · {}{}",
                                    if c.date.is_empty() { "Unknown date" } else { &c.date },
                                    &c.commit[..12],
                                    if c.commit == installed { " · Installed" } else { "" }
                                ))
                                .radio(index, selected, move |_| Message::CommitChoice(commit.clone())),
                            );
                        }
                        widget::container(widget::scrollable(list).height(Length::Shrink).spacing(8))
                            .max_height(280)
                            .into()
                    }
                };
                base.title(format!("Change Version of {}", p.name))
                    .body("Choose a version. Available history depends on the remote; some publishers keep only the current version.")
                    .width(Length::Fixed(560.))
                    .control(choices)
                    .control(
                        widget::text_input("Or paste a 64-character commit", &self.commit)
                            .on_input(|v| Message::Field(Field::Commit, v)),
                    )
                    .control(
                        widget::checkbox(self.hold)
                            .label("Block future updates after changing version")
                            .on_toggle(Message::Hold),
                    )
                    .primary_action(
                        widget::button::suggested("Review")
                            .id(widget::Id::new("dialog.review"))
                            .on_press(Message::Ask(Operation::Version(
                                p.clone(),
                                self.commit.trim().into(),
                                self.hold,
                            ))),
                    )
                    .into()
            }
            Dialog::Message(title, body) => widget::dialog()
                .title(title.clone())
                .body(body.clone())
                .primary_action(
                    widget::button::suggested("OK")
                        .id(widget::Id::new("dialog.cancel"))
                        .on_press(Message::CloseDialog),
                )
                .into(),
            Dialog::Report => {
                let Some((view, outcome)) = &self.outcome else {
                    return None;
                };
                let title = if outcome.ok {
                    view.done.clone()
                } else if outcome.cancelled {
                    view.cancelled.clone()
                } else {
                    view.failed.clone()
                };
                let mut body = match (outcome.ok, outcome.steps.len()) {
                    (true, _) => "Everything finished as planned.".to_string(),
                    (false, 0) => "Nothing was changed.".to_string(),
                    (false, 1) => "1 step finished before this and stays done.".to_string(),
                    (false, n) => format!("{n} steps finished before this and stay done."),
                };
                if let Some(error) = outcome.errors.first() {
                    body = format!("{}\n{body}", plain_error(error, &outcome.output));
                }
                let mut d = widget::dialog()
                    .title(title)
                    .body(body)
                    .width(Length::Fixed(560.))
                    .control(
                        widget::button::text(if self.show_commands {
                            "Hide technical details"
                        } else {
                            "Show technical details"
                        })
                        .trailing_icon(icon::from_name(if self.show_commands {
                            "go-up-symbolic"
                        } else {
                            "go-down-symbolic"
                        }))
                        .id(widget::Id::new("dialog.details"))
                        .on_press(Message::ShowCommands(!self.show_commands)),
                    );
                if self.show_commands {
                    let mut parts = Vec::new();
                    if !outcome.steps.is_empty() {
                        parts.push(format!("Finished:\n{}", outcome.steps.join("\n")));
                    }
                    if !outcome.errors.is_empty() {
                        parts.push(format!("Errors:\n{}", outcome.errors.join("\n")));
                    }
                    if !outcome.output.is_empty() {
                        parts.push(format!("Output:\n{}", outcome.output.join("\n")));
                    }
                    if let Some(report) = &outcome.report {
                        parts.push(format!("Report: {}", report.display()));
                    }
                    let text = parts.join("\n\n");
                    d = d.control(
                        widget::container(
                            widget::scrollable(
                                widget::container(widget::text::monotext(text).size(12))
                                    .padding(12)
                                    .width(Length::Fill),
                            )
                            .height(Length::Shrink),
                        )
                        .class(cosmic::theme::Container::Card)
                        .max_height(260),
                    );
                }
                d.secondary_action(
                    widget::button::standard("Open Report Folder")
                        .id(widget::Id::new("report.open-folder"))
                        .on_press_maybe(
                            outcome.report.is_some().then_some(Message::OpenReportFolder),
                        ),
                )
                .primary_action(
                    widget::button::suggested("Close")
                        .id(widget::Id::new("dialog.cancel"))
                        .on_press(Message::CloseDialog),
                )
                .into()
            }
            Dialog::About => widget::dialog()
                .title("Warehouse COSMIC")
                .body("Manage Flatpak apps with a native graphical interface and CLI. Inspired by flattool/Warehouse and built with Rust and libcosmic.\n\nGPL-3.0-or-later · Version 0.1.0")
                .control(widget::text::caption(
                    "Shortcuts: Ctrl+P/M/D/S/I switch pages · Ctrl+F search · Ctrl+T filters · Ctrl+B select · Ctrl+A select all · Delete removes the selection after confirmation · Ctrl+O open files · Ctrl+R or F5 refresh · Ctrl+, settings · Escape goes back or closes",
                ))
                .primary_action(
                    widget::button::suggested("Close")
                        .id(widget::Id::new("dialog.cancel"))
                        .on_press(Message::CloseDialog),
                )
                .into(),
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
                        self.install_scope = self
                            .install_scope
                            .min(self.inventory.installations.len().saturating_sub(1));
                        // Installed or vanished entries leave the pending list.
                        let packages = &self.inventory.packages;
                        let remotes = &self.inventory.remotes;
                        self.queue.retain(|(installation, remote, reference), _| {
                            !packages.iter().any(|p| {
                                &p.installation == installation && &p.full_ref() == reference
                            }) && remotes
                                .iter()
                                .any(|r| &r.installation == installation && &r.name == remote)
                        });
                        if self.install_target.is_some() && self.target_remote().is_none() {
                            self.install_target = None;
                        }
                        self.selection.clear();
                        if !self.inventory.warnings.is_empty() {
                            self.notice = Some(self.inventory.warnings.join("\n"));
                        }
                        // Previews navigate without executing package or data changes.
                        if !self.flags.open_files.is_empty() {
                            self.restored_page = true;
                            let files = std::mem::take(&mut self.flags.open_files);
                            return self.update(Message::FilesPicked(Ok(files)));
                        }
                        if let Some(preview) = self.flags.preview.take() {
                            self.restored_page = true;
                            return self.preview(preview);
                        }
                        if !self.restored_page {
                            self.restored_page = true;
                            if let Some(page) = Page::from_slug(&self.view.page)
                                && page != Page::Packages
                            {
                                return self.switch(page);
                            }
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
                        Ok(()) => {
                            println!(
                                "Saved app render to {} ({}x{})",
                                path.display(),
                                shot.size.width,
                                shot.size.height
                            );
                            // A capture process saves no state. Exiting here
                            // avoids an intermittent SIGSEGV in the Wayland
                            // event loop's teardown seen on COSMIC sessions.
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                            std::process::exit(0);
                        }
                        Err(e) => {
                            eprintln!("Capture failed: {e}");
                            std::process::exit(1);
                        }
                    }
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
                // Text inputs capture Backspace/Delete, so these only reach
                // here while no field is being edited.
                if matches!(key, Key::Named(Named::Delete | Named::Backspace))
                    && self.batch
                    && self.dialog.is_none()
                    && self.job.is_none()
                    && !self.selection.is_empty()
                {
                    return self.update(match self.page {
                        Page::Packages => Message::BatchUninstall,
                        Page::Snapshots => Message::BatchSnapshotTrash,
                        _ => Message::BatchTrash,
                    });
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
                        "b" if matches!(
                            self.page,
                            Page::Packages | Page::Data | Page::Snapshots
                        ) =>
                        {
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
                let lines = text.lines().count();
                let toast = self.toast(if lines > 1 {
                    format!("Copied {lines} lines")
                } else {
                    "Copied to clipboard".to_string()
                });
                return Task::batch([
                    cosmic::iced::clipboard::write(text).map(cosmic::Action::App),
                    toast,
                ]);
            }
            Message::CloseToast(id) => self.toasts.remove(id),
            Message::Page(page) => return self.switch(page),
            Message::Filters => self.filters = !self.filters,
            Message::Applications(v) => self.view.applications = v,
            Message::Runtimes(v) => self.view.runtimes = v,
            Message::Eol(v) => self.view.eol_only = v,
            Message::FilterRemote(key, on) => {
                if on {
                    self.view.remotes.insert(key);
                } else {
                    self.view.remotes.remove(&key);
                }
            }
            Message::FilterRuntime(runtime, on) => {
                if on {
                    self.view.runtime_refs.insert(runtime);
                } else {
                    self.view.runtime_refs.remove(&runtime);
                }
            }
            Message::FilterInstallation(name) => self.view.installation = name,
            Message::ResetFilters => self.view.reset_filters(),
            Message::PackageSort(sort) => self.view.package_sort = sort,
            Message::Leftovers(v) => {
                self.leftovers = v;
                self.selection.clear();
            }
            Message::DataSort(sort) => self.view.data_sort = sort,
            Message::DataDescending(v) => self.view.data_descending = v,
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
                    self.history = None;
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
            Message::HistoryLoaded(k, result) => {
                if let Some(Dialog::Version(p)) = &self.dialog
                    && p.key() == k
                {
                    let origin = p.origin.clone();
                    self.history = Some(result.map(|text| backend::parse_history(&text)).map_err(|error| {
                        let reason = error
                            .lines()
                            .find_map(|line| line.trim().strip_prefix("error:"))
                            .unwrap_or(&error)
                            .trim()
                            .to_string();
                        format!(
                            "No version history is available from {origin}. Packages installed from a bundle file have no remote history.\n{reason}"
                        )
                    }));
                }
            }
            Message::CommitChoice(commit) => self.commit = commit,
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
                if matches!(&self.dialog, Some(Dialog::Updates(previous, _)) if previous.key() == p.key())
                {
                    self.dialog = Some(Dialog::Updates(p, result));
                }
            }
            Message::ChooseSource(p) => {
                self.remote_choice = self
                    .inventory
                    .remotes
                    .iter()
                    .filter(|r| r.installation == p.installation && !r.disabled())
                    .position(|r| r.name == p.origin)
                    .unwrap_or(0);
                self.dialog = Some(Dialog::Source(p));
            }
            Message::RemoteChoice(i) => self.remote_choice = i,
            Message::BatchUpdate => {
                return self.update(Message::Ask(Operation::Update(self.selected_packages())));
            }
            Message::Field(f, text) => match f {
                Field::RemoteName => self.remote_name = text,
                Field::RemoteUrl => self.remote_url = text,
                Field::InstallQuery => self.install_query = text,
                Field::SnapshotName => self.snapshot_name = text,
                Field::SnapshotSearch => self.snapshot_query = text,
                Field::ImportId => self.import_id = text,
                Field::Commit => self.commit = text,
            },
            Message::Hold(v) => self.hold = v,
            Message::Ask(op) => {
                self.remote_menu = None;
                if self.job.is_none() {
                    match op.commands() {
                        Ok(_) => {
                            self.show_commands = false;
                            self.dialog = Some(Dialog::Confirm(op));
                        }
                        Err(e) => self.notice = Some(e),
                    }
                }
            }
            Message::ShowCommands(show) => self.show_commands = show,
            Message::Confirm => match self.dialog.take() {
                Some(Dialog::Confirm(op)) => return self.run(op),
                Some(Dialog::Uninstall(packages)) => {
                    let trash = self.uninstall_data && packages.iter().any(|p| !p.is_runtime());
                    return self.run(Operation::Uninstall(packages, trash));
                }
                other => self.dialog = other,
            },
            Message::CloseDialog => self.dialog = None,
            Message::Finished(outcome) => {
                self.job = None;
                self.dialog = None;
                self.show_output = false;
                let view = self.job_view.clone();
                let toast = if outcome.ok {
                    self.last_toast = Some(view.done.clone());
                    self.toasts
                        .push(
                            widget::Toast::new(view.done.clone())
                                .action("Details".into(), |_| Message::ShowReport),
                        )
                        .map(cosmic::Action::App)
                } else {
                    Task::none()
                };
                self.outcome = Some((view, *outcome));
                return Task::batch([self.refresh(), toast]);
            }
            Message::ShowOutput(show) => self.show_output = show,
            Message::ShowReport => {
                if self.outcome.is_some() {
                    self.show_commands = false;
                    self.dialog = Some(Dialog::Report);
                }
            }
            Message::OpenReportFolder => {
                if let Some(dir) = self
                    .outcome
                    .as_ref()
                    .and_then(|(_, o)| o.report.as_ref())
                    .and_then(|path| path.parent())
                {
                    let result = backend::open(dir);
                    self.set_notice(result);
                }
            }
            Message::ClearOutcome => self.outcome = None,
            Message::Cancel => {
                if let Some(job) = &self.job {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            }
            Message::ClearNotice => self.notice = None,
            Message::Uninstall(packages) => {
                if !packages.is_empty() && self.job.is_none() {
                    self.uninstall_data = false;
                    self.dialog = Some(Dialog::Uninstall(packages));
                }
            }
            Message::BatchUninstall => {
                return self.update(Message::Uninstall(self.selected_packages()));
            }
            Message::BatchTrash => {
                let ids = self.selected_ids();
                if !ids.is_empty() {
                    return self.update(Message::Ask(Operation::TrashData(ids)));
                }
            }
            Message::BatchSnapshot => {
                return self.update(Message::SnapshotFor(self.selected_ids().join(",")));
            }
            Message::NewSnapshot => {
                // From a package or data view, preselect the current app.
                let ids = if self.page == Page::Snapshots {
                    vec![]
                } else {
                    self.selected_ids()
                };
                return self.update(Message::SnapshotFor(ids.join(",")));
            }
            Message::SnapshotFor(ids) => {
                let available: HashSet<_> =
                    self.inventory.data.iter().map(|d| d.id.as_str()).collect();
                self.snapshot_pick = ids
                    .split(',')
                    .map(str::trim)
                    .filter(|id| available.contains(id))
                    .map(str::to_string)
                    .collect();
                self.snapshot_query.clear();
                self.snapshot_name = format!("Backup {}", snapshots::date(backend::now()));
                self.dialog = Some(Dialog::NewSnapshot);
            }
            Message::SnapshotPick(id, on) => {
                if on {
                    self.snapshot_pick.insert(id);
                } else {
                    self.snapshot_pick.remove(&id);
                }
            }
            Message::SnapshotPickAll(on) => {
                let ids: Vec<_> = self
                    .snapshot_candidates()
                    .iter()
                    .map(|d| d.id.clone())
                    .collect();
                for id in ids {
                    if on {
                        self.snapshot_pick.insert(id);
                    } else {
                        self.snapshot_pick.remove(&id);
                    }
                }
            }
            Message::CreateSnapshot => {
                let operation = Operation::Snapshot {
                    ids: self.snapshot_pick.iter().cloned().collect(),
                    name: self.snapshot_name.trim().into(),
                };
                if self.snapshot_name.trim().is_empty() {
                    self.notice = Some("Enter a snapshot name".into());
                } else {
                    match operation.commands() {
                        Ok(_) => {
                            self.dialog = None;
                            return self.run(operation);
                        }
                        Err(error) => self.notice = Some(error),
                    }
                }
            }
            Message::FindPackage(query) => {
                let _ = self.switch(Page::Install);
                self.install_query = query;
                let target = self
                    .enabled_remotes()
                    .into_iter()
                    .min_by_key(|r| (r.name != "flathub", r.installation != "user"))
                    .map(|r| (r.installation.clone(), r.name.clone()));
                if let Some(target) = target {
                    self.install_target = Some(target);
                    self.results.clear();
                    self.searched = false;
                    return self.update(Message::InstallSearch);
                }
                self.install_target = None;
            }
            Message::ViewSnapshots(id) => {
                let task = self.switch(Page::Snapshots);
                if self.inventory.snapshots.iter().any(|s| s.id == id) {
                    self.query = id;
                } else {
                    let name = self.app_name(&id);
                    let toast = self.toast(format!("No snapshots for {name} yet"));
                    return Task::batch([task, toast]);
                }
                return task;
            }
            Message::ShowRuntime(key) => {
                if !self.view.runtimes {
                    self.view.runtimes = true;
                }
                self.query.clear();
                return self.select_package(key);
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
            Message::BatchInstall => {
                let ids = self.selected_ids();
                if !ids.is_empty() {
                    self.remote_choice = self
                        .enabled_remotes()
                        .iter()
                        .position(|r| r.name == "flathub" && r.installation == "user")
                        .unwrap_or(0);
                    self.dialog = Some(Dialog::InstallIds(ids));
                }
            }
            Message::Launch(p) => {
                let name = p.name.clone();
                let result = backend::launch(&p);
                let ok = result.is_ok();
                self.set_notice(result);
                if ok {
                    return self.toast(format!("Opening {name}"));
                }
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
                self.view.reset_filters();
                self.view
                    .remotes
                    .insert(format!("{}:{}", remote.installation, remote.name));
                self.view.runtimes = true;
                self.filters = true;
                self.selected = None;
                self.detail_loading = false;
                if let Some(package) = self.visible_packages().first() {
                    return self.select_package(package.key());
                }
            }
            Message::InstallBrowse(target) => {
                self.remote_menu = None;
                if self.page != Page::Install {
                    let _ = self.switch(Page::Install);
                }
                if target != self.install_target {
                    self.results.clear();
                    self.searched = false;
                }
                self.install_target = target;
                if self.install_target.is_some() {
                    return widget::text_input::focus(self.search_id.clone())
                        .map(cosmic::Action::App);
                }
            }
            Message::InstallSearch => {
                if !self.searching {
                    if let Some(r) = self.target_remote().cloned() {
                        self.searching = true;
                        self.searched = true;
                        let query = self.install_query.clone();
                        let target = (r.installation.clone(), r.name.clone());
                        return background(
                            move || backend::remote_packages(&r, &query),
                            move |result| Message::Results(target, result),
                        );
                    } else {
                        self.notice = Some("Choose an enabled remote first".into());
                    }
                }
            }
            Message::Results(target, r) => {
                self.searching = false;
                if self.install_target.as_ref() == Some(&target) {
                    match r {
                        Ok(results) => self.results = results,
                        Err(e) => self.notice = Some(e),
                    }
                }
            }
            Message::InstallScope(i) => self.install_scope = i,
            Message::Queue(key, on) => {
                if on {
                    let name = self
                        .results
                        .iter()
                        .find(|(_, reference)| *reference == key.2)
                        .map(|(name, _)| name.clone())
                        .unwrap_or_else(|| key.2.clone());
                    self.queue.insert(key, name);
                } else {
                    self.queue.remove(&key);
                }
            }
            Message::ClearQueue => self.queue.clear(),
            Message::InstallQueue => {
                let groups = self.queue_groups();
                if !groups.is_empty() {
                    return self.update(Message::Ask(Operation::InstallQueue(groups)));
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
                Ok(files) => self.open_files(files),
                Err(e) => {
                    self.drag_over = false;
                    if !e.to_lowercase().contains("cancel") {
                        self.notice = Some(e);
                    }
                }
            },
            Message::DragOver(over) => self.drag_over = over,
            Message::Dropped(mime, data) => {
                self.drag_over = false;
                if mime == "text/uri-list" {
                    let files = dropped_paths(&data);
                    if files.is_empty() {
                        self.notice = Some("The dropped item is not a local file.".into());
                    } else {
                        self.open_files(files);
                    }
                }
            }
            Message::FileTransfer(key) => {
                self.drag_over = false;
                return cosmic::command::file_transfer_receive(key).map(|result| {
                    cosmic::Action::App(Message::FilesPicked(
                        result
                            .map(|files| files.into_iter().map(PathBuf::from).collect())
                            .map_err(|e| format!("Cannot receive the dropped files: {e}")),
                    ))
                });
            }
            Message::ImportSnapshot => {
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
impl App {
    /// `--preview PAGE` only navigates and opens review dialogs for captures.
    fn preview(&mut self, preview: String) -> Task<Message> {
        // `PAGE-narrow` starts at the minimum width (see main.rs).
        let preview = match preview.strip_suffix("-narrow") {
            Some(page) if page != "remotes" => page.to_string(),
            _ => preview,
        };
        let page = match preview.as_str() {
            "remotes" | "remote-dialog" | "remotes-disabled" | "remotes-menu"
            | "remotes-details" | "remotes-preset" | "remotes-search" | "remotes-no-results"
            | "remotes-narrow" | "remote-filter" => Page::Remotes,
            "data" | "leftovers" | "data-size" => Page::Data,
            "snapshots" | "new-snapshot" | "import" => Page::Snapshots,
            "install" | "install-search" | "drop" => Page::Install,
            _ => Page::Packages,
        };
        self.leftovers = preview == "leftovers";
        self.filters = preview == "filters";
        let mut task = self.switch(page);
        self.show_disabled_remotes = preview == "remotes-disabled";
        if preview == "data-size" {
            self.view.data_sort = DataSort::Size;
            self.view.data_descending = true;
        }
        if preview == "drop" {
            self.drag_over = true;
        }
        if preview == "remotes-search" {
            self.query = "flathub".into();
        }
        if preview == "remotes-no-results" {
            self.query = "no-such-remote-fixture".into();
        }
        if preview == "install-search"
            && let Some(r) = self.enabled_remotes().first()
        {
            self.install_target = Some((r.installation.clone(), r.name.clone()));
        }
        if matches!(
            preview.as_str(),
            "remotes-menu" | "remotes-details" | "remote-filter"
        ) && let Some(remote) = self
            .inventory
            .remotes
            .iter()
            .find(|r| !r.disabled() && (preview != "remote-filter" || r.name == "parsec-autobuild"))
            .cloned()
        {
            match preview.as_str() {
                "remotes-menu" => self.remote_menu = Some((remote.installation, remote.name)),
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
                return Task::batch([task, self.update(Message::ChooseSource(p))]);
            }
            if preview == "updates" {
                return Task::batch([task, self.update(Message::CheckUpdates(p))]);
            }
        }
        // Display-only samples of the operation screens; no backend work runs.
        if matches!(
            preview.as_str(),
            "progress" | "result-failed" | "result-report"
        ) && let Some(p) = self.package().cloned()
        {
            let op = Operation::Uninstall(vec![p.clone()], false);
            self.job_view = self.job_view(&op);
            let command = format!("$ {}", op.command_lines());
            if preview == "progress" {
                let job = Job::default();
                job.line(command);
                job.line(format!("Uninstalling {}", p.full_ref()));
                self.show_output = true;
                self.job = Some(job);
            } else {
                self.outcome = Some((
                    self.job_view.clone(),
                    Outcome {
                        ok: false,
                        cancelled: false,
                        steps: vec![],
                        errors: vec![format!("{} exited with exit status: 1", op.command_lines())],
                        output: vec![
                            command,
                            format!("error: {} is needed by another installed app", p.id),
                        ],
                        report: Some(self.paths.data.join("operations/sample.json")),
                    },
                ));
                if preview == "result-report" {
                    self.show_commands = true;
                    self.dialog = Some(Dialog::Report);
                }
            }
            return task;
        }
        if preview == "uninstall"
            && let Some(p) = self.package().cloned()
        {
            self.dialog = Some(Dialog::Uninstall(vec![p]));
            return task;
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
                task = Task::batch([task, cosmic::command::set_theme(theme.theme())]);
            }
        }
        match preview.as_str() {
            "about" => self.dialog = Some(Dialog::About),
            "remote-dialog" => self.dialog = Some(Dialog::Remote),
            "new-snapshot" => return Task::batch([task, self.update(Message::NewSnapshot)]),
            "import" => return Task::batch([task, self.update(Message::ImportSnapshot)]),
            _ => (),
        }
        task
    }
}
fn pick_file(purpose: FilePurpose) -> Task<Message> {
    cosmic::task::future(async move {
        use cosmic::dialog::file_chooser::{self, FileFilter};
        if matches!(purpose, FilePurpose::Install) {
            let result = file_chooser::open::Dialog::new()
                .title("Open Flatpak Files")
                .filter(
                    FileFilter::new("Flatpak Packages, Remotes and Snapshots")
                        .glob("*.flatpak")
                        .glob("*.flatpakref")
                        .glob("*.flatpakrepo")
                        .glob("*.tar.zst"),
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
        app.loading = false;
        app
    }
    fn package(id: &str, installation: &str) -> Package {
        Package {
            name: id.rsplit('.').next().unwrap().into(),
            id: id.into(),
            version: "1".into(),
            branch: "stable".into(),
            arch: "x86_64".into(),
            origin: "flathub".into(),
            installation: installation.into(),
            reference: format!("{id}/x86_64/stable"),
            size: String::new(),
            options: "system".into(),
            runtime: String::new(),
            masked: false,
            pinned: false,
            masks: vec![],
            pins: vec![],
        }
    }
    fn flathub(installation: &str) -> Remote {
        Remote {
            name: "flathub".into(),
            title: "Flathub".into(),
            url: "https://dl.flathub.org/repo/".into(),
            options: String::new(),
            installation: installation.into(),
        }
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
        let remote = flathub("user");
        app.inventory.remotes = vec![remote.clone()];
        let package = package("org.example.App", "system");
        app.inventory.packages = vec![
            package.clone(),
            Package {
                installation: "user".into(),
                ..package
            },
        ];
        app.query = "an earlier search".into();
        app.view.eol_only = true;
        app.view.applications = false;
        let _ = app.update(Message::BrowseRemote(remote));
        let visible = app.visible_packages();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].installation, "user");
        assert!(app.filters && app.query.is_empty() && !app.view.eol_only);
        let _ = app.update(Message::ResetFilters);
        assert_eq!(app.visible_packages().len(), 2);
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
    #[test]
    fn dropped_uris_with_spaces_open_review_and_reject_unsupported_or_mixed_files() {
        let mut app = app();
        let _ = app.update(Message::DragOver(true));
        assert!(app.drag_over);
        let _ = app.update(Message::Dropped(
            "text/uri-list".into(),
            b"# comment\r\nfile:///home/user/My%20Apps/Example%20App.flatpak\r\n".to_vec(),
        ));
        assert!(!app.drag_over);
        assert!(matches!(&app.dialog, Some(Dialog::InstallFiles(paths))
            if paths == &[PathBuf::from("/home/user/My Apps/Example App.flatpak")]));
        assert!(app.job.is_none());
        let _ = app.update(Message::CloseDialog);
        let _ = app.update(Message::Dropped(
            "text/uri-list".into(),
            b"file:///tmp/notes.txt\n".to_vec(),
        ));
        assert!(
            matches!(&app.dialog, Some(Dialog::Message(title, _)) if title == "Unsupported File Type")
        );
        let _ = app.update(Message::Dropped(
            "text/uri-list".into(),
            b"file:///tmp/a.flatpak\nfile:///tmp/b.flatpakrepo\n".to_vec(),
        ));
        assert!(
            matches!(&app.dialog, Some(Dialog::Message(title, _)) if title == "Mixed File Types")
        );
        let _ = app.update(Message::Dropped(
            "text/uri-list".into(),
            b"file:///tmp/example.flatpakrepo\n".to_vec(),
        ));
        assert!(matches!(&app.dialog, Some(Dialog::Remote)) && app.remote_name == "example");
        let _ = app.update(Message::Dropped(
            "text/uri-list".into(),
            b"file:///tmp/org.example.App/1700000000_1.0.tar.zst\n".to_vec(),
        ));
        assert!(
            matches!(&app.dialog, Some(Dialog::ImportSnapshot))
                && app.import_id == "org.example.App"
        );
        assert!(app.job.is_none());
    }
    #[test]
    fn uninstall_keeps_data_by_default_and_needs_apply_to_confirm() {
        let mut app = app();
        let p = package("org.example.App", "user");
        app.inventory.packages = vec![p.clone()];
        let _ = app.update(Message::Uninstall(vec![p.clone()]));
        assert!(matches!(&app.dialog, Some(Dialog::Uninstall(list)) if list.len() == 1));
        assert!(!app.uninstall_data);
        assert!(app.ui_activate("uninstall.trash", false, false).is_ok());
        let _ = app.update(Message::UninstallData(true));
        assert!(app.uninstall_data);
        assert!(app.ui_activate("dialog.confirm", false, false).is_err());
        assert!(matches!(
            app.ui_activate("dialog.confirm", true, false),
            Ok(Message::Confirm)
        ));
        let _ = app.update(Message::CloseDialog);
        assert!(app.dialog.is_none() && app.job.is_none());
        let _ = app.update(Message::Uninstall(vec![Package {
            installation: "system".into(),
            ..p
        }]));
        assert!(app.ui_activate("dialog.confirm", true, false).is_err());
        assert!(app.ui_activate("dialog.confirm", true, true).is_ok());
    }
    #[test]
    fn queued_packages_keep_their_remote_and_installation_across_remote_switches() {
        let mut app = app();
        let mut fedora = flathub("system");
        fedora.name = "fedora".into();
        fedora.title = "-".into();
        app.inventory.remotes = vec![flathub("user"), fedora];
        let _ = app.update(Message::Page(Page::Install));
        let _ = app.update(Message::InstallBrowse(Some((
            "user".into(),
            "flathub".into(),
        ))));
        app.results = vec![("Example".into(), "app/org.example.A/x86_64/stable".into())];
        let _ = app.update(Message::Queue(
            (
                "user".into(),
                "flathub".into(),
                "app/org.example.A/x86_64/stable".into(),
            ),
            true,
        ));
        let _ = app.update(Message::InstallBrowse(Some((
            "system".into(),
            "fedora".into(),
        ))));
        assert!(app.results.is_empty());
        let _ = app.update(Message::Queue(
            (
                "system".into(),
                "fedora".into(),
                "app/org.example.B/x86_64/stable".into(),
            ),
            true,
        ));
        assert_eq!(app.queue.len(), 2);
        assert_eq!(
            app.queue[&(
                "user".to_string(),
                "flathub".to_string(),
                "app/org.example.A/x86_64/stable".to_string()
            )],
            "Example"
        );
        let _ = app.update(Message::InstallQueue);
        match &app.dialog {
            Some(Dialog::Confirm(Operation::InstallQueue(groups))) => {
                assert_eq!(groups.len(), 2);
                assert!(
                    groups
                        .iter()
                        .any(|g| g.installation == "system" && g.remote == "fedora")
                );
            }
            other => panic!("unexpected dialog {other:?}"),
        }
        assert!(app.ui_activate("dialog.confirm", true, false).is_err());
        let _ = app.update(Message::CloseDialog);
        let _ = app.update(Message::ClearQueue);
        assert!(app.queue.is_empty() && app.job.is_none());
    }
    #[test]
    fn snapshot_dialog_selects_visible_apps_and_requires_a_name() {
        let mut app = app();
        app.inventory.data = ["org.example.A", "org.example.B", "org.other.C"]
            .iter()
            .map(|id| DataFolder {
                id: (*id).into(),
                bytes: 1,
                installed: true,
                error: None,
            })
            .collect();
        let _ = app.update(Message::Page(Page::Snapshots));
        let _ = app.update(Message::NewSnapshot);
        assert!(matches!(app.dialog, Some(Dialog::NewSnapshot)));
        assert!(app.snapshot_pick.is_empty());
        let _ = app.update(Message::Field(Field::SnapshotSearch, "example".into()));
        let _ = app.update(Message::SnapshotPickAll(true));
        assert_eq!(app.snapshot_pick.len(), 2);
        let _ = app.update(Message::Field(Field::SnapshotName, "  ".into()));
        let _ = app.update(Message::CreateSnapshot);
        assert!(app.job.is_none() && app.notice.is_some());
        assert!(app.ui_activate("snapshot.create", true, false).is_err());
        let _ = app.update(Message::SnapshotFor("org.other.C,org.missing.D".into()));
        assert_eq!(
            app.snapshot_pick.iter().collect::<Vec<_>>(),
            ["org.other.C"]
        );
    }
    #[test]
    fn progress_shows_app_names_and_percentages_instead_of_commands() {
        let name = |id: &str| {
            if id == "com.example.Game" {
                "Game".to_string()
            } else {
                id.to_string()
            }
        };
        let lines = vec![
            "$ flatpak \"uninstall\" \"--user\" \"app/com.example.Game/x86_64/stable\"".to_string(),
            "Uninstalling app/com.example.Game/x86_64/stable".to_string(),
        ];
        assert_eq!(
            progress_status(&lines, name),
            (Some("Uninstalling Game".into()), None)
        );
        let lines = vec![
            "Installing 1/2… \u{2588}\u{2588} 10%\rInstalling 1/2… \u{2588}\u{2588}\u{2588} 45%"
                .to_string(),
        ];
        let (status, percent) = progress_status(&lines, name);
        assert_eq!(status.as_deref(), Some("Installing 1/2…"));
        assert!((percent.unwrap() - 0.45).abs() < 1e-6);
        assert_eq!(
            progress_status(&["$ flatpak \"update\"".to_string()], name),
            (None, None)
        );
        // Output captured from a real non-interactive install of a fixture.
        let lines = [
            "$ flatpak \"install\" \"--user\" \"--noninteractive\" \"-y\" \"--from\" \"/tmp/test.flatpakref\"",
            "F: Flatpakref file does not contain a RuntimeRepo",
            "Installing runtime/org.example.WarehousePlatform/x86_64/stable",
        ]
        .map(String::from);
        assert_eq!(
            progress_status(&lines, name),
            (
                Some("Installing org.example.WarehousePlatform".into()),
                None
            )
        );
    }
    #[test]
    fn finished_operations_report_in_plain_words() {
        let mut app = app();
        let p = package("com.example.Game", "user");
        app.inventory.packages = vec![p.clone()];
        let view = app.job_view(&Operation::Uninstall(vec![p], false));
        assert_eq!(view.working, "Uninstalling Game…");
        assert_eq!(view.done, "Game uninstalled");
        app.job_view = view;
        let _ = app.update(Message::Finished(Box::new(Outcome {
            ok: true,
            cancelled: false,
            steps: vec!["flatpak uninstall".into()],
            errors: vec![],
            output: vec![],
            report: Some("/tmp/report.json".into()),
        })));
        assert_eq!(app.last_toast.as_deref(), Some("Game uninstalled"));
        assert!(app.notice.is_none());
        let _ = app.update(Message::Finished(Box::new(Outcome::failed(
            "flatpak \"uninstall\" exited with exit status: 1".into(),
        ))));
        let failed = &app.outcome.as_ref().unwrap().1;
        assert!(!failed.ok);
        assert_eq!(
            plain_error(
                &failed.errors[0],
                &["error: Game is needed by Other".to_string()]
            ),
            "Game is needed by Other."
        );
        assert!(app.ui_controls().iter().any(|c| c.id == "result.details"));
        let _ = app.update(Message::ShowReport);
        assert!(matches!(app.dialog, Some(Dialog::Report)));
        let _ = app.update(Message::CloseDialog);
        let _ = app.update(Message::ClearOutcome);
        assert!(app.outcome.is_none());
    }
}
