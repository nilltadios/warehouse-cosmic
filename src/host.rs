// SPDX-License-Identifier: GPL-3.0-or-later
//! Run package operations on the host when this process is Flatpak-packaged.
use std::{path::Path, process::Command};

pub fn sandboxed() -> bool {
    Path::new("/.flatpak-info").is_file()
}

pub fn command(program: &str, watch: bool) -> Command {
    command_for(program, sandboxed(), watch)
}

fn command_for(program: &str, sandbox: bool, watch: bool) -> Command {
    if sandbox {
        let mut cmd = Command::new("/usr/bin/flatpak-spawn");
        cmd.args([
            "--host",
            "--directory=/",
            "--env=LC_ALL=C",
            "--env=FLATPAK_FANCY_OUTPUT=0",
        ]);
        if watch {
            cmd.arg("--watch-bus");
        }
        cmd.arg(program);
        cmd
    } else {
        Command::new(program)
    }
}

/// Size of a regular file that the host's Flatpak can read. Some host paths,
/// such as the host's /tmp, are not visible inside the sandbox, so the host is
/// asked when the sandbox cannot see the file.
pub fn file_size(path: &Path) -> Option<u64> {
    if let Ok(meta) = std::fs::metadata(path) {
        return meta.is_file().then_some(meta.len());
    }
    if !sandboxed() {
        return None;
    }
    let output = command("stat", false)
        .args(["--format=%F %s", "--"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (kind, size) = text.trim().rsplit_once(' ')?;
    kind.starts_with("regular")
        .then(|| size.parse().ok())
        .flatten()
}
pub fn is_file(path: &Path) -> bool {
    file_size(path).is_some()
}

pub fn installations_dir() -> &'static Path {
    Path::new(if sandboxed() {
        "/run/host/etc/flatpak/installations.d"
    } else {
        "/etc/flatpak/installations.d"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_bridge_preserves_arguments_and_watches_transactions() {
        let mut cmd = command_for("pkexec", true, true);
        cmd.args(["/usr/bin/flatpak", "--user", "name; touch /tmp/no"]);
        assert_eq!(cmd.get_program(), "/usr/bin/flatpak-spawn");
        let args: Vec<_> = cmd.get_args().map(|v| v.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "--host",
                "--directory=/",
                "--env=LC_ALL=C",
                "--env=FLATPAK_FANCY_OUTPUT=0",
                "--watch-bus",
                "pkexec",
                "/usr/bin/flatpak",
                "--user",
                "name; touch /tmp/no"
            ]
        );
        let temp = std::env::temp_dir().join(format!("warehouse-size-{}", std::process::id()));
        std::fs::write(&temp, b"12345").unwrap();
        assert_eq!(file_size(&temp), Some(5));
        assert_eq!(file_size(temp.parent().unwrap()), None);
        std::fs::remove_file(&temp).unwrap();
        let launched = command_for("flatpak", true, false);
        assert!(!launched.get_args().any(|arg| arg == "--watch-bus"));
        assert_eq!(command_for("flatpak", false, true).get_program(), "flatpak");
    }
}
