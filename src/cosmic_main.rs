// SPDX-License-Identifier: GPL-3.0-or-later
//! Repo-only COSMIC test helpers. Window activation and placement use COSMIC's
//! toplevel protocols; real pointer input uses the standard RemoteDesktop
//! portal. They are kept out of the app and its CLI, which behave the same on
//! every desktop. Build with
//! `cargo build --features cosmic-tools --bin warehouse-cosmic-cosmic`.
use clap::{Parser, Subcommand};
use cosmic_protocols::{
    toplevel_info::v1::client::{
        zcosmic_toplevel_handle_v1 as cosmic_toplevel, zcosmic_toplevel_info_v1 as toplevel_info,
    },
    toplevel_management::v1::client::zcosmic_toplevel_manager_v1 as toplevel_manager,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, process::ExitCode, time::Duration};
use warehouse_cosmic::{
    ui_client,
    ui_protocol::{self as protocol, Request},
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry, wl_seat},
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1 as foreign_toplevel,
        ext_foreign_toplevel_list_v1 as toplevel_list,
    },
    workspace::v1::client::{
        ext_workspace_group_handle_v1 as workspace_group, ext_workspace_handle_v1 as workspace,
        ext_workspace_manager_v1 as workspace_manager,
    },
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1 as xdg_output_manager, zxdg_output_v1 as xdg_output,
};
use zbus::{OwnedObjectPath, OwnedVariant as OwnedValue, Variant};

const APP_ID: &str = "dev.nilltadios.WarehouseCosmic";

#[derive(Parser)]
#[command(
    name = "warehouse-cosmic-cosmic",
    about = "COSMIC test helpers for a running Warehouse COSMIC window: activation and monitor placement through COSMIC's toplevel protocols, real pointer input through the RemoteDesktop portal."
)]
struct Cli {
    /// Exact instance from `warehouse-cosmic-cli ui instances`.
    #[arg(long, global = true)]
    instance: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Move the real pointer over a control and confirm the window received it.
    Hover {
        target: String,
        /// Save the hovered render to a new absolute PNG path.
        #[arg(long)]
        capture: Option<PathBuf>,
    },
    /// Scroll with the real pointer over a widget.
    Scroll {
        #[arg(long, default_value = "content")]
        target: String,
        #[arg(long)]
        pixels: f64,
        #[arg(long)]
        capture: Option<PathBuf>,
    },
    /// Move the window to the active workspace of a named monitor, for example DP-1.
    Place { monitor: String },
    /// Activate (raise and focus) the window.
    Present,
    /// List the windows and monitors COSMIC reports, for troubleshooting.
    Windows,
}

// ---------- COSMIC toplevel and workspace state ----------

