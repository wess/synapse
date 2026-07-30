//! The layered TOML store roles and teams share: project → user → built-in
//! resolution, plus name validation.
//!
//! The project layer is a repository's `.synapse/<kind>/`, so a role can travel
//! with the checkout. The user layer sits beside `SOUL.md` in the Synapse data
//! directory, which keeps it overridable by `SYNAPSE_DATA` in tests. Built-ins
//! are embedded in the binary and can never be edited in place — editing one
//! copies it down to a layer you own first.

use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Project,
    User,
    Builtin,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::User => "user",
            Self::Builtin => "built-in",
        }
    }
}

/// Names become file names and appear in prompts, so they stay lowercase and
/// predictable across layers.
pub fn valid(name: &str) -> Result<()> {
    anyhow::ensure!(!name.is_empty(), "a name cannot be empty");
    anyhow::ensure!(
        name.bytes()
            .all(|item| item.is_ascii_lowercase() || item.is_ascii_digit() || item == b'-'),
        "`{name}` may only contain lowercase letters, digits, and dashes"
    );
    Ok(())
}

pub fn projectdirectory(root: &Path, kind: &str) -> PathBuf {
    root.join(".synapse").join(kind)
}

pub fn userdirectory(kind: &str) -> Result<PathBuf> {
    Ok(crate::files::data()?.join(kind))
}

pub fn file(directory: &Path, name: &str) -> PathBuf {
    directory.join(format!("{name}.toml"))
}

/// Read `name` from the highest layer that has it, returning the text and where
/// it came from. Built-ins are the floor.
pub fn read(
    kind: &str,
    builtins: &[(&str, &'static str)],
    root: Option<&Path>,
    name: &str,
) -> Option<(String, Source)> {
    let mut candidates = Vec::new();
    if let Some(root) = root {
        candidates.push((projectdirectory(root, kind), Source::Project));
    }
    if let Ok(user) = userdirectory(kind) {
        candidates.push((user, Source::User));
    }
    for (directory, source) in candidates {
        if let Ok(text) = std::fs::read_to_string(file(&directory, name)) {
            return Some((text, source));
        }
    }
    builtins
        .iter()
        .find(|(item, _)| *item == name)
        .map(|(_, text)| ((*text).to_owned(), Source::Builtin))
}

/// Every name across the layers, with the highest layer winning the label.
pub fn scan(
    kind: &str,
    builtins: &[(&str, &'static str)],
    root: Option<&Path>,
) -> BTreeMap<String, Source> {
    let mut seen = BTreeMap::new();
    for (name, _) in builtins {
        seen.insert((*name).to_owned(), Source::Builtin);
    }
    let mut directories = Vec::new();
    if let Ok(user) = userdirectory(kind) {
        directories.push((user, Source::User));
    }
    if let Some(root) = root {
        directories.push((projectdirectory(root, kind), Source::Project));
    }
    for (directory, source) in directories {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
                seen.insert(stem.to_owned(), source);
            }
        }
    }
    seen
}

/// Write `text` into the layer the caller chose, refusing to touch anything but
/// a `.toml` under that layer's directory.
pub fn save(kind: &str, name: &str, user: bool, root: &Path, text: &str) -> Result<PathBuf> {
    valid(name)?;
    let directory = if user {
        userdirectory(kind)?
    } else {
        projectdirectory(root, kind)
    };
    let path = file(&directory, name);
    crate::files::write(&path, text)?;
    Ok(path)
}

/// Remove a name from the layer the caller chose. Built-ins have no file, so a
/// missing one is reported rather than silently accepted.
pub fn remove(kind: &str, name: &str, user: bool, root: &Path) -> Result<PathBuf> {
    valid(name)?;
    let directory = if user {
        userdirectory(kind)?
    } else {
        projectdirectory(root, kind)
    };
    let path = file(&directory, name);
    anyhow::ensure!(path.is_file(), "{} does not exist", path.display());
    std::fs::remove_file(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTINS: &[(&str, &str)] = &[("worker", "name = \"worker\"\n")];

    #[test]
    fn names_stay_lowercase_and_predictable() {
        assert!(valid("frontend").is_ok());
        assert!(valid("build-2").is_ok());
        assert!(valid("").is_err());
        assert!(valid("Frontend").is_err());
        assert!(valid("front end").is_err());
    }

    #[test]
    fn the_project_layer_wins_over_the_builtin() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir_all(projectdirectory(root, "roles")).unwrap();
        std::fs::write(
            file(&projectdirectory(root, "roles"), "worker"),
            "name = \"mine\"\n",
        )
        .unwrap();

        let (text, source) = read("roles", BUILTINS, Some(root), "worker").unwrap();

        assert_eq!(text, "name = \"mine\"\n");
        assert_eq!(source, Source::Project);
    }

    #[test]
    fn a_builtin_is_the_floor_when_no_layer_owns_the_name() {
        let directory = tempfile::tempdir().unwrap();
        let (_, source) = read("roles", BUILTINS, Some(directory.path()), "worker").unwrap();

        assert_eq!(source, Source::Builtin);
        assert!(read("roles", BUILTINS, Some(directory.path()), "missing").is_none());
    }

    #[test]
    fn scanning_labels_each_name_with_its_highest_layer() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir_all(projectdirectory(root, "roles")).unwrap();
        std::fs::write(file(&projectdirectory(root, "roles"), "worker"), "").unwrap();
        std::fs::write(file(&projectdirectory(root, "roles"), "mine"), "").unwrap();

        let seen = scan("roles", BUILTINS, Some(root));

        assert_eq!(seen.get("worker"), Some(&Source::Project));
        assert_eq!(seen.get("mine"), Some(&Source::Project));
    }
}
