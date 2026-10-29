// SPDX-License-Identifier: GPL-3.0-or-later
//! Presentation data for the remote manager. Adding a preset still needs review.
use serde::Deserialize;
use std::sync::LazyLock;
use warehouse_cosmic::backend::Remote;

#[derive(Debug, Deserialize)]
pub struct PopularRemote {
    pub name: String,
    pub title: String,
    pub description: String,
    pub url: String,
}

pub static POPULAR: LazyLock<Vec<PopularRemote>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/popular-remotes.json"))
        .expect("Bundled remote catalog is valid JSON")
});

pub fn title(remote: &Remote) -> &str {
    let title = remote.title.trim();
    if title.is_empty() || title == "-" {
        &remote.name
    } else {
        title
    }
}

/// Whether a preset is already configured, by name or repository URL.
pub fn added(preset: &PopularRemote, configured: &[Remote]) -> bool {
    let url = |value: &str| value.trim().trim_end_matches('/').to_lowercase();
    configured
        .iter()
        .any(|r| r.name == preset.name || (!r.url.is_empty() && url(&r.url) == url(&preset.url)))
}

pub fn visible(remote: &Remote, query: &str, show_disabled: bool) -> bool {
    (show_disabled || !remote.disabled())
        && format!(
            "{} {} {} {}",
            remote.name,
            title(remote),
            remote.url,
            remote.installation
        )
        .to_lowercase()
        .contains(&query.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_respects_disabled_visibility_and_handles_missing_titles() {
        let mut remote = Remote {
            name: "fedora".into(),
            title: "-".into(),
            url: "oci+https://registry.fedoraproject.org".into(),
            installation: "user".into(),
            options: "disabled".into(),
        };
        assert_eq!(title(&remote), "fedora");
        assert!(!visible(&remote, "", false));
        assert!(!visible(&remote, "FEDORA", false));
        assert!(visible(&remote, "FEDORA", true));
        assert!(visible(&remote, "registry.fedoraproject", true));
        assert!(!visible(&remote, "flathub", true));
        remote.options.clear();
        remote.title = "Fedora Flatpaks".into();
        assert_eq!(title(&remote), "Fedora Flatpaks");
        assert!(visible(&remote, "user", false));
    }
}
