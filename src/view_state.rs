// SPDX-License-Identifier: GPL-3.0-or-later
//! Filters, sort orders and the last page, restored between launches like
//! Warehouse. Saved on a normal close; capture instances never write it.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageSort {
    #[default]
    Name,
    Id,
    Remote,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataSort {
    #[default]
    Name,
    Id,
    Size,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    pub page: String,
    pub applications: bool,
    pub runtimes: bool,
    pub eol_only: bool,
    /// `installation:remote` keys. Empty means all remotes.
    pub remotes: BTreeSet<String>,
    /// Runtime refs. Empty means any runtime.
    pub runtime_refs: BTreeSet<String>,
    /// Empty means all installations.
    pub installation: String,
    pub package_sort: PackageSort,
    pub data_sort: DataSort,
    pub data_descending: bool,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            page: "packages".into(),
            applications: true,
            runtimes: false,
            eol_only: false,
            remotes: BTreeSet::new(),
            runtime_refs: BTreeSet::new(),
            installation: String::new(),
            package_sort: PackageSort::Name,
            data_sort: DataSort::Name,
            data_descending: false,
        }
    }
}
impl ViewState {
    pub fn filters_are_default(&self) -> bool {
        let default = Self::default();
        self.applications == default.applications
            && self.runtimes == default.runtimes
            && self.eol_only == default.eol_only
            && self.remotes.is_empty()
            && self.runtime_refs.is_empty()
            && self.installation.is_empty()
    }
    pub fn reset_filters(&mut self) {
        let default = Self::default();
        self.applications = default.applications;
        self.runtimes = default.runtimes;
        self.eol_only = default.eol_only;
        self.remotes.clear();
        self.runtime_refs.clear();
        self.installation.clear();
    }
}
fn path() -> std::path::PathBuf {
    crate::preferences::directory().join("view-state.json")
}
pub fn load() -> Result<ViewState, String> {
    load_from(&path())
}
fn load_from(path: &Path) -> Result<ViewState, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Cannot read saved filters {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ViewState::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(state: &ViewState) -> Result<(), String> {
    crate::window_state::write_json(&path(), state, "view")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_and_sorting_survive_reloads_and_reset_keeps_sorting() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("view-state.json");
        assert_eq!(load_from(&path).unwrap(), ViewState::default());
        let mut state = ViewState {
            page: "data".into(),
            runtimes: true,
            data_sort: DataSort::Size,
            data_descending: true,
            ..ViewState::default()
        };
        state.remotes.insert("user:flathub".into());
        crate::window_state::write_json(&path, &state, "view").unwrap();
        let restored = load_from(&path).unwrap();
        assert_eq!(restored, state);
        assert!(!restored.filters_are_default());
        let mut reset = restored;
        reset.reset_filters();
        assert!(reset.filters_are_default());
        assert_eq!(reset.data_sort, DataSort::Size);
        assert_eq!(reset.page, "data");
        fs::write(&path, "{\"page\":\"remotes\",\"unknown\":1}").unwrap();
        assert_eq!(load_from(&path).unwrap().page, "remotes");
    }
}
