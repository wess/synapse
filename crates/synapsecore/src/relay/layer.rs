//! The layered TOML store roles and teams share: project → user → built-in
//! resolution, plus name validation.
//!
//! The project layer is a repository's `.synapse/<kind>/`, so a role can travel
//! with the checkout. The user layer sits beside `SOUL.md` in the Synapse data
//! directory, which keeps it overridable by `SYNAPSE_DATA` in tests. Built-ins
//! are embedded in the binary and can never be edited in place — editing one
//! copies it down to a layer you own first.
//!
//! A project file is used only once its exact contents are approved, and an
//! edit takes the approval away. These files are not settings: a tool
//! descriptor names the program Synapse runs and the arguments it is given, and
//! a role is the brief and the permissions an agent starts with. A checkout is
//! somebody else's text until its owner has read it, which is the same bargain
//! a `.synapse.yaml` and a project's sources file already make — and without
//! it, cloning a repository and typing `synapse launch claude` runs whatever
//! that repository said `claude` was.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
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

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Where the approval of one project file is written down: a file named for
/// where the project file sits, holding the digest of what it said.
///
/// Files rather than rows in the store's trust table because everything that
/// resolves a layer is synchronous and most of it has no database open. Keyed
/// on the folder resolved and the name as written, like the trust table: a
/// link is approved where it is, not where it points.
fn record(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .with_context(|| format!("{} does not name a file", path.display()))?;
    let folder = path
        .parent()
        .context("a project file has a folder")?
        .canonicalize()
        .with_context(|| format!("could not resolve {}", path.display()))?;
    let place = folder.join(name).display().to_string();
    Ok(crate::files::data()?.join("approved").join(digest(&place)))
}

/// Whether this project file was approved saying exactly this.
pub fn approved(path: &Path, text: &str) -> bool {
    record(path)
        .ok()
        .and_then(|record| std::fs::read_to_string(record).ok())
        .is_some_and(|stored| stored.trim() == digest(text))
}

fn approve(path: &Path, text: &str) -> Result<()> {
    let record = record(path)?;
    let folder = record.parent().context("an approval has a folder")?;
    std::fs::create_dir_all(folder)
        .with_context(|| format!("could not create {}", folder.display()))?;
    crate::database::securedirectory(folder)?;
    std::fs::write(&record, digest(text))
        .with_context(|| format!("could not record approval of {}", path.display()))?;
    crate::database::securefile(&record)
}

/// Approve a project file as it reads now, returning its path. The next edit
/// to it — anybody's — makes it unapproved again.
pub fn allow(kind: &str, root: &Path, name: &str) -> Result<PathBuf> {
    valid(name)?;
    let path = file(&projectdirectory(root, kind), name);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("this project has no {}", path.display()))?;
    approve(&path, &text)?;
    Ok(path)
}

/// Project files of this kind that are present and not approved as they stand,
/// so a surface can say they are being ignored rather than leave somebody to
/// wonder why their role did nothing.
pub fn pending(kind: &str, root: Option<&Path>) -> Vec<String> {
    let Some(root) = root else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries(&projectdirectory(root, kind))
        .into_iter()
        .filter(|(_, path)| std::fs::read_to_string(path).is_ok_and(|text| !approved(path, &text)))
        .map(|(name, _)| name)
        .collect();
    names.sort();
    names
}

/// What to say when `name` did not resolve: that there is no such thing, or
/// that this project has one waiting to be read.
pub fn unknown(label: &str, kind: &str, root: Option<&Path>, name: &str, verb: &str) -> String {
    match pending(kind, root).iter().any(|item| item == name) {
        true => format!(
            "this project's {label} `{name}` has not been approved as it is written; \
             read .synapse/{kind}/{name}.toml, then run `synapse {verb} allow {name}`"
        ),
        false => format!("no {label} named `{name}`"),
    }
}

