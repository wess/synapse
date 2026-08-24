//! The one place skills live.
//!
//! Copying a skill into each tool by hand is how two copies drift apart. The
//! library is the source of truth; every connected tool gets a copy from it,
//! and Synapse remembers which copies are its own.
//!
//! It has shelves. The library root holds skills that are true everywhere; a
//! `@`-prefixed directory beside them holds one project's, with the root it
//! belongs to written in a `.root` file so the shelf can say whose it is. The
//! prefix is not decoration: a skill name can never begin with `@`, so a shelf
//! and a skill can never be mistaken for each other.

use crate::skill::model::{self, ENTRY, Shelf, Skill};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Skills that ship with Synapse. They are written into the library on first
/// use, and are ordinary editable skills from then on.
const BUILTINS: &[(&str, &str)] = &[(
    "synapse-mesh",
    include_str!("../../assets/skills/synapse-mesh/SKILL.md"),
)];

/// What marks a directory in the library as a project's shelf rather than a
/// skill. `model::validname` forbids it in a skill name, so the two sets cannot
/// overlap however the library is edited by hand.
const SHELFMARK: char = '@';

/// The file inside a shelf naming the project it belongs to. A dotfile, so
/// `model::contents` already leaves it out of every listing and digest.
const ROOT: &str = ".root";

pub const TEMPLATE: &str = "---\nname: {name}\ndescription: Say what this skill does and when an agent should reach for it. Lead with the case that matters most, because agents read this line to decide whether to load the rest.\n---\n\n## When to use this\n\nDescribe the situation that should trigger the skill.\n\n## Steps\n\n1. The first thing to do.\n2. The next thing.\n";

/// The library root, which is also the global shelf.
pub fn directory() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("skills"))
}

/// Where one shelf keeps its skills.
pub fn shelfpath(shelf: &Shelf) -> Result<PathBuf> {
    let root = directory()?;
    Ok(match shelf {
        Shelf::Global => root,
        Shelf::Project { slug, .. } => root.join(format!("{SHELFMARK}{slug}")),
    })
}

pub fn path(shelf: &Shelf, name: &str) -> Result<PathBuf> {
    model::validname(name)?;
    Ok(shelfpath(shelf)?.join(name))
}

/// Create a project's shelf if it does not exist, recording which project it is
/// for. Called before anything is written to one, and never for the global
/// shelf, which is the library root itself.
pub fn ensureshelf(shelf: &Shelf) -> Result<()> {
    let Shelf::Project { root, .. } = shelf else {
        return Ok(());
    };
    let marker = shelfpath(shelf)?.join(ROOT);
    if marker.is_file() {
        return Ok(());
    }
    crate::files::write(&marker, &format!("{root}\n"))
        .with_context(|| format!("could not create the shelf for {root}"))
}

/// Write the shipped skills into the library if they are not there yet. Like
/// `SOUL.md`, they are created once and never overwritten afterwards, so an
/// edited copy stays edited.
pub fn ensure() -> Result<()> {
    for (name, content) in BUILTINS {
        let root = path(&Shelf::Global, name)?;
        if root.join(ENTRY).exists() {
            continue;
        }
        crate::files::write(&root.join(ENTRY), content)
            .with_context(|| format!("could not create the built-in skill `{name}`"))?;
    }
    Ok(())
}

/// Every shelf the library holds: the global one, then each project's in a
/// stable order. A `@` directory with no `.root` is skipped rather than
/// guessed at — a shelf that cannot say whose it is cannot be installed
/// anywhere.
pub fn shelves() -> Result<Vec<Shelf>> {
    let mut found = vec![Shelf::Global];
    let root = directory()?;
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", root.display()));
        }
    };
    let mut projects = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let Some(slug) = name
            .to_str()
            .and_then(|value| value.strip_prefix(SHELFMARK))
        else {
            continue;
        };
        let Ok(marker) = std::fs::read_to_string(entry.path().join(ROOT)) else {
            continue;
        };
        projects.push(Shelf::Project {
            slug: slug.to_owned(),
            root: marker.trim().to_owned(),
        });
    }
    projects.sort_by(|left, right| left.key().cmp(right.key()));
    found.extend(projects);
    Ok(found)
}

/// The shelf a project root resolves to, whether or not it exists yet.
pub fn shelffor(root: Option<&Path>) -> Shelf {
    root.map(Shelf::project).unwrap_or(Shelf::Global)
}

/// Every skill in the library across every shelf, with anything unreadable
/// reported rather than silently skipped.
pub fn all() -> Result<(Vec<Skill>, Vec<String>)> {
    ensure()?;
    let mut skills = Vec::new();
    let mut problems = Vec::new();
    for shelf in shelves()? {
        let (found, trouble) = shelved(&shelf)?;
        skills.extend(found);
        problems.extend(trouble);
    }
    Ok((skills, problems))
}

