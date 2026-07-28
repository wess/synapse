use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn copy(source: &Path, target: &Path) -> Result<()> {
    let content =
        fs::read(source).with_context(|| format!("could not read {}", source.display()))?;
    let metadata =
        fs::metadata(source).with_context(|| format!("could not inspect {}", source.display()))?;
    replace(target, &content, Some(&metadata))
}

pub fn replace(path: &Path, content: &[u8], source: Option<&fs::Metadata>) -> Result<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;

    let (temporary, mut file) = temporary(parent)?;
    let result = (|| {
        permissions(&file, source)?;
        file.write_all(content)
            .with_context(|| format!("could not write {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("could not sync {}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, path)
            .with_context(|| format!("could not replace {}", path.display()))?;
        sync(parent)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn temporary(parent: &Path) -> Result<(PathBuf, File)> {
    for _ in 0..100 {
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!(".synapsetemp{}{}", std::process::id(), number);
        let path = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not create a temporary file in {}", parent.display())
                });
            }
        }
    }
    anyhow::bail!(
        "could not create a unique temporary file in {}",
        parent.display()
    )
}

#[cfg(unix)]
fn permissions(file: &File, source: Option<&fs::Metadata>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = source
        .map(|metadata| metadata.permissions().mode())
        .unwrap_or(0o600);
    file.set_permissions(fs::Permissions::from_mode(mode))
        .context("could not preserve file permissions")
}

#[cfg(not(unix))]
fn permissions(_file: &File, _source: Option<&fs::Metadata>) -> Result<()> {
    Ok(())
}

fn sync(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("could not sync {}", path.display()))
}
