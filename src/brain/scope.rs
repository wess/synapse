use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn projectroot(path: &Path) -> Result<Option<PathBuf>> {
    let candidate = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let candidate = candidate
        .canonicalize()
        .with_context(|| format!("could not resolve project path {}", path.display()))?;
    let root = candidate
        .ancestors()
        .find(|ancestor| {
            ancestor.join(".git").exists() || ancestor.join(crate::vault::CONFIG).is_file()
        })
        .map(Path::to_path_buf)
        .unwrap_or(candidate);
    Ok(Some(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_paths_resolve_to_the_project_marker() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("src").join("feature");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir(directory.path().join(".git")).unwrap();

        assert_eq!(
            projectroot(&nested).unwrap(),
            Some(directory.path().canonicalize().unwrap())
        );
    }

    #[test]
    fn unmarked_folders_are_still_valid_explicit_projects() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            projectroot(directory.path()).unwrap(),
            Some(directory.path().canonicalize().unwrap())
        );
    }
}
