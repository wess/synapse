use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallStatus {
    Missing,
    Installed(PathBuf),
    Conflict(PathBuf),
}

pub fn destination() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SYNAPSE_BIN").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    {
        return Ok(crate::files::data()?.join("bin").join("synapse.exe"));
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(crate::files::home()?
            .join(".local")
            .join("bin")
            .join("synapse"))
    }
}

pub fn status() -> Result<InstallStatus> {
    let target = destination()?;
    if !target.exists() && fs::symlink_metadata(&target).is_err() {
        return Ok(InstallStatus::Missing);
    }
    let executable = std::env::current_exe()?.canonicalize()?;
    let source = installedsource(&target)?;
    let samefile = source
        .as_ref()
        .and_then(|value| value.canonicalize().ok())
        .as_ref()
        == Some(&executable);
    let targetdigest = digest(&target).ok();
    let receipt = fs::read_to_string(receiptpath(&target)).ok();
    let managed = targetdigest
        .as_ref()
        .zip(receipt.as_deref())
        .is_some_and(|(digest, receipt)| digest == receipt.trim());
    let currentlauncher = !bundled(&executable)
        || fs::read_to_string(&target).ok().as_deref() == Some(&launcher(&executable));
    if samefile
        || managed && currentlauncher
        || targetdigest.as_ref() == Some(&digest(&executable)?)
    {
        Ok(InstallStatus::Installed(target))
    } else if managed {
        Ok(InstallStatus::Missing)
    } else {
        Ok(InstallStatus::Conflict(target))
    }
}

pub fn install() -> Result<PathBuf> {
    let target = destination()?;
    match status()? {
        InstallStatus::Conflict(path) => {
            anyhow::bail!(
                "{} already exists and is not this Synapse executable",
                path.display()
            )
        }
        InstallStatus::Installed(_) | InstallStatus::Missing => {}
    }
    let parent = target
        .parent()
        .context("CLI destination has no parent folder")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("could not create {}", parent.display()))?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let targetstate = crate::files::Snapshot::capture(&target)?;
    let receipt = receiptpath(&target);
    let receiptstate = crate::files::Snapshot::capture(&receipt)?;
    let result = (|| {
        installtarget(&executable, &target)?;
        crate::files::write(&receipt, &format!("{}\n", digest(&target)?))?;
        Ok(())
    })();
    if let Err(error) = result {
        if let Err(rollback) = receiptstate.restore().and_then(|_| targetstate.restore()) {
            return Err(error).context(format!("install rollback also failed: {rollback:#}"));
        }
        return Err(error);
    }
    Ok(target)
}

fn installtarget(executable: &Path, target: &Path) -> Result<()> {
    if bundled(executable) {
        anyhow::ensure!(
            !executable.starts_with("/Volumes"),
            "move Synapse out of the mounted image before installing its CLI"
        );
        crate::files::write(target, &launcher(executable))?;
        executablepermissions(target)?;
        return Ok(());
    }
    crate::files::atomiccopy(executable, target)
        .with_context(|| format!("could not install {}", target.display()))
}

fn launcher(executable: &Path) -> String {
    let quoted = executable.to_string_lossy().replace('\'', "'\\''");
    format!("#!/bin/sh\nexec '{quoted}' \"$@\"\n")
}

fn bundled(executable: &Path) -> bool {
    executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .is_some_and(|path| path.extension().is_some_and(|value| value == "app"))
}

#[cfg(unix)]
fn executablepermissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("could not make {} executable", path.display()))
}

#[cfg(not(unix))]
fn executablepermissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn installedsource(target: &Path) -> Result<Option<PathBuf>> {
    let metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        let source = std::fs::read_link(target)?;
        return Ok(Some(if source.is_absolute() {
            source
        } else {
            target
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(source)
        }));
    }
    Ok(Some(target.to_path_buf()))
}

fn receiptpath(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_default();
    name.push(".synapsereceipt");
    path.with_file_name(name)
}

fn digest(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn bundled_install_uses_an_executable_launcher() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let directory = tempfile::tempdir().unwrap();
        let executable = directory
            .path()
            .join("quoted'.app")
            .join("contents")
            .join("macos")
            .join("synapse");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, "#!/bin/sh\nprintf 'launched %s\\n' \"$1\"\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let target = directory.path().join("bin").join("synapse");

        installtarget(&executable, &target).unwrap();

        assert!(bundled(&executable));
        let output = Command::new(&target).arg("success").output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "launched success\n"
        );
    }
}
