//! Putting a library skill into a tool's own skills folder, and knowing what
//! Synapse put there.
//!
//! The receipt in the database is what separates "Synapse wrote this and the
//! library has moved on" from "somebody wrote a skill of the same name by
//! hand". Without it every sync would be a guess, and the safe guess would be
//! to do nothing.

use crate::agent::Agent;
use crate::skill::model::{self, ENTRY, Shelf, Skill};
use crate::skill::{Receipts, library};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// The tool does not have this skill.
    Missing,
    /// Installed by Synapse and identical to the library.
    Installed,
    /// Installed by Synapse, but the library has changed since.
    Stale,
    /// Installed by Synapse and edited in place afterwards.
    Modified,
    /// A skill of the same name that Synapse did not write.
    Foreign,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "not installed",
            Self::Installed => "installed",
            Self::Stale => "update available",
            Self::Modified => "changed in place",
            Self::Foreign => "not ours",
        }
    }

    /// Whether writing over it would destroy something Synapse did not create.
    pub fn protected(self) -> bool {
        matches!(self, Self::Modified | Self::Foreign)
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct Status {
    pub skill: String,
    pub tool: String,
    pub state: State,
    pub path: String,
    /// `global` or `project`, so two skills of the same name on two shelves are
    /// two rows and not one contradicting itself.
    pub scope: String,
    /// The project a project-scoped skill belongs to, empty otherwise.
    pub project: String,
    /// Whether it is still waiting for a person to approve it. A proposed skill
    /// is in the library and in no tool, by design.
    pub proposed: bool,
}

impl Status {
    /// The shelf this row's skill lives on. Rebuilt from the row rather than
    /// carried on it: a dashboard hands a row back to say "approve this", and
    /// the project root is the whole of what identifies the shelf.
    pub fn shelf(&self) -> Shelf {
        match self.project.is_empty() {
            true => Shelf::Global,
            false => Shelf::project(Path::new(&self.project)),
        }
    }
}

/// Where `skill` belongs for `agent`, or `None` when the tool has nowhere to
/// put it — a project skill needs a project-local skills folder, and not every
/// tool has one. That is a fact about the tool, not an error.
pub fn target(agent: &Agent, shelf: &Shelf, skill: &str) -> Option<PathBuf> {
    match shelf {
        Shelf::Global => Some(agent.skills.join(skill)),
        Shelf::Project { root, .. } => {
            let relative = agent.projectskills.trim();
            match relative.is_empty() {
                true => None,
                false => Some(Path::new(root).join(relative).join(skill)),
            }
        }
    }
}

/// What the tool currently has for this skill.
pub async fn state(receipts: &Receipts, agent: &Agent, skill: &Skill) -> Result<State> {
    let Some(path) = target(agent, &skill.shelf, &skill.name) else {
        return Ok(State::Missing);
    };
    if !path.join(ENTRY).exists() {
        return Ok(State::Missing);
    }
    let Some(receipt) = receipts
        .receipt(skill.shelf.key(), &skill.name, &agent.name)
        .await?
    else {
        return Ok(State::Foreign);
    };
    let installed = model::contents(&path).and_then(|files| model::digest(&path, &files));
    let Ok(installed) = installed else {
        return Ok(State::Modified);
    };
    if installed != receipt.digest {
        return Ok(State::Modified);
    }
    if receipt.source != skill.digest {
        return Ok(State::Stale);
    }
    Ok(State::Installed)
}

/// Copy a skill into a tool, refusing to write over anything Synapse does not
/// own unless the caller has decided to replace it.
pub async fn install(
    receipts: &Receipts,
    agent: &Agent,
    skill: &Skill,
    replace: bool,
) -> Result<State> {
    let current = state(receipts, agent, skill).await?;
    anyhow::ensure!(
        replace || !current.protected(),
        "`{}` in {} was {} — pass the replace option to overwrite it",
        skill.name,
        agent.name,
        current.label()
    );

    let source = library::path(&skill.shelf, &skill.name)?;
    let path = target(agent, &skill.shelf, &skill.name).with_context(|| {
        format!(
            "{} has nowhere to keep a project skill; add `projectskills` to its descriptor",
            agent.name
        )
    })?;
    std::fs::create_dir_all(&path)
        .with_context(|| format!("could not create {}", path.display()))?;
    library::copy(&source, &path, &skill.files)?;

    let files = model::contents(&path)?;
    let written = model::digest(&path, &files)?;
    receipts
        .record(
            skill.shelf.key(),
            &skill.name,
            &agent.name,
            &path,
            &written,
            &skill.digest,
        )
        .await?;
    Ok(State::Installed)
}

/// Take a skill back out of a tool. Only a copy Synapse wrote and nobody has
/// touched is removed; anything else is reported so the choice stays with the
/// user.
pub async fn remove(
    receipts: &Receipts,
    agent: &Agent,
    shelf: &str,
    skill: &str,
    force: bool,
) -> Result<bool> {
    let receipt = receipts.receipt(shelf, skill, &agent.name).await?;
    // The receipt says where the copy actually went, which is the only place
    // that still resolves once a project shelf has been deleted from under it.
    let path = match &receipt {
        Some(receipt) => PathBuf::from(&receipt.path),
        None if shelf.is_empty() => agent.skills.join(skill),
        None => return Ok(false),
    };
    if !path.exists() {
        receipts.forget(shelf, skill, &agent.name).await?;
        return Ok(false);
    }
    if !force {
        let receipt = receipt
            .as_ref()
            .with_context(|| format!("`{skill}` in {} was not installed by Synapse", agent.name))?;
        let files = model::contents(&path)?;
        let installed = model::digest(&path, &files)?;
        anyhow::ensure!(
            installed == receipt.digest,
            "`{skill}` in {} has been changed since Synapse installed it",
            agent.name
        );
    }
    std::fs::remove_dir_all(&path)
        .with_context(|| format!("could not remove {}", path.display()))?;
    receipts.forget(shelf, skill, &agent.name).await?;
    Ok(true)
}

/// Whether a directory looks like a skill, used to spot skills a tool already
/// has that the library does not.
pub fn skillish(path: &Path) -> bool {
    path.join(ENTRY).is_file()
}

#[cfg(test)]
#[allow(
    clippy::await_holding_lock,
    reason = "the guard serialises tests over one process-wide SYNAPSE_DATA; \
              holding it across the await is the point"
)]
mod tests {
    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
        receipts: Receipts,
        agent: Agent,
    }

    async fn fixture() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let guard = crate::files::scopeddata(directory.path());
        let receipts = Receipts::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let mut agent = crate::agent::tool::resolve(Path::new("/users/test"), None, "claude")
            .unwrap()
            .unwrap();
        agent.instructions = PathBuf::new();
        agent.settings = PathBuf::new();
        agent.integration = PathBuf::new();
        agent.skills = directory.path().join("tool").join("skills");
        Fixture {
            _directory: directory,
            _guard: guard,
            receipts,
            agent,
        }
    }

    #[tokio::test]
    async fn a_fresh_install_reports_itself_as_current() {
        let fixture = fixture().await;
        library::create(&Shelf::Global, "mine").unwrap();
        let skill = library::read(&Shelf::Global, "mine").unwrap();

        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Missing
        );
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();

        assert!(
            target(&fixture.agent, &Shelf::Global, "mine")
                .unwrap()
                .join(ENTRY)
                .is_file()
        );
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Installed
        );
    }

    #[tokio::test]
    async fn editing_the_library_makes_the_copy_stale_and_syncing_clears_it() {
        let fixture = fixture().await;
        library::create(&Shelf::Global, "mine").unwrap();
        let skill = library::read(&Shelf::Global, "mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();

        library::save(
            &Shelf::Global,
            "mine",
            "---\nname: mine\ndescription: Now it says something else entirely.\n---\n\nNew body.\n",
        )
        .unwrap();
        let updated = library::read(&Shelf::Global, "mine").unwrap();
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &updated)
                .await
                .unwrap(),
            State::Stale
        );

        install(&fixture.receipts, &fixture.agent, &updated, false)
            .await
            .unwrap();
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &updated)
                .await
                .unwrap(),
            State::Installed
        );
        assert!(
            std::fs::read_to_string(
                target(&fixture.agent, &Shelf::Global, "mine")
                    .unwrap()
                    .join(ENTRY)
            )
            .unwrap()
            .contains("New body.")
        );
    }

    #[tokio::test]
    async fn a_copy_edited_in_the_tool_is_never_overwritten_by_accident() {
        let fixture = fixture().await;
        library::create(&Shelf::Global, "mine").unwrap();
        let skill = library::read(&Shelf::Global, "mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();
        let entry = target(&fixture.agent, &Shelf::Global, "mine")
            .unwrap()
            .join(ENTRY);
        std::fs::write(
            &entry,
            "---\nname: mine\ndescription: Edited right here in the tool.\n---\n\nTheirs.\n",
        )
        .unwrap();

        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Modified
        );
        let error = install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("changed in place"), "got {error}");
        assert!(std::fs::read_to_string(&entry).unwrap().contains("Theirs."));

        // Asking explicitly still works.
        install(&fixture.receipts, &fixture.agent, &skill, true)
            .await
            .unwrap();
        assert!(!std::fs::read_to_string(&entry).unwrap().contains("Theirs."));
    }

    #[tokio::test]
    async fn a_hand_written_skill_of_the_same_name_is_left_alone() {
        let fixture = fixture().await;
        library::create(&Shelf::Global, "mine").unwrap();
        let skill = library::read(&Shelf::Global, "mine").unwrap();
        let entry = target(&fixture.agent, &Shelf::Global, "mine")
            .unwrap()
            .join(ENTRY);
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(
            &entry,
            "---\nname: mine\ndescription: I wrote this myself.\n---\n\nMine alone.\n",
        )
        .unwrap();

        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Foreign
        );
        assert!(
            install(&fixture.receipts, &fixture.agent, &skill, false)
                .await
                .is_err()
        );
        assert!(
            remove(&fixture.receipts, &fixture.agent, "", "mine", false)
                .await
                .is_err(),
            "removal must not delete a skill Synapse never wrote"
        );
        assert!(
            std::fs::read_to_string(&entry)
                .unwrap()
                .contains("Mine alone.")
        );
    }

    #[tokio::test]
    async fn removing_takes_out_only_what_synapse_installed() {
        let fixture = fixture().await;
        library::create(&Shelf::Global, "mine").unwrap();
        let skill = library::read(&Shelf::Global, "mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();

        assert!(
            remove(&fixture.receipts, &fixture.agent, "", "mine", false)
                .await
                .unwrap()
        );
        assert!(
            !target(&fixture.agent, &Shelf::Global, "mine")
                .unwrap()
                .exists()
        );
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Missing
        );
        // Removing again is not an error; there is simply nothing there.
        assert!(
            !remove(&fixture.receipts, &fixture.agent, "", "mine", false)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_file_dropped_from_the_library_leaves_the_installed_copy_too() {
        let fixture = fixture().await;
        library::create(&Shelf::Global, "mine").unwrap();
        std::fs::write(
            library::path(&Shelf::Global, "mine")
                .unwrap()
                .join("extra.md"),
            "detail",
        )
        .unwrap();
        let skill = library::read(&Shelf::Global, "mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();
        assert!(
            target(&fixture.agent, &Shelf::Global, "mine")
                .unwrap()
                .join("extra.md")
                .is_file()
        );

        std::fs::remove_file(
            library::path(&Shelf::Global, "mine")
                .unwrap()
                .join("extra.md"),
        )
        .unwrap();
        let trimmed = library::read(&Shelf::Global, "mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &trimmed, false)
            .await
            .unwrap();

        assert!(
            !target(&fixture.agent, &Shelf::Global, "mine")
                .unwrap()
                .join("extra.md")
                .exists()
        );
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &trimmed)
                .await
                .unwrap(),
            State::Installed
        );
    }

    #[tokio::test]
    async fn a_project_skill_installs_beside_the_project_and_not_in_the_home() {
        let fixture = fixture().await;
        let project = fixture._directory.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        let shelf = Shelf::project(&project);
        library::create(&shelf, "release").unwrap();
        let skill = library::read(&shelf, "release").unwrap();

        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();

        let landed = target(&fixture.agent, &shelf, "release").unwrap();
        assert!(landed.join(ENTRY).is_file());
        assert!(landed.starts_with(std::fs::canonicalize(&project).unwrap()));
        assert!(
            !fixture.agent.skills.join("release").exists(),
            "a project skill must not reach the personal skills folder"
        );
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Installed
        );
    }

    #[tokio::test]
    async fn a_tool_with_nowhere_to_put_a_project_skill_says_so_rather_than_guessing() {
        let mut fixture = fixture().await;
        fixture.agent.projectskills = String::new();
        let project = fixture._directory.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        let shelf = Shelf::project(&project);
        library::create(&shelf, "release").unwrap();
        let skill = library::read(&shelf, "release").unwrap();

        assert!(target(&fixture.agent, &shelf, "release").is_none());
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Missing
        );
        let error = install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("nowhere to keep"), "got {error}");
    }
}