struct Output {
    proxy: wl_output::WlOutput,
    name: String,
    x: i32,
    y: i32,
    /// Logical size from xdg-output; the portal's stream sizes are physical.
    width: i32,
}
struct Toplevel {
    foreign: foreign_toplevel::ExtForeignToplevelHandleV1,
    cosmic: Option<cosmic_toplevel::ZcosmicToplevelHandleV1>,
    app_id: String,
    closed: bool,
    geometry: Option<(wl_output::WlOutput, i32, i32, i32, i32)>,
}
struct Group {
    proxy: workspace_group::ExtWorkspaceGroupHandleV1,
    outputs: Vec<wl_output::WlOutput>,
    workspaces: Vec<workspace::ExtWorkspaceHandleV1>,
}
#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    toplevels: Vec<Toplevel>,
    groups: Vec<Group>,
    active: Vec<workspace::ExtWorkspaceHandleV1>,
}
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.iter_mut().find(|o| &o.proxy == proxy) else {
            return;
        };
        match event {
            wl_output::Event::Geometry { x, y, .. } => (output.x, output.y) = (x, y),
            wl_output::Event::Name { name } => output.name = name,
            _ => (),
        }
    }
}
impl Dispatch<xdg_output_manager::ZxdgOutputManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &xdg_output_manager::ZxdgOutputManagerV1,
        _: xdg_output_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<xdg_output::ZxdgOutputV1, wl_output::WlOutput> for State {
    fn event(
        state: &mut Self,
        _: &xdg_output::ZxdgOutputV1,
        event: xdg_output::Event,
        output: &wl_output::WlOutput,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.iter_mut().find(|o| &o.proxy == output) else {
            return;
        };
        match event {
            xdg_output::Event::LogicalPosition { x, y } => (output.x, output.y) = (x, y),
            xdg_output::Event::LogicalSize { width, .. } => output.width = width,
            _ => (),
        }
    }
}
impl Dispatch<toplevel_list::ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &toplevel_list::ExtForeignToplevelListV1,
        event: toplevel_list::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let toplevel_list::Event::Toplevel { toplevel } = event {
            state.toplevels.push(Toplevel {
                foreign: toplevel,
                cosmic: None,
                app_id: String::new(),
                closed: false,
                geometry: None,
            });
        }
    }
    event_created_child!(State, toplevel_list::ExtForeignToplevelListV1, [
        toplevel_list::EVT_TOPLEVEL_OPCODE => (foreign_toplevel::ExtForeignToplevelHandleV1, ()),
    ]);
}
impl Dispatch<foreign_toplevel::ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &foreign_toplevel::ExtForeignToplevelHandleV1,
        event: foreign_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(toplevel) = state.toplevels.iter_mut().find(|t| &t.foreign == proxy) else {
            return;
        };
        match event {
            foreign_toplevel::Event::AppId { app_id } => toplevel.app_id = app_id,
            foreign_toplevel::Event::Closed => toplevel.closed = true,
            _ => (),
        }
    }
}
impl Dispatch<toplevel_info::ZcosmicToplevelInfoV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &toplevel_info::ZcosmicToplevelInfoV1,
        _: toplevel_info::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<cosmic_toplevel::ZcosmicToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &cosmic_toplevel::ZcosmicToplevelHandleV1,
        event: cosmic_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let cosmic_toplevel::Event::Geometry {
            output,
            x,
            y,
            width,
            height,
        } = event
            && let Some(toplevel) = state
                .toplevels
                .iter_mut()
                .find(|t| t.cosmic.as_ref() == Some(proxy))
        {
            toplevel.geometry = Some((output, x, y, width, height));
        }
    }
}
impl Dispatch<toplevel_manager::ZcosmicToplevelManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &toplevel_manager::ZcosmicToplevelManagerV1,
        _: toplevel_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<workspace_manager::ExtWorkspaceManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &workspace_manager::ExtWorkspaceManagerV1,
        event: workspace_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let workspace_manager::Event::WorkspaceGroup { workspace_group } = event {
            state.groups.push(Group {
                proxy: workspace_group,
                outputs: vec![],
                workspaces: vec![],
            });
        }
    }
    event_created_child!(State, workspace_manager::ExtWorkspaceManagerV1, [
        workspace_manager::EVT_WORKSPACE_GROUP_OPCODE => (workspace_group::ExtWorkspaceGroupHandleV1, ()),
        workspace_manager::EVT_WORKSPACE_OPCODE => (workspace::ExtWorkspaceHandleV1, ()),
    ]);
}
impl Dispatch<workspace_group::ExtWorkspaceGroupHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &workspace_group::ExtWorkspaceGroupHandleV1,
        event: workspace_group::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(group) = state.groups.iter_mut().find(|g| &g.proxy == proxy) else {
            return;
        };
        match event {
            workspace_group::Event::OutputEnter { output } => group.outputs.push(output),
            workspace_group::Event::WorkspaceEnter { workspace } => {
                group.workspaces.push(workspace)
            }
            _ => (),
        }
    }
}
impl Dispatch<workspace::ExtWorkspaceHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &workspace::ExtWorkspaceHandleV1,
        event: workspace::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let workspace::Event::State { state: flags } = event {
            let active = flags
                .into_result()
                .is_ok_and(|f| f.contains(workspace::State::Active));
            state.active.retain(|w| w != proxy);
            if active {
                state.active.push(proxy.clone());
            }
        }
    }
}

