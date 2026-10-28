// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline integration test. Refuses to run against the user's installation.
use crate::backend::{self, Job, Operation, Paths, Result};
use std::{fs, path::Path};
pub fn run(root: &Path) -> Result<()> {
    if !root.is_absolute() || !root.join(".warehouse-test-root").is_file() {
        return Err("Use the fixture directory created by verification/backend-smoke.sh".into());
    }
    for (key, part) in [
        ("HOME", "home"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        if std::env::var_os(key).as_deref() != Some(root.join(part).as_os_str()) {
            return Err(format!("{key} is not isolated under {}", root.display()));
        }
    }
    let paths = Paths::discover()?;
    let apply = |op: Operation| -> Result<()> {
        println!("{}", op.description());
        let report = backend::execute(op, &paths, &Job::default())?;
        if !report.errors.is_empty() {
            return Err(report.errors.join("\n"));
        }
        println!("PASS: {}", report.title);
        Ok(())
    };
    apply(Operation::RemoteAdd {
        installation: "user".into(),
        name: "warehouse-test".into(),
        location: root.join("test.flatpakrepo").display().to_string(),
    })?;
    let inv = backend::load(&paths)?;
    let remote = inv
        .remotes
        .iter()
        .find(|r| r.name == "warehouse-test" && r.installation == "user")
        .cloned()
        .ok_or("Test remote missing")?;
    if backend::remote_packages(&remote, "WarehouseFixture")?.is_empty() {
        return Err("Remote package search failed".into());
    }
    apply(Operation::RemoteEnabled(remote.clone(), false))?;
    if !backend::load(&paths)?
        .remotes
        .iter()
        .any(|r| r.name == "warehouse-test" && r.disabled())
    {
        return Err("Remote was not disabled".into());
    }
    apply(Operation::RemoteEnabled(remote.clone(), true))?;
    apply(Operation::Install {
        installation: "user".into(),
        remote: "warehouse-test".into(),
        refs: vec!["org.example.WarehouseFixture".into()],
    })?;
    let inv = backend::load(&paths)?;
    let package = inv
        .packages
        .iter()
        .find(|p| p.id == "org.example.WarehouseFixture" && p.installation == "user")
        .cloned()
        .ok_or("Fixture was not installed")?;
    let runtime = inv
        .packages
        .iter()
        .find(|p| p.id == "org.example.WarehousePlatform" && p.installation == "user")
        .cloned()
        .ok_or("Fixture runtime was not installed")?;
    if !backend::details(&package)?.contains("WarehouseFixture") {
        return Err("Package details missing".into());
    }
    if !backend::history(&package)?.contains("Commit:") {
        return Err("Version history missing".into());
    }
    apply(Operation::Mask(package.clone(), true))?;
    if !backend::load(&paths)?
        .packages
        .iter()
        .any(|p| p.key() == package.key() && p.masked)
    {
        return Err("Mask did not apply".into());
    }
    apply(Operation::Mask(package.clone(), false))?;
    apply(Operation::Pin(runtime.clone(), true))?;
    if !backend::load(&paths)?
        .packages
        .iter()
        .any(|p| p.key() == runtime.key() && p.pinned)
    {
        return Err("Pin did not apply".into());
    }
    apply(Operation::Pin(runtime, false))?;
    let first = fs::read_to_string(root.join("first-commit")).map_err(|e| e.to_string())?;
    apply(Operation::Version(
        package.clone(),
        first.trim().into(),
        true,
    ))?;
    let info = backend::details(&package)?;
    if !info.contains(first.trim()) {
        return Err("Rollback commit was not installed".into());
    }
    apply(Operation::Mask(package.clone(), false))?;
    let data = paths.app_data(&package.id)?;
    fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    fs::write(data.join("fixture.txt"), "before").map_err(|e| e.to_string())?;
    if !backend::check_update(&package)?.update_available {
        return Err("Update check missed the newer remote commit".into());
    }
    apply(Operation::Update(vec![package.clone()]))?;
    if backend::check_update(&package)?.update_available {
        return Err("Update did not install the remote head".into());
    }
    apply(Operation::RemoteAdd {
        installation: "user".into(),
        name: "warehouse-alternate".into(),
        location: root.join("test.flatpakrepo").display().to_string(),
    })?;
    let alternate = backend::load(&paths)?
        .remotes
        .into_iter()
        .find(|r| r.name == "warehouse-alternate")
        .ok_or("Alternate remote missing")?;
    apply(Operation::Reinstall {
        package: package.clone(),
        remote: alternate.clone(),
    })?;
    let switched = backend::load(&paths)?
        .packages
        .into_iter()
        .find(|p| p.id == package.id && p.installation == "user")
        .ok_or("Switched package missing")?;
    if switched.origin != "warehouse-alternate" {
        return Err("Reinstall did not switch origin".into());
    }
    if fs::read_to_string(data.join("fixture.txt")).map_err(|e| e.to_string())? != "before" {
        return Err("Source switch changed app data".into());
    }
    apply(Operation::Reinstall {
        package: switched,
        remote: remote.clone(),
    })?;
    apply(Operation::RemoteRemove(alternate))?;
    apply(Operation::Snapshot {
        ids: vec![package.id.clone()],
        name: "Before change".into(),
    })?;
    let snapshot = backend::load(&paths)?
        .snapshots
        .into_iter()
        .find(|s| s.id == package.id)
        .ok_or("Snapshot missing")?;
    fs::write(data.join("fixture.txt"), "after").map_err(|e| e.to_string())?;
    apply(Operation::Restore(snapshot.clone()))?;
    if fs::read_to_string(data.join("fixture.txt")).map_err(|e| e.to_string())? != "before" {
        return Err("Snapshot restore changed data".into());
    }
    apply(Operation::RenameSnapshot(
        snapshot,
        "Renamed fixture".into(),
    ))?;
    apply(Operation::Uninstall(vec![package], false))?;
    if !data.is_dir() {
        return Err("Uninstall removed app data unexpectedly".into());
    }
    let inv = backend::load(&paths)?;
    if !inv
        .data
        .iter()
        .any(|d| d.id == "org.example.WarehouseFixture" && !d.installed)
    {
        return Err("Leftover app data was not detected".into());
    }
    println!(
        "PASS: install, remotes, masks/pins, rollback, update check/apply, source migration with app data preserved, snapshots, uninstall, leftovers"
    );
    Ok(())
}
