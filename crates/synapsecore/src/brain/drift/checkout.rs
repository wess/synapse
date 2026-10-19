//! Whether a path still exists. Anything that can't be checked from here (a
//! project that isn't checked out, another machine's home folder) is
//! `Unknown`, never `Missing`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Version control, build output, and dependencies. Not walked, and a path
/// through one is never reported missing.
const SKIPPED: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".next",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
];

/// Temp folders. Files there are supposed to disappear.
const TRANSIENT: &[&str] = &[
    "/tmp/",
    "/private/tmp/",
    "/var/folders/",
    "/private/var/folders/",
];

/// Past this many entries the index is incomplete and can't prove absence.
const WALKCEILING: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Present,
    Missing,
    Unknown,
}

pub struct Checkout {
    root: Option<PathBuf>,
    /// Project key memory for this root is stored under.
    key: Option<String>,
    home: Option<PathBuf>,
    /// Relative paths under `root`, built lazily. `None` if the walk was too big.
    index: OnceLock<Option<HashSet<String>>>,
}

impl Checkout {
    pub fn new(root: Option<PathBuf>, key: Option<String>, home: Option<PathBuf>) -> Self {
        Self {
            root,
            key,
            home,
            index: OnceLock::new(),
        }
    }

    pub fn at(folder: Option<&Path>) -> Self {
        let root = folder.and_then(|folder| crate::brain::projectroot(folder).ok().flatten());
        let key = root
            .as_deref()
            .and_then(|root| crate::brain::projectkey(root).ok().flatten());
        Self::new(root, key, crate::files::home().ok())
    }

    pub fn nowhere() -> Self {
        Self::new(None, None, crate::files::home().ok())
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Whether relative paths in `project` memory resolve against this root.
    pub fn holds(&self, project: &str) -> bool {
        !project.is_empty() && self.key.as_deref() == Some(project)
    }

    /// `relative` allows resolving relative paths against this checkout. Only
    /// pass true for memory from this project.
    pub fn presence(&self, reference: &str, relative: bool) -> Presence {
        // Build output comes and goes; an unbuilt binary isn't a stale memory.
        if reference
            .split(['/', '\\'])
            .any(|part| SKIPPED.contains(&part))
        {
            return Presence::Unknown;
        }
        if let Some(rest) = reference.strip_prefix("~/") {
            return match &self.home {
                Some(home) => exists(&home.join(rest)),
                None => Presence::Unknown,
            };
        }
        let path = Path::new(reference);
        if path.is_absolute() {
            return self.absolute(path);
        }
        let Some(root) = self.root.as_deref().filter(|_| relative) else {
            return Presence::Unknown;
        };
        let wanted = reference.trim_end_matches(['/', '\\']);
        if root.join(wanted).exists() {
            return Presence::Present;
        }
        // Paths are often written relative to a package, e.g.
        // `src/cli/memory.rs` for `crates/core/src/cli/memory.rs`.
        match self.index.get_or_init(|| walk(root)) {
            Some(index) => {
                let wanted = wanted.replace('\\', "/");
                let suffix = format!("/{wanted}");
                match index.contains(&wanted) || index.iter().any(|path| path.ends_with(&suffix)) {
                    true => Presence::Present,
                    false => Presence::Missing,
                }
            }
            None => Presence::Unknown,
        }
    }

    /// Another account's home is unknown: synced memory can name paths from a
    /// different machine.
    fn absolute(&self, path: &Path) -> Presence {
        let text = path.to_string_lossy();
        let own = self
            .home
            .as_deref()
            .is_some_and(|home| path.starts_with(home));
        let foreign = ["/Users/", "/home/"]
            .iter()
            .any(|prefix| text.starts_with(prefix) && !own)
            || self.home.as_deref().is_some_and(|home| {
                home.parent().is_some_and(|parent| {
                    parent
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("users") || name == "home")
                        && path.starts_with(parent)
                        && !own
                })
            });
        let transient = !own && TRANSIENT.iter().any(|prefix| text.starts_with(prefix));
        match foreign || transient {
            true => Presence::Unknown,
            false => exists(path),
        }
    }
}

fn exists(path: &Path) -> Presence {
    match path.exists() {
        true => Presence::Present,
        false => Presence::Missing,
    }
}

