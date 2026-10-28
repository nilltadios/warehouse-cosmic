// SPDX-License-Identifier: GPL-3.0-or-later
//! Keep the last normal size separately from maximized/fullscreen dimensions.
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub mode: Mode,
}
impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1240,
            height: 800,
            mode: Mode::Windowed,
        }
    }
}
impl WindowState {
    pub fn observe(&mut self, mode: Mode, width: f32, height: f32) {
        self.mode = mode;
        if mode == Mode::Windowed
            && width.is_finite()
            && height.is_finite()
            && width >= 940.
            && height >= 560.
        {
            self.width = (width.round() as u32).min(16384);
            self.height = (height.round() as u32).min(16384);
        }
    }
    pub fn size(self) -> cosmic::iced::Size {
        cosmic::iced::Size::new(
            self.width.clamp(940, 16384) as f32,
            self.height.clamp(560, 16384) as f32,
        )
    }
}
pub fn restore<T: Send + 'static>(
    mode: Mode,
    window: cosmic::iced::window::Id,
) -> cosmic::iced::Task<T> {
    use cosmic::iced::{Task, window as command};
    match mode {
        Mode::Windowed => Task::batch([
            command::set_mode(window, command::Mode::Windowed),
            command::maximize(window, false),
        ]),
        Mode::Maximized => Task::batch([
            command::set_mode(window, command::Mode::Windowed),
            command::maximize(window, true),
        ]),
        Mode::Fullscreen => command::set_mode(window, command::Mode::Fullscreen),
    }
}
fn path() -> std::path::PathBuf {
    crate::preferences::directory().join("window-state.json")
}
pub fn load() -> Result<WindowState, String> {
    load_from(&path())
}
fn load_from(path: &Path) -> Result<WindowState, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Cannot read window state {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WindowState::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(state: &WindowState) -> Result<(), String> {
    save_to(&path(), state)
}
fn save_to(path: &Path, state: &WindowState) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    if fs::read(path).is_ok_and(|old| old == data) {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or("Window state has no parent directory")?;
    let existed = parent.exists();
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    if path.exists() {
        fs::copy(path, path.with_extension(format!("json.bak-{stamp}")))
            .map_err(|e| e.to_string())?;
    }
    let temporary = parent.join(format!(".window-{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&data)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reopening_keeps_mode_and_normal_size_across_fullscreen_and_maximize() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("window-state.json");
        let mut state = WindowState::default();
        state.observe(Mode::Windowed, 1100., 700.);
        for mode in [Mode::Maximized, Mode::Fullscreen, Mode::Windowed] {
            state.observe(
                mode,
                if mode == Mode::Windowed { 1100. } else { 1600. },
                if mode == Mode::Windowed { 700. } else { 900. },
            );
            save_to(&path, &state).unwrap();
            let restored = load_from(&path).unwrap();
            assert_eq!(restored.mode, mode);
            assert_eq!((restored.width, restored.height), (1100, 700));
        }
        let before = fs::read_dir(temp.path()).unwrap().count();
        save_to(&path, &state).unwrap();
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), before);
        assert_eq!(
            WindowState {
                width: 0,
                height: u32::MAX,
                ..state
            }
            .size(),
            cosmic::iced::Size::new(940., 16384.)
        );
    }
}
