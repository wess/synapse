use anyhow::{Context, Result, ensure};
use std::path::{Component, Path, PathBuf};

pub(super) fn root(path: &Path) -> Result<PathBuf> {
    crate::brain::projectroot(path)?.context("project folder does not exist")
}

pub(super) fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 48
            && value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "guide names must contain 1-48 lowercase letters or digits"
    );
    Ok(())
}

pub(super) fn validsource(relative: &str) -> Result<()> {
    let path = Path::new(relative);
    ensure!(
        !relative.is_empty()
            && relative.len() <= 512
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "sources must be repository-relative file paths without traversal"
    );
    ensure!(
        !relative.contains(['\\', ':']) && !relative.chars().any(char::is_control),
        "use forward slashes in source paths"
    );
    for part in path.components() {
        let text = part.as_os_str().to_string_lossy().to_ascii_lowercase();
        ensure!(
            text != ".git"
                && text != ".synapse"
                && !text.starts_with(".env")
                && !text.ends_with(".pem")
                && !text.ends_with(".key"),
            "credential files and Synapse metadata cannot be guide sources"
        );
    }
    Ok(())
}