fn walk(root: &Path) -> Option<HashSet<String>> {
    let mut found = HashSet::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&relative)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = relative.join(&name);
            found.insert(path.to_string_lossy().replace('\\', "/"));
            if found.len() > WALKCEILING {
                return None;
            }
            let directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if directory && !SKIPPED.contains(&name.as_str()) {
                pending.push(path);
            }
        }
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Checkout) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("repo");
        let home = directory.path().join("home");
        for file in [
            "crates/core/src/cli/memory.rs",
            "docs/retrieval.md",
            "target/debug/build.log",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        std::fs::create_dir_all(home.join("Desktop")).unwrap();
        std::fs::write(home.join("Desktop/notes.md"), "").unwrap();
        let checkout = Checkout::new(Some(root), Some("github.com/a/repo".into()), Some(home));
        (directory, checkout)
    }

    #[test]
    fn a_path_from_the_root_is_present_or_missing() {
        let (_directory, checkout) = fixture();
        assert_eq!(
            checkout.presence("docs/retrieval.md", true),
            Presence::Present
        );
        assert_eq!(checkout.presence("docs/", true), Presence::Present);
        assert_eq!(checkout.presence("docs/moved.md", true), Presence::Missing);
    }

    #[test]
    fn a_path_written_from_inside_a_package_is_present() {
        let (_directory, checkout) = fixture();
        assert_eq!(
            checkout.presence("src/cli/memory.rs", true),
            Presence::Present
        );
        assert_eq!(checkout.presence("cli/memory.rs", true), Presence::Present);
        // A suffix match is by whole parts, not characters.
        assert_eq!(checkout.presence("emory.rs", true), Presence::Missing);
    }

    #[test]
    fn build_output_is_not_searched() {
        let (_directory, checkout) = fixture();
        assert_eq!(
            checkout.presence("debug/build.log", true),
            Presence::Missing
        );
    }

    #[test]
    fn build_output_being_absent_is_not_drift() {
        let (_directory, checkout) = fixture();
        assert_eq!(
            checkout.presence("target/release/app", true),
            Presence::Unknown
        );
        assert_eq!(
            checkout.presence("~/code/app/target/debug/app", false),
            Presence::Unknown
        );
        assert_eq!(
            checkout.presence("web/node_modules/react/index.js", true),
            Presence::Unknown
        );
    }

    #[test]
    fn a_relative_path_without_a_checkout_is_unknown() {
        let (_directory, checkout) = fixture();
        assert_eq!(checkout.presence("docs/moved.md", false), Presence::Unknown);
        let nowhere = Checkout::new(None, None, None);
        assert_eq!(
            nowhere.presence("docs/retrieval.md", true),
            Presence::Unknown
        );
    }

    #[test]
    fn home_paths_resolve_against_this_machines_home() {
        let (_directory, checkout) = fixture();
        assert_eq!(
            checkout.presence("~/Desktop/notes.md", false),
            Presence::Present
        );
        assert_eq!(
            checkout.presence("~/Desktop/gone.md", false),
            Presence::Missing
        );
        let homeless = Checkout::new(None, None, None);
        assert_eq!(
            homeless.presence("~/Desktop/notes.md", false),
            Presence::Unknown
        );
    }

    #[test]
    fn another_accounts_home_is_unknown_rather_than_missing() {
        let (directory, checkout) = fixture();
        assert_eq!(
            checkout.presence("/Users/somebodyelse/project/a.rs", false),
            Presence::Unknown
        );
        assert_eq!(
            checkout.presence("/home/somebodyelse/a.rs", false),
            Presence::Unknown
        );
        let own = directory.path().join("home/Desktop/notes.md");
        assert_eq!(
            checkout.presence(&own.to_string_lossy(), false),
            Presence::Present
        );
        #[cfg(windows)]
        let missing = std::env::current_dir()
            .unwrap()
            .join("definitely-not-present.rs");
        #[cfg(not(windows))]
        let missing = PathBuf::from("/definitely/not/here.rs");
        assert_eq!(
            checkout.presence(&missing.to_string_lossy(), false),
            Presence::Missing
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_absolute_paths_and_other_accounts_home() {
        let checkout = Checkout::new(None, None, Some(PathBuf::from("C:\\Users\\me")));
        assert_eq!(
            checkout.presence("C:\\Users\\somebodyelse\\project\\a.rs", false),
            Presence::Unknown
        );
        assert_eq!(
            checkout.presence("C:\\definitely\\not\\here.rs", false),
            Presence::Missing
        );
    }

    #[test]
    fn a_temporary_file_being_gone_is_not_drift() {
        let (_directory, checkout) = fixture();
        assert_eq!(
            checkout.presence("/tmp/screenshot-that-was-cleaned.png", false),
            Presence::Unknown
        );
        assert_eq!(
            checkout.presence("/private/tmp/solar-target/x", false),
            Presence::Unknown
        );
        assert_eq!(
            checkout.presence("/var/folders/ab/cd/T/file.txt", false),
            Presence::Unknown
        );
    }

    #[test]
    fn only_the_project_checked_out_here_resolves_relative_paths() {
        let (_directory, checkout) = fixture();
        assert!(checkout.holds("github.com/a/repo"));
        assert!(!checkout.holds("github.com/a/other"));
        assert!(!checkout.holds(""));
    }

    #[test]
    fn a_walk_that_is_too_large_cannot_say_missing() {
        let directory = tempfile::tempdir().unwrap();
        // Pretend the walk hit its ceiling.
        let checkout = Checkout::new(Some(directory.path().to_path_buf()), None, None);
        checkout.index.set(None).unwrap();
        assert_eq!(
            checkout.presence("nowhere/at/all.rs", true),
            Presence::Unknown
        );
    }
}
