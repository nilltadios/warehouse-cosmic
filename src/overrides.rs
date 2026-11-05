// SPDX-License-Identifier: GPL-3.0-or-later
//! Launchers and icons in the user's own data folder that take priority over
//! the copies Flatpak exports. `$XDG_DATA_HOME` comes before the Flatpak
//! exports in the desktop's lookup order, so a copy left by a native install,
//! a menu editor or a script keeps winning after an install, an uninstall or a
//! source change. Nothing here removes a file on its own: fixing one moves it
//! to Trash, and the user can keep any of them.
use crate::backend::{Package, Paths, Result, move_to_trash, validate_id};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    time::UNIX_EPOCH,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Launcher,
    Icon,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Override {
    /// The application ID the file is named after.
    pub id: String,
    pub kind: Kind,
    pub path: PathBuf,
    /// Opening the app from this launcher fails or starts something else.
    pub broken: bool,
    /// The application is installed as a Flatpak.
    pub installed: bool,
    /// One plain sentence describing what the file does.
    pub message: String,
    /// The user chose to keep this file as it is now.
    pub kept: bool,
}

/// The user's data folder on the host, where launchers and icons live.
pub fn user_share(home: &Path, sandboxed: bool) -> PathBuf {
    // Inside Flatpak, XDG_DATA_HOME is the app's private folder. Flatpak passes
    // the host's value as HOST_XDG_DATA_HOME when the host sets one.
    let var = if sandboxed {
        "HOST_XDG_DATA_HOME"
    } else {
        "XDG_DATA_HOME"
    };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
}