/// One shelf's skills, sorted by name.
pub fn shelved(shelf: &Shelf) -> Result<(Vec<Skill>, Vec<String>)> {
    let root = shelfpath(shelf)?;
    let mut skills = Vec::new();
    let mut problems = Vec::new();
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((skills, problems));
        }
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", root.display()));
        }
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') || name.starts_with(SHELFMARK) {
            continue;
        }
        match read(shelf, name) {
            Ok(skill) => skills.push(skill),
            Err(error) => problems.push(match shelf {
                Shelf::Global => format!("{name}: {error:#}"),
                Shelf::Project { root, .. } => format!("{name} ({root}): {error:#}"),
            }),
        }
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((skills, problems))
}

pub fn read(shelf: &Shelf, name: &str) -> Result<Skill> {
    let root = path(shelf, name)?;
    anyhow::ensure!(
        root.join(ENTRY).is_file(),
        "no skill named `{name}` in the {} library",
        shelf.label()
    );
    model::read(&root, shelf.clone())
}

/// Find a skill by name the way a person means it: this project's copy first,
/// then the global one. Everything that takes a bare name resolves through
/// here, so a project skill shadows a global one of the same name rather than
/// colliding with it.
pub fn locate(name: &str, root: Option<&Path>) -> Result<Skill> {
    model::validname(name)?;
    if let Some(root) = root {
        let shelf = Shelf::project(root);
        if let Ok(skill) = read(&shelf, name) {
            return Ok(skill);
        }
    }
    read(&Shelf::Global, name)
}

/// Create a skill from the template, refusing to overwrite one that exists.
pub fn create(shelf: &Shelf, name: &str) -> Result<PathBuf> {
    write(shelf, name, &TEMPLATE.replace("{name}", name))
}

/// Write a whole skill that does not exist yet, validating it first so a broken
/// one never reaches the library.
pub fn write(shelf: &Shelf, name: &str, content: &str) -> Result<PathBuf> {
    let root = path(shelf, name)?;
    anyhow::ensure!(
        !root.join(ENTRY).exists(),
        "a skill named `{name}` already exists in the {} library",
        shelf.label()
    );
    model::parse(name, content).context("that skill is not valid")?;
    ensureshelf(shelf)?;
    crate::files::write(&root.join(ENTRY), content)?;
    Ok(root)
}

/// Save edited text for a skill, validating before it lands so a broken file
/// never replaces a working one.
pub fn save(shelf: &Shelf, name: &str, content: &str) -> Result<PathBuf> {
    let root = path(shelf, name)?;
    model::parse(name, content)?;
    ensureshelf(shelf)?;
    crate::files::write(&root.join(ENTRY), content)?;
    Ok(root)
}

pub fn delete(shelf: &Shelf, name: &str) -> Result<PathBuf> {
    let root = path(shelf, name)?;
    anyhow::ensure!(
        root.is_dir(),
        "no skill named `{name}` in the {} library",
        shelf.label()
    );
    std::fs::remove_dir_all(&root)
        .with_context(|| format!("could not remove {}", root.display()))?;
    Ok(root)
}

