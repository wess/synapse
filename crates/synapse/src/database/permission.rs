use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

pub fn prepare(path: &Path) -> Result<bool> {
    let existed = path.is_file() && fs::metadata(path).is_ok_and(|item| item.len() > 0);
    if let Some(parent) = path.parent() {
        let created = !parent.exists();
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
        if created || parent.file_name().is_some_and(|name| name == "synapse") {
            securedirectory(parent)?;
        }
    }
    if path.is_file() {
        securefile(path)?;
    }
    Ok(existed)
}

pub fn securefiles(path: &Path) -> Result<()> {
    for file in [
        path.to_path_buf(),
        sidecar(path, "wal"),
        sidecar(path, "shm"),
        lockpath(path),
    ] {
        if file.is_file() {
            securefile(&file)?;
        }
    }
    Ok(())
}

pub fn sharedlock(path: &Path) -> Result<File> {
    let file = lockfile(path)?;
    FileExt::lock_shared(&file).context("could not acquire the database lock")?;
    Ok(file)
}

pub fn exclusivelock(path: &Path) -> Result<File> {
    let file = lockfile(path)?;
    FileExt::try_lock_exclusive(&file).context(
        "Synapse is using this database; close the app and connected tools before restoring",
    )?;
    Ok(file)
}

pub fn securefile(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("could not secure {}", path.display()))?;
    }
    Ok(())
}

pub fn securedirectory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("could not secure {}", path.display()))?;
    }
    Ok(())
}

pub fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}-{suffix}", path.display()))
}

fn lockfile(path: &Path) -> Result<File> {
    let path = lockpath(path);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("could not open {}", path.display()))?;
    securefile(&path)?;
    Ok(file)
}

fn lockpath(path: &Path) -> PathBuf {
    path.with_extension("lock")
}
