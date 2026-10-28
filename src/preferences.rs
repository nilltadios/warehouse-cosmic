// SPDX-License-Identifier: GPL-3.0-or-later
//! Desktop preferences. No account or package data is stored here.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    #[default]
    System,
    Dark,
    Light,
}
impl AppTheme {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];
    pub fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }
    pub fn theme(self) -> cosmic::Theme {
        if self == Self::System {
            return cosmic::theme::system_preference();
        }
        let dark = self == Self::Dark;
        let shared = if dark {
            cosmic::theme::system_dark()
        } else {
            cosmic::theme::system_light()
        };
        // An absent COSMIC Light config can deserialize to the default dark
        // palette. Explicit choices must also ignore desktop mode updates.
        let selected = if shared.theme_type.is_dark() == dark {
            shared.cosmic().clone()
        } else {
            let builder = if dark {
                cosmic::cosmic_theme::ThemeBuilder::dark()
            } else {
                cosmic::cosmic_theme::ThemeBuilder::light()
            };
            builder
                .accent(*cosmic::theme::system_preference().cosmic().accent_color())
                .build()
        };
        cosmic::Theme::custom(Arc::new(selected))
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: AppTheme,
}
pub fn directory() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("warehouse-cosmic")
}
fn path() -> PathBuf {
    directory().join("preferences.json")
}
pub fn load() -> Result<Preferences, String> {
    load_from(&path())
}
fn load_from(path: &Path) -> Result<Preferences, String> {
    match fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).map_err(|e| {
            format!(
                "Cannot read settings in {}: {e}. Using the desktop theme.",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}
pub fn save(preferences: &Preferences) -> Result<(), String> {
    save_to(&path(), preferences)
}
fn save_to(path: &Path, preferences: &Preferences) -> Result<(), String> {
    let parent = path.parent().ok_or("Settings path has no parent")?;
    let existed = parent.exists();
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    if path.exists() {
        fs::copy(path, path.with_extension(format!("json.bak-{stamp}")))
            .map_err(|e| e.to_string())?;
    }
    let staging = parent.join(format!(".preferences-{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&staging)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(preferences).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&staging, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(staging);
    }
    result.map_err(|e| format!("Cannot save settings to {}: {e}", path.display()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_modes_keep_their_light_or_dark_palette() {
        let dark = AppTheme::Dark.theme();
        let light = AppTheme::Light.theme();
        assert!(dark.theme_type.is_dark());
        assert!(!light.theme_type.is_dark());
        assert!(matches!(
            dark.theme_type,
            cosmic::theme::ThemeType::Custom(_)
        ));
        assert!(matches!(
            light.theme_type,
            cosmic::theme::ThemeType::Custom(_)
        ));
    }
    #[test]
    fn theme_survives_reloads_and_previous_choice_is_backed_up() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("preferences.json");
        assert_eq!(load_from(&path).unwrap().theme, AppTheme::System);
        save_to(
            &path,
            &Preferences {
                theme: AppTheme::Dark,
            },
        )
        .unwrap();
        assert_eq!(load_from(&path).unwrap().theme, AppTheme::Dark);
        save_to(
            &path,
            &Preferences {
                theme: AppTheme::Light,
            },
        )
        .unwrap();
        assert_eq!(load_from(&path).unwrap().theme, AppTheme::Light);
        let backup = fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .find(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .unwrap()
            .path();
        assert_eq!(load_from(&backup).unwrap().theme, AppTheme::Dark);
    }
    #[test]
    fn unreadable_settings_are_not_silently_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("preferences.json");
        fs::write(&path, "invalid settings").unwrap();
        assert!(load_from(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "invalid settings");
        save_to(&path, &Preferences::default()).unwrap();
        let backup = fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .find(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .unwrap()
            .path();
        assert_eq!(fs::read_to_string(backup).unwrap(), "invalid settings");
    }
}
