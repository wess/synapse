//! Putting a library skill into a tool's own skills folder, and knowing what
//! Synapse put there.
//!
//! The receipt in the database is what separates "Synapse wrote this and the
//! library has moved on" from "somebody wrote a skill of the same name by
//! hand". Without it every sync would be a guess, and the safe guess would be
//! to do nothing.

use crate::agent::Agent;
use crate::skill::model::{self, ENTRY, Skill};
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
}

/// Where `skill` belongs for `agent`.
pub fn target(agent: &Agent, skill: &str) -> PathBuf {
    agent.skills.join(skill)
}

/// What the tool currently has for this skill.
pub async fn state(receipts: &Receipts, agent: &Agent, skill: &Skill) -> Result<State> {
    let path = target(agent, &skill.name);
    if !path.join(ENTRY).exists() {
        return Ok(State::Missing);
    }
    let Some(receipt) = receipts.receipt(&skill.name, agent.name).await? else {
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

    let source = library::path(&skill.name)?;
    let path = target(agent, &skill.name);
    std::fs::create_dir_all(&path)
        .with_context(|| format!("could not create {}", path.display()))?;
    library::copy(&source, &path, &skill.files)?;

    let files = model::contents(&path)?;
    let written = model::digest(&path, &files)?;
    receipts
        .record(&skill.name, agent.name, &path, &written, &skill.digest)
        .await?;
    Ok(State::Installed)
}

/// Take a skill back out of a tool. Only a copy Synapse wrote and nobody has
/// touched is removed; anything else is reported so the choice stays with the
/// user.
pub async fn remove(receipts: &Receipts, agent: &Agent, skill: &str, force: bool) -> Result<bool> {
    let path = target(agent, skill);
    if !path.exists() {
        receipts.forget(skill, agent.name).await?;
        return Ok(false);
    }
    let receipt = receipts.receipt(skill, agent.name).await?;
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
    receipts.forget(skill, agent.name).await?;
    Ok(true)
}

/// Whether a directory looks like a skill, used to spot skills a tool already
/// has that the library does not.
pub fn skillish(path: &Path) -> bool {
    path.join(ENTRY).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Kind;

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
        let agent = Agent {
            kind: Kind::Claude,
            name: "Claude Code",
            command: "claude",
            instructions: PathBuf::new(),
            settings: PathBuf::new(),
            integration: PathBuf::new(),
            skills: directory.path().join("tool").join("skills"),
        };
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
        library::create("mine").unwrap();
        let skill = library::read("mine").unwrap();

        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Missing
        );
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();

        assert!(target(&fixture.agent, "mine").join(ENTRY).is_file());
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
        library::create("mine").unwrap();
        let skill = library::read("mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();

        library::save(
            "mine",
            "---\nname: mine\ndescription: Now it says something else entirely.\n---\n\nNew body.\n",
        )
        .unwrap();
        let updated = library::read("mine").unwrap();
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
            std::fs::read_to_string(target(&fixture.agent, "mine").join(ENTRY))
                .unwrap()
                .contains("New body.")
        );
    }

    #[tokio::test]
    async fn a_copy_edited_in_the_tool_is_never_overwritten_by_accident() {
        let fixture = fixture().await;
        library::create("mine").unwrap();
        let skill = library::read("mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();
        let entry = target(&fixture.agent, "mine").join(ENTRY);
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
        library::create("mine").unwrap();
        let skill = library::read("mine").unwrap();
        let entry = target(&fixture.agent, "mine").join(ENTRY);
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
            remove(&fixture.receipts, &fixture.agent, "mine", false)
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
        library::create("mine").unwrap();
        let skill = library::read("mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();

        assert!(
            remove(&fixture.receipts, &fixture.agent, "mine", false)
                .await
                .unwrap()
        );
        assert!(!target(&fixture.agent, "mine").exists());
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &skill)
                .await
                .unwrap(),
            State::Missing
        );
        // Removing again is not an error; there is simply nothing there.
        assert!(
            !remove(&fixture.receipts, &fixture.agent, "mine", false)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_file_dropped_from_the_library_leaves_the_installed_copy_too() {
        let fixture = fixture().await;
        library::create("mine").unwrap();
        std::fs::write(library::path("mine").unwrap().join("extra.md"), "detail").unwrap();
        let skill = library::read("mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &skill, false)
            .await
            .unwrap();
        assert!(target(&fixture.agent, "mine").join("extra.md").is_file());

        std::fs::remove_file(library::path("mine").unwrap().join("extra.md")).unwrap();
        let trimmed = library::read("mine").unwrap();
        install(&fixture.receipts, &fixture.agent, &trimmed, false)
            .await
            .unwrap();

        assert!(!target(&fixture.agent, "mine").join("extra.md").exists());
        assert_eq!(
            state(&fixture.receipts, &fixture.agent, &trimmed)
                .await
                .unwrap(),
            State::Installed
        );
    }
}
