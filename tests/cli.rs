use std::{fs, process::Command};
fn cli() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_warehouse-cosmic-cli"));
    c.env_remove("WAYLAND_DISPLAY").env_remove("DISPLAY");
    c
}
#[test]
fn discovery_works_without_a_display() {
    let output = cli().args(["commands", "--json"]).output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "warehouse-cosmic-commands/1");
    assert!(
        value["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["command"] == "snapshot.restore" && c["mutates"] == true)
    );
}
#[test]
fn unknown_parameter_is_a_structured_usage_error() {
    let output = cli()
        .args(["run", "--cmd", "data.list={\"force\":true}"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "invalid_argument");
}
#[test]
fn invalid_later_step_prevents_all_script_effects() {
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("steps.jsonl");
    fs::write(&script,"remote.add={\"installation\":\"user\",\"name\":\"never-created\",\"location\":\"https://example.invalid/repo\"}\nunknown.command\n").unwrap();
    let output = cli()
        .env("HOME", root.path().join("home"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .args(["run", "--apply", "--script"])
        .arg(script)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!root.path().join("data").exists());
}
#[test]
fn describe_and_schema_are_valid_json() {
    for args in [vec!["describe", "package.uninstall"], vec!["schema"]] {
        let output = cli().args(args).output().unwrap();
        assert!(output.status.success());
        assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok());
    }
}
#[test]
fn gui_discovery_is_available_without_a_desktop_or_gui_process() {
    for args in [["ui", "commands"], ["ui", "schema"]] {
        let output = cli()
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/nonexistent-ui-test-bus",
            )
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok());
    }
}