/// Every `.toml` in a layer's folder, by the name it would resolve under.
fn entries(directory: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("toml"))
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_owned();
            Some((stem, path))
        })
        .collect()
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
        let path = file(&directory, name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            // Unapproved, it is not there: the name falls through to a layer
            // this user owns, or to nothing.
            if source == Source::Project && !approved(&path, &text) {
                continue;
            }
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
        for (name, path) in entries(&directory) {
            if source == Source::Project
                && !std::fs::read_to_string(&path).is_ok_and(|text| approved(&path, &text))
            {
                continue;
            }
            seen.insert(name, source);
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
    // Written through Synapse by the person whose project it is, which is the
    // reading an approval stands for. What is recorded is what landed on disk.
    if !user {
        let written = std::fs::read_to_string(&path)
            .with_context(|| format!("could not read back {}", path.display()))?;
        approve(&path, &written)?;
    }
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

    /// A checkout and a data folder, with the data folder held for the test.
    fn project() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
        let directory = tempfile::tempdir().unwrap();
        let guard = crate::files::scopeddata(&directory.path().join("data"));
        std::fs::create_dir_all(projectdirectory(&directory.path().join("repo"), "roles")).unwrap();
        (directory, guard)
    }

    #[test]
    fn the_project_layer_wins_over_the_builtin_once_it_is_approved() {
        let (directory, _data) = project();
        let root = directory.path().join("repo");
        let path = file(&projectdirectory(&root, "roles"), "worker");
        std::fs::write(&path, "name = \"mine\"\n").unwrap();

        // Somebody else's text until its owner has read it.
        let (_, source) = read("roles", BUILTINS, Some(&root), "worker").unwrap();
        assert_eq!(source, Source::Builtin);
        assert_eq!(pending("roles", Some(&root)), ["worker"]);

        allow("roles", &root, "worker").unwrap();
        let (text, source) = read("roles", BUILTINS, Some(&root), "worker").unwrap();
        assert_eq!(text, "name = \"mine\"\n");
        assert_eq!(source, Source::Project);
        assert!(pending("roles", Some(&root)).is_empty());

        // And somebody else's again the moment it says something different.
        std::fs::write(&path, "name = \"theirs\"\n").unwrap();
        let (text, source) = read("roles", BUILTINS, Some(&root), "worker").unwrap();
        assert_eq!(text, "name = \"worker\"\n");
        assert_eq!(source, Source::Builtin);
    }

    #[test]
    fn a_project_file_nobody_approved_is_not_there_and_says_why() {
        let (directory, _data) = project();
        let root = directory.path().join("repo");
        std::fs::write(file(&projectdirectory(&root, "roles"), "theirs"), "x = 1\n").unwrap();

        assert!(read("roles", BUILTINS, Some(&root), "theirs").is_none());
        assert!(!scan("roles", BUILTINS, Some(&root)).contains_key("theirs"));
        let message = unknown("role", "roles", Some(&root), "theirs", "relay role");
        assert!(message.contains("has not been approved"), "{message}");
        assert!(
            message.contains("synapse relay role allow theirs"),
            "{message}"
        );
        assert_eq!(
            unknown("role", "roles", Some(&root), "nobody", "relay role"),
            "no role named `nobody`"
        );
    }

    #[test]
    fn approving_one_checkout_does_not_approve_the_same_file_in_another() {
        let (directory, _data) = project();
        let (root, clone) = (
            directory.path().join("repo"),
            directory.path().join("clone"),
        );
        std::fs::create_dir_all(projectdirectory(&clone, "roles")).unwrap();
        for at in [&root, &clone] {
            std::fs::write(file(&projectdirectory(at, "roles"), "mine"), "x = 1\n").unwrap();
        }
        allow("roles", &root, "mine").unwrap();

        assert!(read("roles", BUILTINS, Some(&root), "mine").is_some());
        assert!(read("roles", BUILTINS, Some(&clone), "mine").is_none());
    }

    #[test]
    fn a_file_saved_through_synapse_into_the_project_is_already_approved() {
        let (directory, _data) = project();
        let root = directory.path().join("repo");

        save("roles", "mine", false, &root, "x = 1\n").unwrap();

        let (_, source) = read("roles", BUILTINS, Some(&root), "mine").unwrap();
        assert_eq!(source, Source::Project);
        assert!(allow("roles", &root, "missing").is_err());
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
        let (directory, _data) = project();
        let root = directory.path().join("repo");
        for name in ["worker", "mine"] {
            std::fs::write(file(&projectdirectory(&root, "roles"), name), "").unwrap();
            allow("roles", &root, name).unwrap();
        }

        let seen = scan("roles", BUILTINS, Some(&root));

        assert_eq!(seen.get("worker"), Some(&Source::Project));
        assert_eq!(seen.get("mine"), Some(&Source::Project));
    }
}