fn tilde(paths: &Paths, path: &Path) -> String {
    match path.strip_prefix(&paths.home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
/// A symbolic link into Flatpak's exports is Flatpak's own file.
fn exported(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
        && fs::canonicalize(path)
            .is_ok_and(|target| target.to_string_lossy().contains("/flatpak/exports/"))
}
fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut list: Vec<_> = fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    list.sort();
    list
}

/// Split an Exec value into arguments following the desktop entry quoting
/// rules. Arguments that contain spaces (such as `sh -c "…"`) are split again
/// so a `flatpak run` inside them is found.
fn arguments(exec: &str) -> Vec<String> {
    fn split(text: &str) -> Vec<String> {
        let (mut out, mut word, mut quoted, mut chars) =
            (Vec::new(), String::new(), false, text.chars());
        let mut any = false;
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    quoted = !quoted;
                    any = true;
                }
                '\\' if quoted => word.extend(chars.next()),
                c if c.is_whitespace() && !quoted => {
                    if any || !word.is_empty() {
                        out.push(std::mem::take(&mut word));
                    }
                    any = false;
                }
                c => word.push(c),
            }
        }
        if any || !word.is_empty() {
            out.push(word);
        }
        out
    }
    let mut out = Vec::new();
    for arg in split(exec) {
        if arg.contains(char::is_whitespace) {
            out.extend(arguments(&arg));
        } else {
            out.push(arg);
        }
    }
    out
}
#[derive(Debug, Default, PartialEq, Eq)]
struct Target {
    id: String,
    branch: Option<String>,
    arch: Option<String>,
    installation: Option<String>,
}
/// The app a `flatpak run` command line starts.
fn flatpak_target(exec: &str) -> Option<Target> {
    let args = arguments(exec);
    let start = args.windows(2).position(|w| {
        Path::new(&w[0]).file_name().is_some_and(|n| n == "flatpak") && w[1] == "run"
    })?;
    let mut target = Target::default();
    let mut rest = args[start + 2..].iter();
    while let Some(arg) = rest.next() {
        let Some(option) = arg.strip_prefix("--") else {
            if arg.starts_with('-') {
                continue;
            }
            // APP, APP//BRANCH, or app/APP/ARCH/BRANCH.
            let reference = arg.strip_prefix("app/").unwrap_or(arg);
            let mut parts = reference.split('/');
            target.id = parts.next().unwrap_or_default().to_string();
            let nonempty = |v: Option<&str>| v.filter(|v| !v.is_empty()).map(str::to_string);
            target.arch = target.arch.or(nonempty(parts.next()));
            target.branch = target.branch.or(nonempty(parts.next()));
            return (!target.id.is_empty()).then_some(target);
        };
        let (name, value) = match option.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (option, None),
        };
        let takes_value = matches!(
            name,
            "branch"
                | "arch"
                | "command"
                | "installation"
                | "runtime"
                | "runtime-version"
                | "commit"
                | "runtime-commit"
                | "cwd"
        );
        let value = if takes_value && value.is_none() {
            rest.next().cloned()
        } else {
            value
        };
        match name {
            "branch" => target.branch = value,
            "arch" => target.arch = value,
            "user" => target.installation = Some("user".into()),
            "system" => target.installation = Some("system".into()),
            "installation" => target.installation = value,
            _ => (),
        }
    }
    None
}
/// `(group, key, value)` for every entry of a desktop file.
fn entries(text: &str) -> Vec<(&str, &str, &str)> {
    let mut group = "";
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            group = name;
        } else if let Some((key, value)) = line.split_once('=')
            && !line.starts_with('#')
        {
            out.push((group, key.trim(), value.trim()));
        }
    }
    out
}
/// Why starting `exec` from a launcher named after `id` fails, if it does.
fn exec_problem(id: &str, exec: &str, apps: &[&Package]) -> Option<String> {
    let Some(target) = flatpak_target(exec) else {
        let program = arguments(exec).into_iter().next().unwrap_or_default();
        return Some(format!("starts “{program}” instead of the Flatpak"));
    };
    let mut found: Vec<&&Package> = apps.iter().filter(|p| p.id == target.id).collect();
    if found.is_empty() {
        return Some(format!("starts {}, which is not installed", target.id));
    }
    let list = |found: &[&&Package], f: fn(&Package) -> &str| {
        found
            .iter()
            .map(|p| f(p).to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(installation) = &target.installation {
        let installed = list(&found, |p| &p.installation);
        found.retain(|p| &p.installation == installation);
        if found.is_empty() {
            return Some(format!(
                "starts it from the {installation} installation, but it is installed in {installed}"
            ));
        }
    }
    if let Some(branch) = &target.branch {
        let installed = list(&found, |p| &p.branch);
        found.retain(|p| &p.branch == branch);
        if found.is_empty() {
            return Some(format!(
                "starts branch {branch}, which is not installed (installed: {installed})"
            ));
        }
    }
    if let Some(arch) = &target.arch {
        let installed = list(&found, |p| &p.arch);
        found.retain(|p| &p.arch == arch);
        if found.is_empty() {
            return Some(format!(
                "starts the {arch} build, which is not installed (installed: {installed})"
            ));
        }
    }
    (target.id != id).then(|| format!("starts {} instead of this app", target.id))
}
/// `(broken, what the launcher does)`.
fn judge_launcher(id: &str, text: &str, apps: &[&Package]) -> (bool, String) {
    let entries = entries(text);
    let main = |key: &str| {
        entries
            .iter()
            .find(|(g, k, _)| *g == "Desktop Entry" && *k == key)
            .map(|(_, _, v)| *v)
    };
    if main("Hidden") == Some("true") {
        return (false, "hides this app from the application menu".into());
    }
    if !apps.iter().any(|p| p.id == id) {
        return (
            true,
            format!("is left over from {id}, which is not installed"),
        );
    }
    let Some(exec) = main("Exec") else {
        return (true, "has no command to start the app".into());
    };
    // The main command decides; actions (right-click entries) come next.
    let actions = entries
        .iter()
        .filter(|(g, k, _)| g.starts_with("Desktop Action ") && *k == "Exec")
        .map(|(_, _, v)| *v);
    if let Some(problem) = std::iter::once(exec)
        .chain(actions)
        .find_map(|e| exec_problem(id, e, apps))
    {
        return (true, format!("replaces this app's own and {problem}"));
    }
    let hidden = main("NoDisplay") == Some("true");
    (
        false,
        if hidden {
            "replaces this app's own and hides it from the application menu".into()
        } else {
            "replaces this app's own launcher".into()
        },
    )
}

/// Find launchers and icons that override a Flatpak's own. `known` lists
/// further app IDs to check, such as leftover data folders of uninstalled apps.
pub fn scan(paths: &Paths, packages: &[Package], known: &[&str]) -> Vec<Override> {
    let apps: Vec<&Package> = packages.iter().filter(|p| !p.is_runtime()).collect();
    let installed: BTreeSet<&str> = apps.iter().map(|p| p.id.as_str()).collect();
    let mut ids: BTreeSet<String> = installed
        .iter()
        .chain(known)
        .map(|s| s.to_string())
        .collect();
    let kept = load_kept(paths);
    let is_kept = |path: &Path| {
        kept.get(path)
            .is_some_and(|stamp| Some(*stamp) == modified(path))
    };
    let mut found = Vec::new();
    for path in sorted_entries(&paths.share.join("applications")) {
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".desktop"))
        else {
            continue;
        };
        if validate_id(id).is_err() || exported(&path) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        // Other launchers that happen to use reverse-DNS names are not ours to judge.
        let flatpak = text.lines().any(|l| l.starts_with("X-Flatpak="))
            || entries(&text)
                .iter()
                .any(|(_, k, v)| *k == "Exec" && flatpak_target(v).is_some());
        if !installed.contains(id) && !flatpak {
            continue;
        }
        let (broken, what) = judge_launcher(id, &text, &apps);
        ids.insert(id.to_string());
        found.push(Override {
            id: id.into(),
            kind: Kind::Launcher,
            message: format!("The launcher {} {what}.", tilde(paths, &path)),
            kept: is_kept(&path),
            installed: installed.contains(id),
            broken,
            path,
        });
    }
    let mut icons = Vec::new();
    for theme in sorted_entries(&paths.share.join("icons")) {
        // Both theme layouts: SIZE/apps (hicolor) and apps/SIZE (Breeze).
        for sub in sorted_entries(&theme) {
            icons.extend(sorted_entries(&sub.join("apps")));
            if sub.file_name().is_some_and(|n| n == "apps") {
                for size in sorted_entries(&sub) {
                    icons.extend(sorted_entries(&size));
                }
            }
        }
    }
    for path in icons {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = [".svg", ".png", ".svgz", ".xpm"]
            .iter()
            .find_map(|ext| name.strip_suffix(ext))
        else {
            continue;
        };
        let id = stem.strip_suffix("-symbolic").unwrap_or(stem);
        if !ids.contains(id)
            || exported(&path)
            || !fs::symlink_metadata(&path).is_ok_and(|m| !m.is_dir())
        {
            continue;
        }
        let what = if installed.contains(id) {
            "replaces this app's own icon".to_string()
        } else {
            format!("is left over from {id}, which is not installed")
        };
        found.push(Override {
            id: id.into(),
            kind: Kind::Icon,
            message: format!("The icon {} {what}.", tilde(paths, &path)),
            kept: is_kept(&path),
            installed: installed.contains(id),
            broken: false,
            path,
        });
    }
    found
}