/// The compositor's view of this app's window, plus handles to act on it.
struct Desktop {
    connection: Connection,
    queue: wayland_client::EventQueue<State>,
    state: State,
    seat: wl_seat::WlSeat,
    manager: toplevel_manager::ZcosmicToplevelManagerV1,
}
impl Desktop {
    fn connect() -> Result<Self, String> {
        let connection =
            Connection::connect_to_env().map_err(|e| format!("No Wayland session: {e}"))?;
        let (globals, mut queue) =
            registry_queue_init::<State>(&connection).map_err(|e| e.to_string())?;
        let qh = queue.handle();
        let mut state = State::default();
        for global in globals.contents().clone_list() {
            if global.interface == wl_output::WlOutput::interface().name {
                let proxy: wl_output::WlOutput =
                    globals
                        .registry()
                        .bind(global.name, global.version.min(4), &qh, ());
                state.outputs.push(Output {
                    proxy,
                    name: String::new(),
                    x: 0,
                    y: 0,
                    width: 0,
                });
            }
        }
        let missing = |what: &str| {
            format!("The compositor does not offer {what}; these helpers need COSMIC.")
        };
        let seat: wl_seat::WlSeat = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| missing("wl_seat"))?;
        let _list: toplevel_list::ExtForeignToplevelListV1 = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| missing("ext_foreign_toplevel_list_v1"))?;
        let info: toplevel_info::ZcosmicToplevelInfoV1 = globals
            .bind(&qh, 2..=3, ())
            .map_err(|_| missing("zcosmic_toplevel_info_v1 (version 2+)"))?;
        let manager: toplevel_manager::ZcosmicToplevelManagerV1 = globals
            .bind(&qh, 4..=4, ())
            .map_err(|_| missing("zcosmic_toplevel_manager_v1 (version 4)"))?;
        let xdg_outputs: xdg_output_manager::ZxdgOutputManagerV1 = globals
            .bind(&qh, 2..=3, ())
            .map_err(|_| missing("zxdg_output_manager_v1"))?;
        for output in &state.outputs {
            xdg_outputs.get_xdg_output(&output.proxy, &qh, output.proxy.clone());
        }
        let _workspaces: workspace_manager::ExtWorkspaceManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| missing("ext_workspace_manager_v1"))?;
        for _ in 0..2 {
            queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        }
        for toplevel in &mut state.toplevels {
            toplevel.cosmic = Some(info.get_cosmic_toplevel(&toplevel.foreign, &qh, ()));
        }
        // Handle state can follow shortly after creation; wait briefly for it.
        for _ in 0..10 {
            queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
            if state
                .toplevels
                .iter()
                .all(|t| t.closed || t.geometry.is_some())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(Self {
            connection,
            queue,
            state,
            seat,
            manager,
        })
    }
    /// Connect, waiting up to two seconds for this app's window to appear.
    fn with_window() -> Result<Self, String> {
        let mut last = String::new();
        for _ in 0..20 {
            let desktop = Self::connect()?;
            match desktop.window() {
                Ok(_) => return Ok(desktop),
                Err(error) => last = error,
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(last)
    }
    /// Toplevel identity carries no process ID, so exactly one window of this
    /// app must be open; otherwise the helpers could act on the wrong one.
    fn window(&self) -> Result<&Toplevel, String> {
        let windows: Vec<_> = self
            .state
            .toplevels
            .iter()
            .filter(|t| t.app_id == APP_ID && !t.closed)
            .collect();
        match windows.as_slice() {
            [window] => Ok(window),
            [] => Err("No Warehouse COSMIC window is open".into()),
            list => Err(format!(
                "{} Warehouse COSMIC windows are open; close the others so the helper acts on the right one",
                list.len()
            )),
        }
    }
    fn flush(&mut self) -> Result<(), String> {
        self.connection.flush().map_err(|e| e.to_string())?;
        self.queue
            .roundtrip(&mut self.state)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn present(&mut self) -> Result<(), String> {
        let handle = self.window()?.cosmic.clone().ok_or("No COSMIC handle")?;
        self.manager.activate(&handle, &self.seat);
        self.flush()
    }
    fn place(&mut self, monitor: &str) -> Result<(), String> {
        let handle = self.window()?.cosmic.clone().ok_or("No COSMIC handle")?;
        let output = self
            .state
            .outputs
            .iter()
            .find(|o| o.name == monitor)
            .ok_or_else(|| {
                format!(
                    "No monitor named {monitor}; available: {}",
                    self.state
                        .outputs
                        .iter()
                        .map(|o| o.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?
            .proxy
            .clone();
        let group = self
            .state
            .groups
            .iter()
            .find(|g| g.outputs.contains(&output))
            .ok_or("That monitor has no workspace group")?;
        let target = group
            .workspaces
            .iter()
            .find(|w| self.state.active.contains(w))
            .or(group.workspaces.first())
            .ok_or("That monitor has no workspace")?
            .clone();
        self.manager
            .move_to_ext_workspace(&handle, &target, &output);
        self.manager.activate(&handle, &self.seat);
        self.flush()
    }
    /// Logical widths of the monitors, keyed by their logical top-left corner,
    /// so portal stream sizes (physical pixels) can be converted.
    fn logical_widths(&self) -> Vec<((i32, i32), i32)> {
        self.state
            .outputs
            .iter()
            .map(|o| ((o.x, o.y), o.width))
            .collect()
    }
    /// The window's top-left corner and size in global compositor coordinates.
    fn frame(&self) -> Result<(f64, f64, f64, f64), String> {
        let (output, x, y, width, height) = self
            .window()?
            .geometry
            .clone()
            .ok_or("The compositor did not report the window's position")?;
        let origin = self
            .state
            .outputs
            .iter()
            .find(|o| o.proxy == output)
            .ok_or("The window is on an unknown monitor")?;
        Ok((
            f64::from(origin.x + x),
            f64::from(origin.y + y),
            f64::from(width),
            f64::from(height),
        ))
    }
}

// ---------- RemoteDesktop portal ----------

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

fn token_file() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
        })
        .join("warehouse-cosmic/remote-desktop-token")
}
/// Call a portal method that answers through a Request object's Response signal.
async fn request<B>(
    bus: &zbus::Connection,
    interface: &str,
    method: &str,
    token: &str,
    body: &B,
) -> Result<HashMap<String, OwnedValue>, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let sender = bus
        .unique_name()
        .ok_or("No unique bus name")?
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("{PORTAL_PATH}/request/{sender}/{token}");
    let proxy = zbus::Proxy::new(bus, PORTAL, path, "org.freedesktop.portal.Request")
        .await
        .map_err(|e| e.to_string())?;
    let mut responses = proxy
        .receive_signal("Response")
        .await
        .map_err(|e| e.to_string())?;
    bus.call_method(Some(PORTAL), PORTAL_PATH, Some(interface), method, body)
        .await
        .map_err(|e| format!("{interface}.{method}: {e}"))?;
    let message = responses
        .next()
        .await
        .ok_or("The portal closed the request")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) =
        message.body().deserialize().map_err(|e| e.to_string())?;
    match code {
        0 => Ok(results),
        1 => Err("Remote control was not allowed".into()),
        _ => Err(format!("{method} failed in the portal")),
    }
}
fn token(step: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("warehouse_{step}_{nanos}")
}
/// A shared monitor: its PipeWire node and its area in global coordinates.
struct Stream {
    node: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
/// Physical pixels per logical pixel for the monitor a stream shows.
fn stream_scale(stream: &Stream, widths: &[((i32, i32), i32)]) -> f64 {
    widths
        .iter()
        .find(|((x, y), w)| f64::from(*x) == stream.x && f64::from(*y) == stream.y && *w > 0)
        .map(|(_, w)| stream.width / f64::from(*w))
        .unwrap_or(1.0)
}
/// A started RemoteDesktop session with monitor streams for absolute input.
struct Remote {
    bus: zbus::Connection,
    session: OwnedObjectPath,
    streams: Vec<Stream>,
}
impl Remote {
    async fn start() -> Result<Self, String> {
        let bus = zbus::Connection::session()
            .await
            .map_err(|e| e.to_string())?;
        let create_token = token("create");
        let created = request(
            &bus,
            "org.freedesktop.portal.RemoteDesktop",
            "CreateSession",
            &create_token,
            &(HashMap::from([
                ("handle_token", Variant::from(create_token.clone())),
                ("session_handle_token", Variant::from(token("session"))),
            ]),),
        )
        .await?;
        let session: String = created
            .get("session_handle")
            .and_then(|v| String::try_from(v.clone()).ok())
            .ok_or("The portal returned no session")?;
        let session = OwnedObjectPath::try_from(session).map_err(|e| e.to_string())?;
        let restore = std::fs::read_to_string(token_file()).unwrap_or_default();
        let devices_token = token("devices");
        let mut options = HashMap::from([
            ("handle_token", Variant::from(devices_token.clone())),
            ("types", Variant::from(2_u32)),
            ("persist_mode", Variant::from(2_u32)),
        ]);
        if !restore.trim().is_empty() {
            options.insert("restore_token", Variant::from(restore.trim().to_string()));
        }
        request(
            &bus,
            "org.freedesktop.portal.RemoteDesktop",
            "SelectDevices",
            &devices_token,
            &(&session, options),
        )
        .await?;
        let sources_token = token("sources");
        request(
            &bus,
            "org.freedesktop.portal.ScreenCast",
            "SelectSources",
            &sources_token,
            &(
                &session,
                HashMap::from([
                    ("handle_token", Variant::from(sources_token.clone())),
                    ("types", Variant::from(1_u32)),
                    ("multiple", Variant::from(true)),
                ]),
            ),
        )
        .await?;
        let start_token = token("start");
        let started = request(
            &bus,
            "org.freedesktop.portal.RemoteDesktop",
            "Start",
            &start_token,
            &(
                &session,
                "",
                HashMap::from([("handle_token", Variant::from(start_token.clone()))]),
            ),
        )
        .await?;
        if let Some(token) = started
            .get("restore_token")
            .and_then(|v| String::try_from(v.clone()).ok())
        {
            let path = token_file();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, token);
        }
        let raw: Vec<(u32, HashMap<String, OwnedValue>)> = started
            .get("streams")
            .and_then(|v| Vec::try_from(v.clone()).ok())
            .ok_or("The portal returned no monitor streams")?;
        let streams = raw
            .into_iter()
            .filter_map(|(node, props)| {
                let position = <(i32, i32)>::try_from(props.get("position")?.clone()).ok()?;
                let size = <(i32, i32)>::try_from(props.get("size")?.clone()).ok()?;
                Some(Stream {
                    node,
                    x: f64::from(position.0),
                    y: f64::from(position.1),
                    width: f64::from(size.0),
                    height: f64::from(size.1),
                })
            })
            .collect::<Vec<_>>();
        if streams.is_empty() {
            return Err("The portal reported no monitor positions".into());
        }
        if std::env::var_os("WAREHOUSE_COSMIC_DEBUG").is_some() {
            for s in &streams {
                eprintln!(
                    "stream node {} at {},{} size {}x{}",
                    s.node, s.x, s.y, s.width, s.height
                );
            }
        }
        Ok(Self {
            bus,
            session,
            streams,
        })
    }
    async fn call<B>(&self, method: &str, body: &B) -> Result<(), String>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.bus
            .call_method(
                Some(PORTAL),
                PORTAL_PATH,
                Some("org.freedesktop.portal.RemoteDesktop"),
                method,
                body,
            )
            .await
            .map(|_| ())
            .map_err(|e| format!("{method}: {e}"))
    }
    /// Move to a global logical point. Stream positions are logical, while
    /// stream sizes and pointer coordinates are in the stream's pixels.
    async fn move_to(&self, x: f64, y: f64, widths: &[((i32, i32), i32)]) -> Result<(), String> {
        if std::env::var_os("WAREHOUSE_COSMIC_DEBUG").is_some() {
            eprintln!("move to global {x},{y}");
        }
        let stream = self
            .streams
            .iter()
            .find(|stream| {
                let scale = stream_scale(stream, widths);
                x >= stream.x
                    && y >= stream.y
                    && x < stream.x + stream.width / scale
                    && y < stream.y + stream.height / scale
            })
            .ok_or("The target is not on any shared monitor")?;
        let scale = stream_scale(stream, widths);
        let options: HashMap<&str, Variant> = HashMap::new();
        self.call(
            "NotifyPointerMotionAbsolute",
            &(
                &self.session,
                options,
                stream.node,
                (x - stream.x) * scale,
                (y - stream.y) * scale,
            ),
        )
        .await
    }
    async fn scroll(&self, pixels: f64) -> Result<(), String> {
        self.call(
            "NotifyPointerAxis",
            &(
                &self.session,
                HashMap::from([("finish", Variant::from(true))]),
                0.0_f64,
                pixels,
            ),
        )
        .await
    }
    async fn close(self) {
        let _ = self
            .bus
            .call_method(
                Some(PORTAL),
                &self.session,
                Some("org.freedesktop.portal.Session"),
                "Close",
                &(),
            )
            .await;
    }
}