/// Copy a skill directory to `target`, replacing only files the skill owns and
/// removing anything a previous copy left behind.
pub fn copy(source: &Path, target: &Path, files: &[String]) -> Result<()> {
    for file in files {
        let from = source.join(file);
        let to = target.join(file);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        crate::files::atomiccopy(&from, &to).with_context(|| format!("could not copy {file}"))?;
    }
    // A file dropped from the skill should not survive in the installed copy.
    if let Ok(existing) = model::contents(target) {
        for file in existing {
            if !files.contains(&file) {
                let _ = std::fs::remove_file(target.join(&file));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
        let directory = tempfile::tempdir().unwrap();
        let guard = crate::files::scopeddata(directory.path());
        (directory, guard)
    }

    #[test]
    fn the_built_in_skills_are_written_once_and_then_left_alone() {
        let (_directory, _guard) = library();

        ensure().unwrap();
        let entry = path(&Shelf::Global, "synapse-mesh").unwrap().join(ENTRY);
        assert!(entry.is_file());

        std::fs::write(
            &entry,
            "---\nname: synapse-mesh\ndescription: Mine.\n---\n\nMine.\n",
        )
        .unwrap();
        ensure().unwrap();

        assert!(
            std::fs::read_to_string(&entry).unwrap().contains("Mine."),
            "an edited built-in must not be overwritten"
        );
    }

    #[test]
    fn every_built_in_skill_is_valid_against_the_standard() {
        for (name, content) in BUILTINS {
            model::parse(name, content)
                .unwrap_or_else(|error| panic!("the built-in `{name}` is invalid: {error:#}"));
        }
    }

    #[test]
    fn listing_reports_a_broken_skill_rather_than_hiding_it() {
        let (_directory, _guard) = library();
        ensure().unwrap();
        let broken = directory().unwrap().join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join(ENTRY), "no frontmatter here").unwrap();

        let (skills, problems) = all().unwrap();

        assert!(skills.iter().any(|skill| skill.name == "synapse-mesh"));
        assert!(!skills.iter().any(|skill| skill.name == "broken"));
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("broken:"), "got {:?}", problems[0]);
    }

    #[test]
    fn creating_uses_the_template_and_refuses_to_replace_an_existing_skill() {
        let (_directory, _guard) = library();

        create(&Shelf::Global, "mine").unwrap();
        let skill = read(&Shelf::Global, "mine").unwrap();
        assert_eq!(skill.name, "mine");
        assert!(!skill.description.is_empty());
        assert_eq!(skill.shelf, Shelf::Global);

        assert!(create(&Shelf::Global, "mine").is_err());
    }

    #[test]
    fn an_invalid_name_never_reaches_the_filesystem() {
        let (_directory, _guard) = library();
        assert!(create(&Shelf::Global, "Bad Name").is_err());
        assert!(!directory().unwrap().join("Bad Name").exists());
    }

    #[test]
    fn saving_a_broken_edit_leaves_the_working_skill_in_place() {
        let (_directory, _guard) = library();
        create(&Shelf::Global, "mine").unwrap();
        let before = read(&Shelf::Global, "mine").unwrap();

        assert!(
            save(
                &Shelf::Global,
                "mine",
                "---\nname: mine\n---\n\nNo description.\n"
            )
            .is_err()
        );

        assert_eq!(read(&Shelf::Global, "mine").unwrap(), before);
    }

    #[test]
    fn copying_mirrors_the_skill_and_clears_what_it_dropped() {
        let (directory, _guard) = library();
        create(&Shelf::Global, "mine").unwrap();
        let source = path(&Shelf::Global, "mine").unwrap();
        std::fs::write(source.join("extra.md"), "detail").unwrap();
        let target = directory.path().join("target").join("mine");

        let skill = read(&Shelf::Global, "mine").unwrap();
        copy(&source, &target, &skill.files).unwrap();
        assert!(target.join("extra.md").is_file());

        std::fs::remove_file(source.join("extra.md")).unwrap();
        let trimmed = read(&Shelf::Global, "mine").unwrap();
        copy(&source, &target, &trimmed.files).unwrap();

        assert!(target.join(ENTRY).is_file());
        assert!(
            !target.join("extra.md").exists(),
            "a file the skill no longer has must not survive in the copy"
        );
    }

    #[test]
    fn a_project_shelf_keeps_its_own_skill_of_the_same_name() {
        let (directory, _guard) = library();
        let project = directory.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        let shelf = Shelf::project(&project);

        create(&Shelf::Global, "release").unwrap();
        create(&shelf, "release").unwrap();
        save(
            &shelf,
            "release",
            "---\nname: release\ndescription: This repository only.\n---\n\nSteps.\n",
        )
        .unwrap();

        let global = read(&Shelf::Global, "release").unwrap();
        let scoped = read(&shelf, "release").unwrap();
        assert_ne!(global.digest, scoped.digest);
        assert_eq!(scoped.shelf, shelf);

        // A bare name resolves to the project's copy when there is one.
        assert_eq!(
            locate("release", Some(&project)).unwrap().digest,
            scoped.digest
        );
        assert_eq!(locate("release", None).unwrap().digest, global.digest);
    }

    #[test]
    fn a_shelf_is_listed_with_the_project_it_belongs_to() {
        let (directory, _guard) = library();
        let project = directory.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        let shelf = Shelf::project(&project);
        create(&shelf, "mine").unwrap();

        let found = shelves().unwrap();
        assert_eq!(found.len(), 2, "got {found:?}");
        assert_eq!(found[0], Shelf::Global);
        assert_eq!(found[1].root(), shelf.root());

        // And the shelf directory is never mistaken for a skill.
        let (skills, problems) = all().unwrap();
        assert!(problems.is_empty(), "got {problems:?}");
        assert!(skills.iter().any(|skill| skill.name == "mine"));
        assert!(skills.iter().all(|skill| !skill.name.starts_with('@')));
    }

    #[test]
    fn two_checkouts_of_the_same_name_are_two_shelves() {
        let (directory, _guard) = library();
        let first = directory.path().join("one").join("api");
        let second = directory.path().join("two").join("api");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();

        let left = Shelf::project(&first);
        let right = Shelf::project(&second);
        assert_ne!(left.key(), right.key());
        assert!(left.key().starts_with("api-"), "got {}", left.key());
    }
}