fn kept_file(paths: &Paths) -> PathBuf {
    paths.data.join("kept-overrides.json")
}
fn modified(path: &Path) -> Option<u128> {
    fs::symlink_metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos())
}
/// Files the user chose to keep, with the modification time they had then.
/// A file that changes afterwards is reported again.
fn load_kept(paths: &Paths) -> BTreeMap<PathBuf, u128> {
    fs::read(kept_file(paths))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}
/// Remember (or forget) the user's choice to keep these files as they are.
pub fn set_kept(paths: &Paths, files: &[PathBuf], keep: bool) -> Result<()> {
    let mut kept = load_kept(paths);
    for file in files {
        if keep {
            let stamp =
                modified(file).ok_or_else(|| format!("{} does not exist", file.display()))?;
            kept.insert(file.clone(), stamp);
        } else {
            kept.remove(file);
        }
    }
    fs::create_dir_all(&paths.data).map_err(|e| e.to_string())?;
    let target = kept_file(paths);
    let staging = target.with_extension(format!("json.tmp-{}", std::process::id()));
    let data = serde_json::to_vec_pretty(&kept).map_err(|e| e.to_string())?;
    fs::File::create(&staging)
        .and_then(|mut f| f.write_all(&data).and_then(|()| f.sync_all()))
        .and_then(|()| fs::rename(&staging, &target))
        .map_err(|e| format!("Cannot save {}: {e}", target.display()))
}

/// Only launcher and icon files directly in the user's own menu and icon
/// folders can be moved, whatever a request names.
pub fn check_movable(paths: &Paths, file: &Path) -> Result<()> {
    let refuse = || {
        Err(format!(
            "{} is not a launcher or icon in {}",
            file.display(),
            paths.share.display()
        ))
    };
    if file.components().any(|c| c.as_os_str() == "..") {
        return refuse();
    }
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let launcher =
        file.parent() == Some(&paths.share.join("applications")) && name.ends_with(".desktop");
    let icon = file.starts_with(paths.share.join("icons"))
        && file
            .ancestors()
            .any(|a| a.file_name().is_some_and(|n| n == "apps"))
        && [".svg", ".png", ".svgz", ".xpm"]
            .iter()
            .any(|e| name.ends_with(e));
    if !(launcher || icon) || fs::symlink_metadata(file).is_ok_and(|m| m.is_dir()) {
        return refuse();
    }
    Ok(())
}

