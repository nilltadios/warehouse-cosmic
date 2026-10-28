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
        let launched = command_for("flatpak", true, false);
        assert!(!launched.get_args().any(|arg| arg == "--watch-bus"));
        assert_eq!(command_for("flatpak", false, true).get_program(), "flatpak");
    }
}
