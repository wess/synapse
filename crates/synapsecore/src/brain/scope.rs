use anyhow::Result;
use std::path::{Path, PathBuf};

/// The project `path` belongs to: the closest ancestor carrying a `.git` or a
/// `.synapse.yaml`, or the folder itself when it carries neither.
///
/// A path that does not resolve is not an error, it is the absence of a
/// project. A session can be pointed at a folder that has been deleted or was
/// never there — a worker given a directory that does not exist, a session
/// whose folder moved — and scoping is not the part of remembering that should
/// fail: recall falls back to global memory, and a project-scoped write says
/// plainly that it has no project to file itself under.
pub fn projectroot(path: &Path) -> Result<Option<PathBuf>> {
    let candidate = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let Ok(candidate) = candidate.canonicalize() else {
        return Ok(None);
    };
    let root = candidate
        .ancestors()
        .find(|ancestor| {
            ancestor.join(".git").exists() || ancestor.join(crate::vault::CONFIG).is_file()
        })
        .map(Path::to_path_buf)
        .unwrap_or(candidate);
    Ok(Some(root))
}

/// The key a project's memory is filed and recalled under: the repository's
/// normalized `origin` when it has one, so every checkout of it on every
/// machine shares one memory, and its root folder otherwise.
///
/// Every place that turns a path into the `project` column goes through here.
/// One that called `projectroot` and kept the path would file memory where no
/// recall looks for it.
pub fn projectkey(path: &Path) -> Result<Option<String>> {
    Ok(projectroot(path)?.map(|root| keyof(&root)))
}

/// The key for a folder already known to be a project root.
pub(super) fn keyof(root: &Path) -> String {
    super::origin::origin(root).unwrap_or_else(|| root.display().to_string())
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
    fn a_repository_with_an_origin_is_keyed_by_it_from_any_folder_inside() {
        let directory = tempfile::tempdir().unwrap();
        let git = directory.path().join(".git");
        std::fs::create_dir(&git).unwrap();
        std::fs::write(
            git.join("config"),
            "[remote \"origin\"]\n\turl = git@github.com:wess/app.git\n",
        )
        .unwrap();
        let nested = directory.path().join("src");
        std::fs::create_dir(&nested).unwrap();

        assert_eq!(
            projectkey(&nested).unwrap().as_deref(),
            Some("github.com/wess/app")
        );
    }

    #[test]
    fn a_repository_without_an_origin_is_keyed_by_its_folder() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join(".git")).unwrap();

        assert_eq!(
            projectkey(directory.path()).unwrap(),
            Some(
                directory
                    .path()
                    .canonicalize()
                    .unwrap()
                    .display()
                    .to_string()
            )
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

    /// A session pointed at a folder that is not there has no project, which
    /// recall reads as "global only" rather than as a reason to fail.
    #[test]
    fn a_folder_that_is_not_there_is_no_project_rather_than_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("gone").join("deeper");

        assert_eq!(projectroot(&missing).unwrap(), None);
    }

    /// And a project-scoped write still refuses rather than filing itself under
    /// an empty project, which every other session with no project would read.
    #[test]
    fn a_project_scoped_write_still_needs_a_project() {
        use crate::brain::MemoryScope;

        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("gone");

        assert_eq!(MemoryScope::Global.project(Some(&missing)).unwrap(), "");
        let error = MemoryScope::Project
            .project(Some(&missing))
            .unwrap_err()
            .to_string();
        assert!(error.contains("needs a project path"), "got {error}");
    }
}
