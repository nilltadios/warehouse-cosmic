// SPDX-License-Identifier: GPL-3.0-or-later
//! GUI automation uses the same messages as visible controls.
use super::*;
use crate::ui_bridge::{Node, Pending, Tree};
use cosmic::Application;
use serde_json::{Value, json};
use warehouse_cosmic::ui_protocol::{self as protocol, Request};

impl App {
    pub(super) fn ui_state(&self) -> Value {
        let modal = self.dialog.as_ref().map(|dialog| match dialog {
            Dialog::InstallFiles(_) => "install-files",
            Dialog::Confirm(_) => "confirmation",
            Dialog::Remote => "add-remote",
            Dialog::RemoteDetails(_) => "remote-details",
            Dialog::About => "about",
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
        json!({"instance":self.ui_instance, "process_id":std::process::id(), "flatpak_instance":flatpak_instance,
            "page":self.page.name(), "theme":self.preferences.theme, "rendered_theme_dark":cosmic::theme::active().theme_type.is_dark(), "settings_open":self.core.window.show_context,
            "modal":modal, "loading":self.loading, "detail_loading":self.detail_loading, "busy":self.job.is_some(), "sidebar_visible":self.core.nav_bar_active(),
            "window":self.window_state, "pointer":self.ui_pointer, "pointer_serial":self.ui_pointer_serial, "search":self.query, "notice":self.notice,
            "show_disabled_remotes":self.show_disabled_remotes, "remote_menu":self.remote_menu,
            "data_filter":if self.leftovers { "leftover" } else { "active" },
            "remotes":self.inventory.remotes.iter().filter(|r| remotes::visible(r, &self.query, self.show_disabled_remotes)).map(|r| json!({"name":r.name,"title":remotes::title(r),"installation":r.installation,"disabled":r.disabled()})).collect::<Vec<_>>(),
            "selected_package":self.selected, "visible_packages":self.visible_packages().iter().map(|p| json!({"id":p.id,"installation":p.installation,"origin":p.origin,"key":p.key()})).collect::<Vec<_>>(),
            "install_files":files, "installation":self.inventory.installations.get(self.install_scope),
            "actions":self.ui_actions()})
    }
    fn ui_actions(&self) -> Vec<String> {
        if self.job.is_some() {
            return vec!["operation.cancel".into()];
        }
        if let Some(dialog) = &self.dialog {
            let mut actions = vec!["dialog.cancel".into(), "escape".into()];
            match dialog {
                Dialog::InstallFiles(_) => {
                    actions.push("install.confirm".into());
                    actions.extend(
                        self.inventory
                            .installations
                            .iter()
                            .map(|name| format!("install.scope:{name}")),
                    );
                }
                Dialog::Confirm(_) => actions.push("dialog.confirm".into()),
                Dialog::Remote => actions.push("dialog.review".into()),
                _ => (),
            }
            return actions;
        }
        let mut actions: Vec<String> = [
            "header.logo",
            "header.sidebar",
            "header.settings",
            "header.about",
            "header.refresh",
            "nav.packages",
            "nav.remotes",
            "nav.data",
            "nav.snapshots",
            "nav.install",
            "escape",
        ]
        .map(str::to_string)
        .into();
        match self.page {
            Page::Remotes => {
                actions.extend(
                    [
                        "remotes.show-disabled",
                        "remotes.add-custom",
                        "remotes.add-file",
                    ]
                    .map(str::to_string),
                );
                actions.extend(
                    remotes::POPULAR
                        .iter()
                        .map(|remote| format!("remotes.preset:{}", remote.name)),
                );
                for r in self
                    .inventory
                    .remotes
                    .iter()
                    .filter(|r| remotes::visible(r, &self.query, self.show_disabled_remotes))
                {
                    for action in ["menu", "filter"] {
                        actions.push(format!("remote:{}:{}:{action}", r.installation, r.name));
                    }
                }
                if self.remote_menu.is_some() {
                    actions.extend(
                        [
                            "remote.menu.details",
                            "remote.menu.copy-name",
                            "remote.menu.copy-url",
                            "remote.menu.toggle",
                            "remote.menu.remove",
                        ]
                        .map(str::to_string),
                    );
                }
            }
            Page::Packages => {
                actions.extend(["packages.filters", "packages.select"].map(str::to_string));
                actions.extend(
                    self.visible_packages()
                        .iter()
                        .map(|p| format!("package:{}", p.key())),
                );
            }
            Page::Install => {
                actions.extend(["install.choose-file", "install.search"].map(str::to_string))
            }
            Page::Data => actions.extend(["data.active", "data.leftover"].map(str::to_string)),
            _ => (),
        }
        actions
    }
    pub(super) fn ui_activate(
        &self,
        target: &str,
        apply: bool,
        allow_system: bool,
    ) -> Result<Message, String> {
        if !self.ui_actions().iter().any(|action| action == target) {
            return Err(format!(
                "Control {target:?} is not available in the current view; inspect actions first"
            ));
        }
        let system = self
            .inventory
            .installations
            .get(self.install_scope)
            .is_some_and(|name| name != "user");
        match target {
            "header.logo" | "header.about" => Ok(Message::About),
            "header.sidebar" => Ok(Message::ToggleSidebar),
            "header.settings" => Ok(Message::Settings),
            "header.refresh" => Ok(Message::Refresh),
            "nav.packages" => Ok(Message::Page(Page::Packages)),
            "nav.remotes" => Ok(Message::Page(Page::Remotes)),
            "nav.data" => Ok(Message::Page(Page::Data)),
            "nav.snapshots" => Ok(Message::Page(Page::Snapshots)),
            "nav.install" => Ok(Message::Page(Page::Install)),
            "data.active" => Ok(Message::Leftovers(false)),
            "data.leftover" => Ok(Message::Leftovers(true)),
            "dialog.cancel" => Ok(Message::CloseDialog),
            "install.confirm" => {
                if !apply {
                    return Err("Installing requires --apply".into());
                }
                if system && !allow_system {
                    return Err(
                        "System installation requires --allow-system as well as --apply".into(),
                    );
                }
                Ok(Message::InstallFilesConfirmed)
            }
            "dialog.confirm" => {
                if !apply {
                    return Err("Confirming a package or data change requires --apply".into());
                }
                if let Some(Dialog::Confirm(operation)) = &self.dialog {
                    let system = operation.commands()?.iter().any(|command| {
                        command
                            .args
                            .iter()
                            .any(|arg| arg == "--system" || arg.starts_with("--installation="))
                    });
                    if system && !allow_system {
                        return Err("System changes also require --allow-system".into());
                    }
                }
                Ok(Message::Confirm)
            }
            "dialog.review" => Ok(Message::Ask(Operation::RemoteAdd {
                installation: self
                    .inventory
                    .installations
                    .get(self.install_scope)
                    .cloned()
                    .unwrap_or_else(|| "user".into()),
                name: self.remote_name.trim().into(),
                location: self.remote_url.trim().into(),
            })),
            "remotes.show-disabled" => {
                Ok(Message::ShowDisabledRemotes(!self.show_disabled_remotes))
            }
            "remotes.add-custom" => Ok(Message::AddRemote),
            "remotes.add-file" => Ok(Message::AddRemoteFile),
            "packages.filters" => Ok(Message::Filters),
            "packages.select" => Ok(Message::Batch(!self.batch)),
            "install.choose-file" => Ok(Message::ChooseFile(FilePurpose::Install)),
            "install.search" => Ok(Message::InstallSearch),
            "operation.cancel" => Ok(Message::Cancel),
            _ => {
                if let Some(name) = target.strip_prefix("install.scope:") {
                    return self
                        .inventory
                        .installations
                        .iter()
                        .position(|item| item == name)
                        .map(Message::InstallScope)
                        .ok_or("Unknown installation".into());
                }
                if let Some(name) = target.strip_prefix("remotes.preset:") {
                    return remotes::POPULAR
                        .iter()
                        .position(|item| item.name == name)
                        .map(Message::PopularRemote)
                        .ok_or("Unknown preset".into());
                }
                if let Some(key) = target.strip_prefix("package:") {
                    return Ok(Message::Select(key.into()));
                }
                if let Some(control) = target.strip_prefix("remote.menu.") {
                    let remote = self
                        .remote_menu
                        .as_ref()
                        .and_then(|(installation, name)| {
                            self.inventory
                                .remotes
                                .iter()
                                .find(|r| &r.installation == installation && &r.name == name)
                        })
                        .cloned()
                        .ok_or("No remote menu is open")?;
                    return match control {
                        "details" => Ok(Message::RemoteDetails(remote)),
                        "copy-name" => Ok(Message::Copy(remote.name)),
                        "copy-url" => Ok(Message::Copy(remote.url)),
                        "toggle" => Ok(Message::Ask(Operation::RemoteEnabled(
                            remote.clone(),
                            remote.disabled(),
                        ))),
                        "remove" => Ok(Message::Ask(Operation::RemoteRemove(remote))),
                        _ => Err("Unknown menu action".into()),
                    };
                }
                if let Some(control) = target.strip_prefix("remote:")
                    && let Some((key, action)) = control.rsplit_once(':')
                    && let Some((installation, name)) = key.split_once(':')
                {
                    let remote = self
                        .inventory
                        .remotes
                        .iter()
                        .find(|r| r.installation == installation && r.name == name)
                        .cloned()
                        .ok_or("Remote no longer exists")?;
                    return match action {
                        "menu" => Ok(Message::RemoteMenu(Some((
                            remote.installation,
                            remote.name,
                        )))),
                        "filter" => Ok(Message::BrowseRemote(remote)),
                        _ => Err("Unknown remote action".into()),
                    };
                }
                Err("Unsupported GUI action".into())
            }
        }
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
                    _ => return failure("Unknown text field".into()),
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
