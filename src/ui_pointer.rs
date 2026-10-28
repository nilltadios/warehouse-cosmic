// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit KDE/Wayland GUI-test input. No input device or service is created at startup.
use crate::{
    host, ui_client,
    ui_protocol::{self as protocol, Request},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
fn input(x: f64, y: f64, scroll: Option<f64>) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() || scroll.is_some_and(|value| !value.is_finite()) {
        return Err("Input coordinates must be finite".into());
    }
    use reis::{
        ei,
        event::{DeviceCapability, EiEvent},
    };
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    let bus = ui_client::connection()?;
    let proxy = ui_client::desktop_proxy(
        &bus,
        "org.kde.KWin",
        "/org/kde/KWin/EIS/RemoteDesktop",
        "org.kde.KWin.EIS.RemoteDesktop",
    )?;
    // KWin uses the portal's capability mask: 2 grants pointer input only.
    let (fd, cookie): (zbus::OwnedFd, i32) = zbus::block_on(proxy.call("connectToEIS", &(2_i32,)))
        .map_err(|e| format!("KDE native test input is unavailable: {e}"))?;
    let stream = UnixStream::from(OwnedFd::from(fd));
    let watchdog = stream.try_clone().map_err(|e| e.to_string())?;
    let (cancel, deadline) = std::sync::mpsc::channel();
    let timer = std::thread::spawn(move || {
        if deadline.recv_timeout(Duration::from_secs(3)).is_err() {
            let _ = watchdog.shutdown(std::net::Shutdown::Both);
        }
    });
    let result = (|| {
        let context = ei::Context::new(stream).map_err(|e| e.to_string())?;
        let (connection, events) = context
            .handshake_blocking(
                "Warehouse COSMIC CLI GUI test",
                ei::handshake::ContextType::Sender,
            )
            .map_err(|e| e.to_string())?;
        for event in events {
            match event.map_err(|e| e.to_string())? {
                EiEvent::SeatAdded(event) => {
                    event.seat.bind_capabilities(
                        DeviceCapability::PointerAbsolute | DeviceCapability::Scroll,
                    );
                    connection.flush().map_err(|e| e.to_string())?;
                }
                EiEvent::DeviceResumed(event)
                    if event
                        .device
                        .has_capability(DeviceCapability::PointerAbsolute) =>
                {
                    let device = event.device.device();
                    device.start_emulating(connection.serial(), 0);
                    let pointer = event
                        .device
                        .interface::<ei::PointerAbsolute>()
                        .ok_or("No absolute pointer interface")?;
                    pointer.motion_absolute(x as f32, y as f32);
                    if let Some(pixels) = scroll {
                        event
                            .device
                            .interface::<ei::Scroll>()
                            .ok_or("No scroll interface")?
                            .scroll(0., pixels as f32);
                    }
                    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
                    let timestamp = now.tv_sec as u64 * 1_000_000 + now.tv_nsec as u64 / 1_000;
                    device.frame(connection.serial(), timestamp);
                    if scroll.is_some() {
                        event
                            .device
                            .interface::<ei::Scroll>()
                            .unwrap()
                            .scroll_stop(0, 1, 0);
                        device.frame(connection.serial(), timestamp + 1);
                    }
                    device.stop_emulating(connection.serial());
                    connection.flush().map_err(|e| e.to_string())?;
                    std::thread::sleep(Duration::from_millis(30));
                    return Ok(());
                }
                _ => (),
            }
        }
        Err("KDE input connection ended before a pointer became available".into())
    })();
    let _ = cancel.send(());
    let _ = timer.join();
    let _: Result<(), _> = zbus::block_on(proxy.call("disconnect", &(cookie,)));
    result
}
fn host_output(program: &str, args: &[&str]) -> Result<String, String> {
    let result = host::command(program, true)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}