/// Move files to Trash, then refresh the desktop caches that exist for their
/// folders. Returns `(steps, errors)`; a failed refresh is reported, not hidden.
pub fn trash(paths: &Paths, files: &[PathBuf]) -> Result<(Vec<String>, Vec<String>)> {
    let (mut steps, mut errors) = (Vec::new(), Vec::new());
    let mut refresh_menu = false;
    let mut themes = BTreeSet::new();
    for file in files {
        check_movable(paths, file)?;
        move_to_trash(file)?;
        steps.push(format!("Moved {} to Trash", file.display()));
        let icons = paths.share.join("icons");
        if let Ok(rest) = file.strip_prefix(&icons) {
            if let Some(theme) = rest.components().next() {
                themes.insert(icons.join(theme));
            }
        } else {
            refresh_menu = true;
        }
    }
    let _ = set_kept(paths, files, false);
    let applications = paths.share.join("applications");
    let mut refreshes: Vec<(&str, Vec<String>)> = Vec::new();
    // Only caches that already exist are refreshed; without one the desktop
    // reads the folders directly.
    if refresh_menu && applications.join("mimeinfo.cache").is_file() {
        refreshes.push((
            "update-desktop-database",
            vec![applications.display().to_string()],
        ));
    }
    for theme in themes {
        if theme.join("icon-theme.cache").is_file() {
            refreshes.push((
                "gtk-update-icon-cache",
                vec![
                    "--force".into(),
                    "--ignore-theme-index".into(),
                    "--quiet".into(),
                    theme.display().to_string(),
                ],
            ));
        }
    }
    for (program, args) in refreshes {
        let result = crate::host::command(program, false)
            .args(&args)
            .stdin(Stdio::null())
            .output();
        match result {
            Ok(out) if out.status.success() => {
                steps.push(format!("Refreshed with {program} {}", args.join(" ")))
            }
            Ok(out) => errors.push(format!(
                "{program} could not refresh {}: {}",
                args.last().cloned().unwrap_or_default(),
                String::from_utf8_lossy(&out.stderr).trim()
            )),
            Err(e) => errors.push(format!("Cannot run {program}: {e}")),
        }
    }
    Ok((steps, errors))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, branch: &str, installation: &str) -> Package {
        Package {
            name: id.into(),
            id: id.into(),
            version: String::new(),
            branch: branch.into(),
            arch: "x86_64".into(),
            origin: "origin".into(),
            installation: installation.into(),
            reference: format!("{id}/x86_64/{branch}"),
            size: String::new(),
            options: "current".into(),
            runtime: String::new(),
            masked: false,
            pinned: false,
            masks: vec![],
            pins: vec![],
        }
    }
    fn fixture() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: t.path().into(),
            data: t.path().join("data"),
            share: t.path().join(".local/share"),
        };
        (t, paths)
    }
    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn reads_flatpak_run_lines_like_exported_and_hand_written_launchers() {
        let t = flatpak_target("/usr/bin/flatpak run --branch=master --arch=x86_64 --command=a --file-forwarding org.x.App @@u %U @@").unwrap();
        assert_eq!(
            t,
            Target {
                id: "org.x.App".into(),
                branch: Some("master".into()),
                arch: Some("x86_64".into()),
                installation: None
            }
        );
        let t = flatpak_target("env A=1 flatpak run --user --branch stable org.x.App").unwrap();
        assert_eq!(
            (t.branch.as_deref(), t.installation.as_deref()),
            (Some("stable"), Some("user"))
        );
        let t = flatpak_target("sh -c \"flatpak run app/org.x.App/aarch64/beta\"").unwrap();
        assert_eq!(
            (t.arch.as_deref(), t.branch.as_deref()),
            (Some("aarch64"), Some("beta"))
        );
        assert_eq!(
            flatpak_target("flatpak run org.x.App//test")
                .unwrap()
                .branch
                .as_deref(),
            Some("test")
        );
        assert!(flatpak_target("\"/home/u/.local/bin/warehouse-cosmic\" %F").is_none());
    }

    #[test]
    fn reports_broken_and_shadowing_launchers_and_icons_and_remembers_kept_files() {
        let (_t, paths) = fixture();
        let apps = paths.share.join("applications");
        let packages = vec![
            app("org.x.Auth", "stable", "user"),
            app("org.x.Fine", "stable", "user"),
        ];
        // The CosmicAuthenticator case: an old copy pinned to a removed branch.
        write(
            &apps.join("org.x.Auth.desktop"),
            "[Desktop Entry]\nType=Application\nExec=/usr/bin/flatpak run --branch=master --arch=x86_64 org.x.Auth\nX-Flatpak=org.x.Auth\n",
        );
        // A working customization.
        write(
            &apps.join("org.x.Fine.desktop"),
            "[Desktop Entry]\nExec=flatpak run org.x.Fine\nName=Mine\n",
        );
        // Left over from an uninstalled app, and an unrelated native launcher.
        write(
            &apps.join("org.x.Gone.desktop"),
            "[Desktop Entry]\nExec=flatpak run org.x.Gone\n",
        );
        write(
            &apps.join("org.native.Tool.desktop"),
            "[Desktop Entry]\nExec=tool\n",
        );
        // The Warehouse icon case, plus a symbolic icon and a Flatpak export link.
        let hicolor = paths.share.join("icons/hicolor");
        write(&hicolor.join("scalable/apps/org.x.Fine.svg"), "<svg/>");
        write(
            &hicolor.join("symbolic/apps/org.x.Fine-symbolic.svg"),
            "<svg/>",
        );
        let export = paths
            .share
            .join("flatpak/exports/share/icons/hicolor/48x48/apps/org.x.Auth.png");
        write(&export, "png");
        fs::create_dir_all(hicolor.join("48x48/apps")).unwrap();
        std::os::unix::fs::symlink(&export, hicolor.join("48x48/apps/org.x.Auth.png")).unwrap();

        let found = scan(&paths, &packages, &[]);
        let summary: Vec<_> = found
            .iter()
            .map(|o| (o.id.as_str(), o.kind, o.broken, o.installed))
            .collect();
        assert_eq!(
            summary,
            [
                ("org.x.Auth", Kind::Launcher, true, true),
                ("org.x.Fine", Kind::Launcher, false, true),
                ("org.x.Gone", Kind::Launcher, true, false),
                ("org.x.Fine", Kind::Icon, false, true),
                ("org.x.Fine", Kind::Icon, false, true),
            ]
        );
        assert_eq!(
            found[0].message,
            "The launcher ~/.local/share/applications/org.x.Auth.desktop replaces this app's own and starts branch master, which is not installed (installed: stable)."
        );
        assert!(found[3].message.ends_with("replaces this app's own icon."));

        set_kept(&paths, &[found[3].path.clone()], true).unwrap();
        let again = scan(&paths, &packages, &[]);
        assert!(again[3].kept && !again[4].kept);
        // Changing the kept file asks again.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&found[3].path, "<svg>new</svg>");
        assert!(!scan(&paths, &packages, &[])[3].kept);
    }

    #[test]
    fn native_launchers_of_installed_flatpaks_are_broken_and_hidden_ones_are_not() {
        let (_t, paths) = fixture();
        let apps = paths.share.join("applications");
        let packages = vec![
            app("org.x.App", "stable", "user"),
            app("org.x.Two", "stable", "system"),
        ];
        write(
            &apps.join("org.x.App.desktop"),
            "[Desktop Entry]\nExec=\"/home/u/.local/bin/app\" %F\n",
        );
        write(
            &apps.join("org.x.Two.desktop"),
            "[Desktop Entry]\nHidden=true\n",
        );
        let found = scan(&paths, &packages, &[]);
        assert!(
            found[0].broken
                && found[0]
                    .message
                    .contains("starts “/home/u/.local/bin/app” instead of the Flatpak")
        );
        assert!(!found[1].broken && found[1].message.contains("hides this app"));
        write(
            &apps.join("org.x.Two.desktop"),
            "[Desktop Entry]\nExec=flatpak run --user org.x.Two\n",
        );
        assert!(
            scan(&paths, &packages, &[])[1]
                .message
                .contains("installed in system")
        );
    }

    #[test]
    fn only_launchers_and_icons_in_the_user_folders_can_be_moved() {
        let (_t, paths) = fixture();
        let launcher = paths.share.join("applications/org.x.App.desktop");
        let icon = paths
            .share
            .join("icons/hicolor/scalable/apps/org.x.App.svg");
        write(&launcher, "[Desktop Entry]\n");
        write(&icon, "<svg/>");
        assert!(check_movable(&paths, &launcher).is_ok());
        assert!(check_movable(&paths, &icon).is_ok());
        for bad in [
            paths.home.join(".bashrc"),
            paths.share.join("applications"),
            paths.share.join("applications/../../.bashrc"),
            paths.share.join("icons/hicolor/index.theme"),
            paths.share.join("applications/sub/x.desktop"),
        ] {
            assert!(check_movable(&paths, &bad).is_err(), "{}", bad.display());
        }
    }
}
