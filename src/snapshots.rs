// SPDX-License-Identifier: GPL-3.0-or-later
//! Warehouse-compatible tar.zst snapshots, staged before data replacement.
use crate::backend::{self, Job, Paths, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub timestamp: u64,
    pub version: String,
    pub path: PathBuf,
    pub official: bool,
    pub bytes: u64,
}
#[derive(Serialize, Deserialize)]
struct Metadata {
    snapshot_version: u32,
    name: String,
}
fn metadata_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.json",
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .trim_end_matches(".tar.zst")
    ))
}
pub fn list(root: &Path, official: bool) -> Result<Vec<Snapshot>> {
    let mut result = vec![];
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(e) => return Err(format!("{}: {e}", root.display())),
    };
    for dir in entries {
        let dir = dir.map_err(|e| e.to_string())?;
        if !dir.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let id = dir.file_name().to_string_lossy().into_owned();
        if backend::validate_id(&id).is_err() {
            continue;
        }
        for entry in fs::read_dir(dir.path()).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".tar.zst") else {
                continue;
            };
            let Some((timestamp, version)) = stem.split_once('_') else {
                continue;
            };
            let Ok(timestamp) = timestamp.parse() else {
                continue;
            };
            let path = entry.path();
            let title = fs::read(metadata_path(&path))
                .ok()
                .and_then(|b| serde_json::from_slice::<Metadata>(&b).ok())
                .map(|m| m.name)
                .unwrap_or_else(|| "Unnamed snapshot".into());
            result.push(Snapshot {
                id: id.clone(),
                name: title,
                timestamp,
                version: version.into(),
                bytes: entry.metadata().map_err(|e| e.to_string())?.len(),
                path,
                official,
            });
        }
    }
    Ok(result)
}
fn private_dir(path: &Path) -> Result<()> {
    let existed = path.exists();
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    backend::ensure_real_dir(path)?;
    if existed {
        Ok(())
    } else {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
    }
}
pub fn export(paths: &Paths, s: &Snapshot, target: &Path, job: &Job) -> Result<()> {
    validate_snapshot(paths, s, false)?;
    if !target.is_absolute()
        || !target
            .file_name()
            .is_some_and(|p| p.to_string_lossy().ends_with(".tar.zst"))
    {
        return Err("Choose an absolute path ending in .tar.zst".into());
    }
    if target == s.path {
        return Err("Choose a different destination from the source snapshot".into());
    }
    for path in [target.to_path_buf(), metadata_path(target)] {
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.is_file() || meta.is_symlink() {
                return Err("Export destination must be a regular file".into());
            }
            let backup = path.with_file_name(format!(
                "{}.bak-{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                backend::now()
            ));
            if backup.exists() {
                return Err(format!("Backup already exists: {}", backup.display()));
            }
            fs::copy(&path, backup).map_err(|e| e.to_string())?;
        }
    }
    let stage = target.with_extension(format!("zst.partial-{}", std::process::id()));
    let copied = (|| -> Result<()> {
        let mut input = File::open(&s.path).map_err(|e| e.to_string())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            job.check()?;
            let n = input.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&stage, target).map_err(|e| e.to_string())?;
        save_metadata(target, &s.name)
    })();
    if copied.is_err() {
        let _ = fs::remove_file(stage);
    }
    copied
}
fn destination(paths: &Paths, id: &str) -> Result<PathBuf> {
    backend::validate_id(id)?;
    private_dir(&paths.data)?;
    private_dir(&paths.snapshots())?;
    let dir = paths.snapshots().join(id);
    private_dir(&dir)?;
    let stamp = backend::now();
    for n in 0..1000 {
        let path = dir.join(format!("{stamp}_data-{n}.tar.zst"));
        if !path.exists() && !metadata_path(&path).exists() {
            return Ok(path);
        }
    }
    Err("Cannot allocate snapshot filename".into())
}
pub fn create(paths: &Paths, id: &str, name: &str, job: &Job) -> Result<Snapshot> {
    let source = paths.app_data(id)?;
    backend::ensure_real_dir(&source)?;
    let target = destination(paths, id)?;
    let library = fs::canonicalize(paths.snapshots()).map_err(|e| e.to_string())?;
    let partial = target.with_extension("partial");
    let result = (|| -> Result<()> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|e| e.to_string())?;
        let encoder = zstd::Encoder::new(file, 3).map_err(|e| e.to_string())?;
        let mut archive = tar::Builder::new(encoder);
        archive.follow_symlinks(false);
        // Manual traversal permits cancellation between files and preserves symlinks.
        let mut dirs = vec![source.clone()];
        while let Some(dir) = dirs.pop() {
            job.check()?;
            let rel = dir.strip_prefix(&source).map_err(|e| e.to_string())?;
            if !rel.as_os_str().is_empty() {
                archive.append_dir(rel, &dir).map_err(|e| e.to_string())?;
            }
            for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                job.check()?;
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                let rel = path.strip_prefix(&source).map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    // A Flatpak stores its own library inside its app data.
                    // Never archive that library, including the live output.
                    if fs::canonicalize(&path).map_err(|e| e.to_string())? == library {
                        continue;
                    }
                    dirs.push(path);
                } else {
                    archive
                        .append_path_with_name(&path, rel)
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                }
            }
        }
        let encoder = archive.into_inner().map_err(|e| e.to_string())?;
        encoder
            .finish()
            .map_err(|e| e.to_string())?
            .sync_all()
            .map_err(|e| e.to_string())?;
        job.check()?;
        fs::rename(&partial, &target).map_err(|e| e.to_string())?;
        save_metadata(&target, name)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result?;
    job.line(format!("Snapshot saved: {}", target.display()));
    describe(&target, id, name, false)
}
fn save_metadata(path: &Path, name: &str) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(&Metadata {
        snapshot_version: 1,
        name: name.into(),
    })
    .map_err(|e| e.to_string())?;
    let target = metadata_path(path);
    let partial = target.with_extension("json.partial");
    let mut file = File::create(&partial).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(partial, target).map_err(|e| e.to_string())
}
fn describe(path: &Path, id: &str, name: &str, official: bool) -> Result<Snapshot> {
    let stem = path.file_name().unwrap_or_default().to_string_lossy();
    let (ts, version) = stem
        .trim_end_matches(".tar.zst")
        .split_once('_')
        .ok_or("Invalid snapshot filename")?;
    Ok(Snapshot {
        id: id.into(),
        name: name.into(),
        timestamp: ts.parse().map_err(|_| "Invalid timestamp")?,
        version: version.into(),
        path: path.into(),
        official,
        bytes: fs::metadata(path).map_err(|e| e.to_string())?.len(),
    })
}
fn validate_snapshot(paths: &Paths, s: &Snapshot, writing: bool) -> Result<()> {
    backend::validate_id(&s.id)?;
    if writing && s.official {
        return Err("Import this snapshot before editing it. The official Warehouse library is read-only here.".into());
    }
    let root = if s.official {
        paths.official_snapshots()
    } else {
        paths.snapshots()
    };
    let expected = root.join(&s.id);
    backend::ensure_real_dir(&root)?;
    backend::ensure_real_dir(&expected)?;
    if s.path.parent() != Some(expected.as_path())
        || !s
            .path
            .file_name()
            .is_some_and(|s| s.to_string_lossy().ends_with(".tar.zst"))
    {
        return Err("Snapshot is outside its library".into());
    }
    let meta = fs::symlink_metadata(&s.path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.is_symlink() {
        return Err("Snapshot must be a regular file".into());
    }
    Ok(())
}
fn extract(source: &Path, destination: &Path, job: &Job) -> Result<()> {
    let file = File::open(source).map_err(|e| e.to_string())?;
    let decoder = zstd::Decoder::new(file).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(false);
    archive.set_preserve_ownerships(false);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        job.check()?;
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        if path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err("Unsafe path in snapshot".into());
        }
        let kind = entry.header().entry_type();
        // tar's unpack_in protects parent symlinks and checks hard-link targets.
        if !(kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link()) {
            return Err(format!("Unsupported archive entry {}", path.display()));
        }
        if !entry
            .unpack_in(destination)
            .map_err(|e| format!("{}: {e}", path.display()))?
        {
            return Err("Archive entry escapes the destination".into());
        }
    }
    Ok(())
}
pub fn restore(paths: &Paths, s: &Snapshot, job: &Job) -> Result<String> {
    validate_snapshot(paths, s, false)?;
    let target = paths.app_data(&s.id)?;
    let parent = target.parent().ok_or("Missing data parent")?;
    private_dir(parent)?;
    let stage = parent.join(format!(
        ".warehouse-stage-{}-{}",
        std::process::id(),
        backend::now()
    ));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    if let Err(e) = extract(&s.path, &stage, job) {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    if let Err(e) = job.check() {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    let mut recovery = None;
    if fs::symlink_metadata(&target).is_ok() {
        if let Err(e) = backend::ensure_real_dir(&target) {
            let _ = fs::remove_dir_all(&stage);
            return Err(e);
        }
        // Sibling rename is atomic even if HOME and XDG_DATA_HOME use different disks.
        let backup = parent.join(format!(
            ".warehouse-recovery-{}-{}-{}",
            s.id,
            backend::now(),
            std::process::id()
        ));
        if backup.exists() {
            let _ = fs::remove_dir_all(&stage);
            return Err("Recovery directory already exists".into());
        }
        fs::rename(&target, &backup).map_err(|e| e.to_string())?;
        recovery = Some(backup);
    }
    if let Err(e) = fs::rename(&stage, &target) {
        if let Some(ref backup) = recovery {
            fs::rename(backup, &target).map_err(|rollback| {
                format!(
                    "Restore failed: {e}; rollback failed: {rollback}. Recovery: {}",
                    backup.display()
                )
            })?;
        }
        let _ = fs::remove_dir_all(&stage);
        return Err(e.to_string());
    }
    Ok(format!(
        "Restored {}. {}",
        s.id,
        recovery
            .map(|p| format!(
                "Previous data: {}. To undo, close the app and exchange this folder with {}.",
                p.display(),
                target.display()
            ))
            .unwrap_or_else(|| "There was no previous data.".into())
    ))
}
pub fn rename(paths: &Paths, s: &Snapshot, name: &str) -> Result<()> {
    validate_snapshot(paths, s, true)?;
    if name.trim().is_empty() {
        return Err("Enter a snapshot name".into());
    }
    let meta = metadata_path(&s.path);
    if meta.exists() {
        let backup = meta.with_extension(format!("json.bak-{}", backend::now()));
        fs::copy(&meta, backup).map_err(|e| e.to_string())?;
    }
    save_metadata(&s.path, name)
}
pub fn trash_snapshot(paths: &Paths, s: &Snapshot) -> Result<()> {
    validate_snapshot(paths, s, true)?;
    let meta = metadata_path(&s.path);
    if meta.exists() {
        backend::move_to_trash(&meta)?;
    }
    backend::move_to_trash(&s.path)
}
pub fn import(paths: &Paths, id: &str, source: &Path, name: &str, job: &Job) -> Result<Snapshot> {
    let target = destination(paths, id)?;
    let staging = paths.data.join(format!(
        ".import-check-{}-{}",
        std::process::id(),
        backend::now()
    ));
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let checked = extract(source, &staging, job);
    let _ = fs::remove_dir_all(&staging);
    checked?;
    let partial = target.with_extension("partial");
    let result = (|| -> Result<()> {
        let mut input = File::open(source).map_err(|e| e.to_string())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            job.check()?;
            let n = input.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&partial, &target).map_err(|e| e.to_string())?;
        save_metadata(&target, name)
    })();
    if result.is_err() {
        let _ = fs::remove_file(partial);
    }
    result?;
    describe(&target, id, name, false)
}
pub fn date(timestamp: u64) -> String {
    // Human-readable local date comes from libcosmic's jiff dependency indirectly;
    // a deterministic UTC timestamp keeps this module independent of time zones.
    let d = (timestamp / 86400) as i64 + 719468;
    let era = d.div_euclid(146097);
    let doe = d - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(month <= 2);
    format!(
        "{y:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        timestamp % 86400 / 3600,
        timestamp % 3600 / 60
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let p = Paths {
            home: t.path().join("home"),
            data: t.path().join("data"),
        };
        fs::create_dir_all(p.app_data("org.example.App").unwrap()).unwrap();
        (t, p)
    }
    #[test]
    fn own_data_snapshot_excludes_the_snapshot_library() {
        let (_temp, mut paths) = setup();
        let source = paths.app_data("org.example.App").unwrap();
        paths.data = source.join("data/warehouse-cosmic");
        fs::create_dir_all(paths.snapshots()).unwrap();
        fs::write(paths.snapshots().join("existing-archive"), b"exclude").unwrap();
        fs::write(source.join("settings"), b"keep").unwrap();
        let snapshot = create(&paths, "org.example.App", "Self", &Job::default()).unwrap();
        let decoder = zstd::Decoder::new(File::open(snapshot.path).unwrap()).unwrap();
        let mut archive = tar::Archive::new(decoder);
        let names: Vec<PathBuf> = archive
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().into_owned())
            .collect();
        assert!(names.contains(&PathBuf::from("settings")));
        assert!(
            !names
                .iter()
                .any(|path| path.starts_with("data/warehouse-cosmic/Snapshots"))
        );
    }
    #[test]
    fn snapshot_roundtrip_preserves_data_and_recovery() {
        let (_t, p) = setup();
        let data = p.app_data("org.example.App").unwrap();
        fs::write(data.join("config"), b"original").unwrap();
        std::os::unix::fs::symlink("config", data.join("link")).unwrap();
        let s = create(&p, "org.example.App", "Before change", &Job::default()).unwrap();
        assert_eq!(list(&p.snapshots(), false).unwrap().len(), 1);
        fs::write(data.join("config"), b"changed").unwrap();
        let report = restore(&p, &s, &Job::default()).unwrap();
        assert!(report.contains("Previous data"));
        assert_eq!(fs::read(data.join("config")).unwrap(), b"original");
        assert_eq!(
            fs::read_link(data.join("link")).unwrap(),
            Path::new("config")
        );
        let backup = fs::read_dir(data.parent().unwrap())
            .unwrap()
            .flatten()
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".warehouse-recovery")
            })
            .unwrap();
        assert_eq!(fs::read(backup.path().join("config")).unwrap(), b"changed");
    }
    #[test]
    fn corrupt_restore_leaves_current_data() {
        let (_t, p) = setup();
        let data = p.app_data("org.example.App").unwrap();
        fs::write(data.join("config"), b"keep").unwrap();
        let s = create(&p, "org.example.App", "Test", &Job::default()).unwrap();
        fs::write(&s.path, b"invalid zstd").unwrap();
        assert!(restore(&p, &s, &Job::default()).is_err());
        assert_eq!(fs::read(data.join("config")).unwrap(), b"keep");
    }
    #[test]
    fn cancellation_leaves_no_snapshot() {
        let (_t, p) = setup();
        let job = Job::default();
        job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(create(&p, "org.example.App", "Cancelled", &job).is_err());
        assert!(list(&p.snapshots(), false).unwrap().is_empty());
    }
    #[test]
    fn date_epoch() {
        assert_eq!(date(0), "1970-01-01 00:00 UTC");
    }
    #[test]
    fn traversal_archive_cannot_replace_data_or_escape_staging() {
        let (_t, p) = setup();
        let data = p.app_data("org.example.App").unwrap();
        fs::write(data.join("config"), "keep").unwrap();
        let s = create(&p, "org.example.App", "Unsafe", &Job::default()).unwrap();
        let encoder = zstd::Encoder::new(File::create(&s.path).unwrap(), 1).unwrap();
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o644);
        header.as_mut_bytes()[..10].copy_from_slice(b"../escaped");
        header.set_cksum();
        tar.append(&header, &b"oops"[..]).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        assert!(restore(&p, &s, &Job::default()).is_err());
        assert_eq!(fs::read_to_string(data.join("config")).unwrap(), "keep");
        assert!(!data.parent().unwrap().join("escaped").exists());
    }
    #[test]
    fn symlink_pivot_archive_cannot_write_outside_staging() {
        let (t, p) = setup();
        let outside = t.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let s = create(&p, "org.example.App", "Unsafe", &Job::default()).unwrap();
        let encoder = zstd::Encoder::new(File::create(&s.path).unwrap(), 1).unwrap();
        let mut tar = tar::Builder::new(encoder);
        let mut link = tar::Header::new_gnu();
        link.set_size(0);
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_mode(0o777);
        tar.append_link(&mut link, "pivot", &outside).unwrap();
        let mut file = tar::Header::new_gnu();
        file.set_size(4);
        file.set_mode(0o644);
        tar.append_data(&mut file, "pivot/escaped", &b"oops"[..])
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        assert!(restore(&p, &s, &Job::default()).is_err());
        assert!(!outside.join("escaped").exists());
    }
}