fn inspect(connection: &zbus::Connection, instance: &str) -> Result<Value, String> {
    Ok(
        ui_client::call(connection, instance, Request::Inspect, false, false)?["data"]["state"]
            .clone(),
    )
}
fn window(
    connection: &zbus::Connection,
    instance: &str,
) -> Result<(String, BTreeMap<String, String>), String> {
    let state = inspect(connection, instance)?;
    let mut pids = std::collections::BTreeSet::new();
    if let Some(id) = state["flatpak_instance"].as_str() {
        let listing = host_output("flatpak", &["ps", "--columns=instance,pid"])?;
        let pid = listing
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                Some((fields.next()?, fields.next()?))
            })
            .find(|(candidate, _)| *candidate == id)
            .and_then(|(_, pid)| pid.parse::<u32>().ok())
            .ok_or("The GUI Flatpak instance is no longer running")?;
        pids.insert(pid);
        let listing = host_output("ps", &["-eo", "pid,ppid", "--no-headers"])?;
        let pairs: Vec<(u32, u32)> = listing
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
            })
            .collect();
        loop {
            let before = pids.len();
            for (pid, parent) in &pairs {
                if pids.contains(parent) {
                    pids.insert(*pid);
                }
            }
            if before == pids.len() {
                break;
            }
        }
    } else if let Some(pid) = state["process_id"].as_u64() {
        pids.insert(pid as u32);
    }
    for _ in 0..100 {
        let listing: Value = serde_json::from_str(&host_output(
            "busctl",
            &[
                "--user",
                "--json=short",
                "call",
                "org.kde.KWin",
                "/WindowsRunner",
                "org.kde.krunner1",
                "Match",
                "s",
                "Warehouse COSMIC",
            ],
        )?)
        .map_err(|e| e.to_string())?;
        if let Some(entries) = listing["data"][0].as_array() {
            for entry in entries {
                let Some(identity) = entry[0].as_str().and_then(|value| value.strip_prefix("0_"))
                else {
                    continue;
                };
                let text = host_output(
                    "qdbus-qt6",
                    &[
                        "org.kde.KWin",
                        "/KWin",
                        "org.kde.KWin.getWindowInfo",
                        identity,
                    ],
                )?;
                let data: BTreeMap<_, _> = text
                    .lines()
                    .filter_map(|line| {
                        line.split_once(": ")
                            .map(|(key, value)| (key.to_string(), value.to_string()))
                    })
                    .collect();
                if data.get("resourceClass").map(String::as_str)
                    == Some("dev.nilltadios.WarehouseCosmic")
                    && data
                        .get("pid")
                        .and_then(|pid| pid.parse::<u32>().ok())
                        .is_some_and(|pid| pids.contains(&pid))
                {
                    return Ok((identity.to_string(), data));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(80));
    }
    Err("No KWin window belongs to the selected GUI instance".into())
}
fn script(body: &str) -> Result<(), String> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        })
        .join("warehouse-cosmic/ui");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let token = ui_client::new_instance().replace('.', "-");
    let path = cache.join(format!("{token}.js"));
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    host_output(
        "qdbus-qt6",
        &[
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.loadScript",
            &path.to_string_lossy(),
            &token,
        ],
    )?;
    let result = host_output(
        "qdbus-qt6",
        &["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"],
    );
    std::thread::sleep(Duration::from_millis(40));
    let _ = host_output(
        "qdbus-qt6",
        &[
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.unloadScript",
            &token,
        ],
    );
    result.map(|_| ())
}
pub fn place(connection: &zbus::Connection, instance: &str, monitor: &str) -> Result<(), String> {
    let (identity, _) = window(connection, instance)?;
    let monitor = serde_json::to_string(monitor).map_err(|e| e.to_string())?;
    script(&format!(
        "const output = workspace.screens.find(s => s.name === {monitor}); if (output) {{ for (const w of workspace.windowList()) {{ if (String(w.internalId) === {identity:?}) {{ workspace.sendClientToScreen(w, output); workspace.raiseWindow(w); }} }} }}"
    ))
}
/// Present only the window belonging to this exact GUI process on KDE.
/// KWin's default window-runner action also restores a minimized window.
pub fn present(connection: &zbus::Connection, instance: &str) -> Result<(), String> {
    let (identity, _) = window(connection, instance)?;
    let proxy = ui_client::desktop_proxy(
        connection,
        "org.kde.KWin",
        "/WindowsRunner",
        "org.kde.krunner1",
    )?;
    zbus::block_on(proxy.call("Run", &(format!("0_{identity}"), ""))).map_err(|e| e.to_string())
}
pub fn interact(
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
    let bounds = resolved["data"]["bounds"]
        .as_array()
        .ok_or("Widget has no rendered bounds")?;
    let numbers: Vec<f64> = bounds.iter().filter_map(Value::as_f64).collect();
    if numbers.len() != 4 || numbers[2] <= 0. || numbers[3] <= 0. {
        return Err("Widget has no visible area".into());
    }
    let (identity, frame) = window(connection, instance)?;
    let local_x = numbers[0] + numbers[2] / 2.;
    let local_y = numbers[1] + numbers[3] / 2.;
    let width = frame
        .get("width")
        .and_then(|value| value.parse::<f64>().ok())
        .ok_or("Missing window width")?;
    let height = frame
        .get("height")
        .and_then(|value| value.parse::<f64>().ok())
        .ok_or("Missing window height")?;
    if local_x < 0. || local_y < 0. || local_x >= width || local_y >= height {
        return Err("Target is outside the visible window; scroll it into view first".into());
    }
    let x = frame
        .get("x")
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or("KWin did not provide window coordinates")?
        + numbers[0]
        + numbers[2] / 2.;
    let y = frame
        .get("y")
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or("KWin did not provide window coordinates")?
        + numbers[1]
        + numbers[3] / 2.;
    let token = ui_client::new_instance();
    script(&format!(
        "const p = workspace.cursorPos; callDBus({instance:?}, {path:?}, {interface:?}, 'ReportPointer', {token:?}, JSON.stringify([p.x, p.y])); for (const w of workspace.windowList()) {{ if (String(w.internalId) === {identity:?}) workspace.raiseWindow(w); }}",
        path = protocol::PATH,
        interface = protocol::INTERFACE
    ))?;
    let proxy =
        crate::ui_client::desktop_proxy(connection, instance, protocol::PATH, protocol::INTERFACE)
            .map_err(|e| e.to_string())?;
    let mut previous: Vec<f64> = vec![];
    for _ in 0..40 {
        previous = zbus::block_on(proxy.call("PointerPosition", &(token.as_str(),)))
            .map_err(|e| e.to_string())?;
        if previous.len() == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if previous.len() != 2 {
        return Err("KWin did not return the pointer position; no input was sent".into());
    }
    let before_serial = inspect(connection, instance)?["pointer_serial"]
        .as_u64()
        .unwrap_or(0);
    // A small in-widget move makes repeated hovers observable too.
    input(x + (numbers[2] / 4.).min(2.), y, None)?;
    std::thread::sleep(Duration::from_millis(30));
    if let Err(error) = input(x, y, scroll) {
        let _ = input(previous[0], previous[1], None);
        return Err(error);
    }
    let result = (|| {
        let mut state = Value::Null;
        for _ in 0..30 {
            state = inspect(connection, instance)?;
            if let Some(point) = state["pointer"].as_array()
                && point.len() == 2
            {
                let px = point[0].as_f64().unwrap_or(-1.);
                let py = point[1].as_f64().unwrap_or(-1.);
                if state["pointer_serial"].as_u64().unwrap_or(0) > before_serial
                    && px >= numbers[0]
                    && px <= numbers[0] + numbers[2]
                    && py >= numbers[1]
                    && py <= numbers[1] + numbers[3]
                {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        let point = state["pointer"]
            .as_array()
            .ok_or("Native pointer input did not reach the GUI")?;
        let px = point[0].as_f64().unwrap_or(-1.);
        let py = point[1].as_f64().unwrap_or(-1.);
        if state["pointer_serial"].as_u64().unwrap_or(0) <= before_serial
            || px < numbers[0]
            || px > numbers[0] + numbers[2]
            || py < numbers[1]
            || py > numbers[1] + numbers[3]
        {
            return Err("The compositor did not deliver input to the requested widget".into());
        }
        std::thread::sleep(Duration::from_millis(350));
        let ui = ui_client::call(connection, instance, Request::Inspect, false, false)?;
        let image = capture
            .as_ref()
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
            json!({"instance":instance, "target":target, "native_input_delivered":true, "pointer":point, "ui":ui["data"], "capture":image, "restored_pointer":capture.is_some()}),
        ))
    })();
    if capture.is_some() || result.is_err() {
        let _ = input(previous[0], previous[1], None);
    }
    result
}