// ---------- Commands ----------

fn inspect(connection: &zbus::Connection, instance: &str) -> Result<Value, String> {
    Ok(
        ui_client::call(connection, instance, Request::Inspect, false, false)?["data"]["state"]
            .clone(),
    )
}
fn interact(
    connection: &zbus::Connection,
    instance: &str,
    target: &str,
    scroll: Option<f64>,
    capture: Option<PathBuf>,
) -> Result<Value, String> {
    let resolved = ui_client::call(
        connection,
        instance,
        Request::Resolve {
            target: target.into(),
        },
        false,
        false,
    )?;
    let bounds: Vec<f64> = resolved["data"]["bounds"]
        .as_array()
        .ok_or("Widget has no rendered bounds")?
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    if bounds.len() != 4 || bounds[2] <= 0. || bounds[3] <= 0. {
        return Err("Widget has no visible area".into());
    }
    let mut desktop = Desktop::with_window()?;
    desktop.present()?;
    let (wx, wy, width, height) = desktop.frame()?;
    let widths = desktop.logical_widths();
    let (cx, cy) = (bounds[0] + bounds[2] / 2., bounds[1] + bounds[3] / 2.);
    if cx >= width || cy >= height {
        return Err("Target is outside the visible window; scroll it into view first".into());
    }
    let before = inspect(connection, instance)?["pointer_serial"]
        .as_u64()
        .unwrap_or(0);
    zbus::block_on(async {
        let remote = Remote::start().await?;
        let result = async {
            // A small in-widget move makes repeated hovers observable too.
            remote
                .move_to(wx + cx + (bounds[2] / 4.).min(2.), wy + cy, &widths)
                .await?;
            std::thread::sleep(Duration::from_millis(30));
            remote.move_to(wx + cx, wy + cy, &widths).await?;
            if let Some(pixels) = scroll {
                remote.scroll(pixels).await?;
            }
            Ok::<(), String>(())
        }
        .await;
        remote.close().await;
        result
    })?;
    let mut state = Value::Null;
    for _ in 0..30 {
        state = inspect(connection, instance)?;
        if delivered(&state, before, &bounds) {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    if !delivered(&state, before, &bounds) {
        return Err("The compositor did not deliver input to the requested widget".into());
    }
    std::thread::sleep(Duration::from_millis(350));
    let image = capture
        .map(|path| {
            ui_client::call(
                connection,
                instance,
                Request::Capture {
                    path: std::path::absolute(path).map_err(|e| e.to_string())?,
                    overwrite: false,
                },
                false,
                false,
            )
        })
        .transpose()?;
    Ok(protocol::success(
        json!({"instance":instance, "target":target, "native_input_delivered":true, "pointer":state["pointer"], "capture":image}),
    ))
}
/// The window reports the last cursor position it saw; input counts as
/// delivered only when that report is new and inside the requested widget.
fn delivered(state: &Value, before: u64, bounds: &[f64]) -> bool {
    let Some(point) = state["pointer"].as_array().filter(|p| p.len() == 2) else {
        return false;
    };
    let (x, y) = (
        point[0].as_f64().unwrap_or(-1.),
        point[1].as_f64().unwrap_or(-1.),
    );
    state["pointer_serial"].as_u64().unwrap_or(0) > before
        && x >= bounds[0]
        && x <= bounds[0] + bounds[2]
        && y >= bounds[1]
        && y <= bounds[1] + bounds[3]
}
fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = (|| -> Result<Value, String> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if !desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("COSMIC"))
        {
            return Err(format!(
                "These helpers need COSMIC; the current desktop is {}.",
                if desktop.is_empty() {
                    "unknown"
                } else {
                    &desktop
                }
            ));
        }
        if matches!(cli.command, Command::Windows) {
            let desktop = Desktop::connect()?;
            let s = &desktop.state;
            return Ok(protocol::success(json!({
                "monitors": s.outputs.iter().map(|o| json!({"name":o.name,"x":o.x,"y":o.y})).collect::<Vec<_>>(),
                "windows": s.toplevels.iter().filter(|t| !t.closed).map(|t| json!({
                    "app_id":t.app_id,
                    "monitor":t.geometry.as_ref().and_then(|(output, ..)| s.outputs.iter().find(|o| &o.proxy == output)).map(|o| o.name.clone()),
                    "geometry":t.geometry.as_ref().map(|(_, x, y, w, h)| [x, y, w, h]),
                })).collect::<Vec<_>>(),
            })));
        }
        let connection = ui_client::connection()?;
        let instance = ui_client::selected(&connection, cli.instance)?;
        match cli.command {
            Command::Hover { target, capture } => {
                interact(&connection, &instance, &target, None, capture)
            }
            Command::Scroll {
                target,
                pixels,
                capture,
            } => interact(&connection, &instance, &target, Some(pixels), capture),
            Command::Place { monitor } => {
                let mut desktop = Desktop::with_window()?;
                desktop.place(&monitor)?;
                Ok(protocol::success(
                    json!({"instance":instance,"monitor":monitor}),
                ))
            }
            Command::Windows => unreachable!("handled before connecting to the GUI"),
            Command::Present => {
                let mut desktop = Desktop::with_window()?;
                desktop.present()?;
                Ok(protocol::success(json!({"instance":instance})))
            }
        }
    })();
    match result {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", protocol::error(&error));
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
