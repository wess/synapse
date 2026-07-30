//! The one place skills live.
//!
//! Copying a skill into each tool by hand is how two copies drift apart. The
//! library is the source of truth; every connected tool gets a copy from it,
//! and Synapse remembers which copies are its own.

use crate::skill::model::{self, ENTRY, Skill};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Skills that ship with Synapse. They are written into the library on first
/// use, and are ordinary editable skills from then on.
const BUILTINS: &[(&str, &str)] = &[(
    "synapse-mesh",
    include_str!("../../assets/skills/synapse-mesh/SKILL.md"),
)];

pub const TEMPLATE: &str = "---\nname: {name}\ndescription: Say what this skill does and when an agent should reach for it. Lead with the case that matters most, because agents read this line to decide whether to load the rest.\n---\n\n## When to use this\n\nDescribe the situation that should trigger the skill.\n\n## Steps\n\n1. The first thing to do.\n2. The next thing.\n";

pub fn directory() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("skills"))
}

pub fn path(name: &str) -> Result<PathBuf> {
    model::validname(name)?;
    Ok(directory()?.join(name))
}

/// Write the shipped skills into the library if they are not there yet. Like
/// `SOUL.md`, they are created once and never overwritten afterwards, so an
/// edited copy stays edited.
pub fn ensure() -> Result<()> {
    for (name, content) in BUILTINS {
        let root = path(name)?;
        if root.join(ENTRY).exists() {
            continue;
        }
        crate::files::write(&root.join(ENTRY), content)
            .with_context(|| format!("could not create the built-in skill `{name}`"))?;
    }
    Ok(())
}

/// Every skill in the library, with anything unreadable reported rather than
/// silently skipped.
pub fn all() -> Result<(Vec<Skill>, Vec<String>)> {
    ensure()?;
    let root = directory()?;
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
        if name.starts_with('.') {
            continue;
        }
        match read(name) {
            Ok(skill) => skills.push(skill),
            Err(error) => problems.push(format!("{name}: {error:#}")),
        }
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((skills, problems))
}

pub fn read(name: &str) -> Result<Skill> {
    let root = path(name)?;
    anyhow::ensure!(
        root.join(ENTRY).is_file(),
        "no skill named `{name}` in the library"
    );
    model::read(&root)
}

/// Create a skill from the template, refusing to overwrite one that exists.
pub fn create(name: &str) -> Result<PathBuf> {
    let root = path(name)?;
    anyhow::ensure!(
        !root.join(ENTRY).exists(),
        "a skill named `{name}` already exists"
    );
    let content = TEMPLATE.replace("{name}", name);
    model::parse(name, &content).context("the skill template is not valid")?;
    crate::files::write(&root.join(ENTRY), &content)?;
    Ok(root)
}

/// Save edited text for a skill, validating before it lands so a broken file
/// never replaces a working one.
pub fn save(name: &str, content: &str) -> Result<PathBuf> {
    let root = path(name)?;
    model::parse(name, content)?;
    crate::files::write(&root.join(ENTRY), content)?;
    Ok(root)
}

pub fn delete(name: &str) -> Result<PathBuf> {
    let root = path(name)?;
    anyhow::ensure!(root.is_dir(), "no skill named `{name}` in the library");
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
        let entry = path("synapse-mesh").unwrap().join(ENTRY);
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

        create("mine").unwrap();
        let skill = read("mine").unwrap();
        assert_eq!(skill.name, "mine");
        assert!(!skill.description.is_empty());

        assert!(create("mine").is_err());
    }

    #[test]
    fn an_invalid_name_never_reaches_the_filesystem() {
        let (_directory, _guard) = library();
        assert!(create("Bad Name").is_err());
        assert!(!directory().unwrap().join("Bad Name").exists());
    }

    #[test]
    fn saving_a_broken_edit_leaves_the_working_skill_in_place() {
        let (_directory, _guard) = library();
        create("mine").unwrap();
        let before = read("mine").unwrap();

        assert!(save("mine", "---\nname: mine\n---\n\nNo description.\n").is_err());

        assert_eq!(read("mine").unwrap(), before);
    }

    #[test]
    fn copying_mirrors_the_skill_and_clears_what_it_dropped() {
        let (directory, _guard) = library();
        create("mine").unwrap();
        let source = path("mine").unwrap();
        std::fs::write(source.join("extra.md"), "detail").unwrap();
        let target = directory.path().join("target").join("mine");

        let skill = read("mine").unwrap();
        copy(&source, &target, &skill.files).unwrap();
        assert!(target.join("extra.md").is_file());

        std::fs::remove_file(source.join("extra.md")).unwrap();
        let trimmed = read("mine").unwrap();
        copy(&source, &target, &trimmed.files).unwrap();

        assert!(target.join(ENTRY).is_file());
        assert!(
            !target.join("extra.md").exists(),
            "a file the skill no longer has must not survive in the copy"
        );
    }
}
