use crate::files::atomic;
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Snapshot {
    path: PathBuf,
    target: PathBuf,
    link: Option<PathBuf>,
    content: Option<Vec<u8>>,
    metadata: Option<fs::Metadata>,
}

impl Snapshot {
    pub fn capture(path: &Path) -> Result<Self> {
        let node = match fs::symlink_metadata(path) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| format!("could not inspect {}", path.display()));
            }
        };
        let link = node
            .as_ref()
            .filter(|metadata| metadata.file_type().is_symlink())
            .map(|_| fs::read_link(path))
            .transpose()
            .with_context(|| format!("could not read the link at {}", path.display()))?;
        let target = if link.is_some() {
            fs::canonicalize(path)
                .with_context(|| format!("could not resolve the link at {}", path.display()))?
        } else {
            path.to_path_buf()
        };
        let content = match fs::read(&target) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| format!("could not read {}", target.display()));
            }
        };
        let metadata = if content.is_some() {
            Some(
                fs::metadata(&target)
                    .with_context(|| format!("could not inspect {}", target.display()))?,
            )
        } else {
            None
        };
        Ok(Self {
            path: path.to_path_buf(),
            target,
            link,
            content,
            metadata,
        })
    }

    pub fn backup(&self) -> Result<()> {
        if let Some(content) = &self.content {
            atomic::replace(&backuppath(&self.path), content, self.metadata.as_ref())
                .with_context(|| format!("could not back up {}", self.path.display()))?;
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        if let Some(content) = &self.content {
            atomic::replace(&self.target, content, self.metadata.as_ref())
                .with_context(|| format!("could not restore {}", self.target.display()))?;
            if let Some(link) = &self.link {
                restorelink(&self.path, link)?;
            }
            return Ok(());
        }
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("could not remove {}", self.path.display()))
            }
        }
    }
}

#[cfg(unix)]
fn restorelink(path: &Path, link: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    if fs::read_link(path).ok().as_deref() == Some(link) {
        return Ok(());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for number in 0..100 {
        let temporary = parent.join(format!(".synapselink{}{}", std::process::id(), number));
        match symlink(link, &temporary) {
            Ok(()) => {
                if let Err(error) = fs::rename(&temporary, path) {
                    let _ = fs::remove_file(&temporary);
                    return Err(error).with_context(|| {
                        format!("could not restore the link at {}", path.display())
                    });
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("could not restore the link at {}", path.display()));
            }
        }
    }
    anyhow::bail!("could not restore the link at {}", path.display())
}

#[cfg(not(unix))]
fn restorelink(_path: &Path, _link: &Path) -> Result<()> {
    anyhow::bail!("restoring configuration links is not supported on this platform")
}

fn backuppath(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".synapsebackup");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn restores_a_link_replaced_by_an_external_writer() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("dotfiles.json");
        let path = directory.path().join("settings.json");
        fs::write(&target, "{\"kept\":true}\n").unwrap();
        symlink(&target, &path).unwrap();
        let snapshot = Snapshot::capture(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, "{\"changed\":true}\n").unwrap();
        fs::write(&target, "{\"changed\":true}\n").unwrap();

        snapshot.restore().unwrap();

        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "{\"kept\":true}\n");
    }
}
