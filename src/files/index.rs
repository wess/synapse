use crate::files::{atomic, validate};
use anyhow::{Context, Result};
use directories::BaseDirs;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn home() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SYNAPS_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .context("could not locate the home directory")
}

pub fn data() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SYNAPS_DATA").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    BaseDirs::new()
        .map(|dirs| dirs.data_local_dir().join("synaps"))
        .context("could not locate the application data directory")
}

pub fn database() -> Result<PathBuf> {
    Ok(data()?.join("brain.db"))
}

pub fn ensure(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    if !path.exists() {
        let initial = match path.extension().and_then(|value| value.to_str()) {
            Some("json") => "{}\n",
            _ => "",
        };
        write(path, initial)?;
    }
    Ok(())
}

pub fn read(path: &Path) -> Result<String> {
    ensure(path)?;
    fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))
}

pub fn write(path: &Path, content: &str) -> Result<()> {
    validate::content(path, content)?;
    let target = writetarget(path)?;

    let previous = match fs::read(&target) {
        Ok(value) if value == content.as_bytes() => return Ok(()),
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", target.display()));
        }
    };
    let metadata = match fs::metadata(&target) {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("could not inspect {}", target.display()));
        }
    };

    if let Some(previous) = previous {
        let backup = backuppath(path);
        atomic::replace(&backup, &previous, metadata.as_ref())
            .with_context(|| format!("could not back up {}", path.display()))?;
    }

    atomic::replace(&target, content.as_bytes(), metadata.as_ref())
        .with_context(|| format!("could not save {}", path.display()))
}

fn writetarget(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)
            .with_context(|| format!("could not resolve the link at {}", path.display())),
        Ok(_) => Ok(path.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(error).with_context(|| format!("could not inspect {}", path.display())),
    }
}

fn backuppath(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_default();
    name.push(".synapsbackup");
    path.with_file_name(name)
}

pub fn reveal(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("could not create {}", path.display()))?;

    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(path).status();

    #[cfg(target_os = "windows")]
    let status = Command::new("explorer").arg(path).status();

    #[cfg(all(unix, not(target_os = "macos")))]
    let status = Command::new("xdg-open").arg(path).status();

    let status = status.with_context(|| format!("could not reveal {}", path.display()))?;
    anyhow::ensure!(
        status.success(),
        "the operating system could not reveal {}",
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_json_files_start_valid() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        ensure(&path).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "{}\n");
    }

    #[test]
    fn text_roundtrips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.md");
        write(&path, "# Notes\n").unwrap();
        assert_eq!(read(&path).unwrap(), "# Notes\n");
    }

    #[test]
    fn overwrites_atomically_with_a_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        write(&path, "{\"version\":1}\n").unwrap();
        write(&path, "{\"version\":2}\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"version\":2}\n");
        assert_eq!(
            fs::read_to_string(backuppath(&path)).unwrap(),
            "{\"version\":1}\n"
        );
    }

    #[test]
    fn invalid_content_does_not_touch_the_file_or_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        write(&path, "{\"version\":1}\n").unwrap();
        write(&path, "{\"version\":2}\n").unwrap();
        let backup = backuppath(&path);

        assert!(write(&path, "{").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"version\":2}\n");
        assert_eq!(fs::read_to_string(backup).unwrap(), "{\"version\":1}\n");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        write(&path, "version = 1\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        write(&path, "version = 2\n").unwrap();

        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_symlinks_without_replacing_them() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("dotfiles.toml");
        let path = directory.path().join("config.toml");
        fs::write(&target, "version = 1\n").unwrap();
        symlink(&target, &path).unwrap();

        write(&path, "version = 2\n").unwrap();

        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(target).unwrap(), "version = 2\n");
        assert_eq!(
            fs::read_to_string(directory.path().join("config.toml.synapsbackup")).unwrap(),
            "version = 1\n"
        );
    }
}
