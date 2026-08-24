//! Proposals and revisions: what an agent wrote, and what a skill used to say.
//!
//! Both hang off [`Receipts`] rather than a store of their own, because every
//! command that reads one reads the other, and `brain.db` charges an integrity
//! check per handle.
//!
//! The two tables answer the two halves of self-improvement. A proposal is a
//! skill an agent wrote that nobody has approved: it exists in the library and
//! in no tool, so writing one costs the user a row in a list rather than
//! context in every session on the machine. A revision is the text a skill held
//! before it was corrected, which is what makes a correction reversible — the
//! same bargain a superseded memory makes.

use crate::skill::Receipts;
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// How many revisions of one skill are kept. Anything unbounded that Synapse
/// writes gets a bound; the ones worth reading are the recent ones.
const KEPT: i64 = 20;

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct Proposal {
    pub skill: String,
    pub scope: String,
    /// The project it belongs to, empty for a global skill.
    pub project: String,
    /// The tool whose session wrote it.
    pub tool: String,
    /// One line from the agent saying why it is worth keeping.
    pub note: String,
    pub created: i64,
    #[serde(skip)]
    pub shelf: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct Revision {
    pub id: i64,
    pub skill: String,
    /// What the skill said before this revision replaced it.
    pub body: String,
    /// One line saying what was wrong with it.
    pub note: String,
    pub tool: String,
    pub created: i64,
}

fn now() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs() as i64)
}

impl Receipts {
    /// Record that an agent wrote this skill and that nobody has looked at it.
    pub async fn propose(
        &self,
        shelf: &str,
        skill: &str,
        project: &str,
        tool: &str,
        note: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO skillproposal(shelf, skill, project, tool, note, created) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(shelf, skill) DO UPDATE SET \
             project = excluded.project, tool = excluded.tool, note = excluded.note, \
             created = excluded.created",
        )
        .bind(shelf)
        .bind(skill)
        .bind(project)
        .bind(tool)
        .bind(note)
        .bind(now()?)
        .execute(&self.pool)
        .await
        .context("could not record the proposal")?;
        Ok(())
    }

    /// Whether this skill is still waiting for somebody to look at it.
    pub async fn proposed(&self, shelf: &str, skill: &str) -> Result<Option<Proposal>> {
        Ok(self
            .proposals()
            .await?
            .into_iter()
            .find(|item| item.shelf == shelf && item.skill == skill))
    }

    /// Everything waiting for review, oldest first — a queue reads better in
    /// the order it arrived.
    pub async fn proposals(&self) -> Result<Vec<Proposal>> {
        let rows: Vec<(String, String, String, String, String, i64)> = sqlx::query_as(
            "SELECT shelf, skill, project, tool, note, created FROM skillproposal \
             ORDER BY created, skill",
        )
        .fetch_all(&self.pool)
        .await
        .context("could not read the proposed skills")?;
        Ok(rows
            .into_iter()
            .map(|(shelf, skill, project, tool, note, created)| Proposal {
                scope: match shelf.is_empty() {
                    true => "global".to_owned(),
                    false => "project".to_owned(),
                },
                shelf,
                skill,
                project,
                tool,
                note,
                created,
            })
            .collect())
    }

    /// How many skills are waiting, without reading any of them. What the
    /// status line and the session hook want.
    pub async fn waiting(&self) -> Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM skillproposal")
            .fetch_one(&self.pool)
            .await
            .context("could not count the proposed skills")
    }

    pub async fn settle(&self, shelf: &str, skill: &str) -> Result<()> {
        sqlx::query("DELETE FROM skillproposal WHERE shelf = ? AND skill = ?")
            .bind(shelf)
            .bind(skill)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Keep what a skill said before it was replaced, and drop everything past
    /// the bound.
    pub async fn revised(
        &self,
        shelf: &str,
        skill: &str,
        body: &str,
        note: &str,
        tool: &str,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO skillrevision(shelf, skill, body, note, tool, created) \
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(shelf)
        .bind(skill)
        .bind(body)
        .bind(note)
        .bind(tool)
        .bind(now()?)
        .fetch_one(&self.pool)
        .await
        .context("could not record the revision")?;

        sqlx::query(
            "DELETE FROM skillrevision WHERE shelf = ? AND skill = ? AND id NOT IN \
             (SELECT id FROM skillrevision WHERE shelf = ? AND skill = ? ORDER BY id DESC LIMIT ?)",
        )
        .bind(shelf)
        .bind(skill)
        .bind(shelf)
        .bind(skill)
        .bind(KEPT)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// One skill's revisions, newest first.
    pub async fn revisions(&self, shelf: &str, skill: &str) -> Result<Vec<Revision>> {
        let rows: Vec<(i64, String, String, String, i64)> = sqlx::query_as(
            "SELECT id, body, note, tool, created FROM skillrevision \
             WHERE shelf = ? AND skill = ? ORDER BY id DESC",
        )
        .bind(shelf)
        .bind(skill)
        .fetch_all(&self.pool)
        .await
        .context("could not read the skill's revisions")?;
        Ok(rows
            .into_iter()
            .map(|(id, body, note, tool, created)| Revision {
                id,
                skill: skill.to_owned(),
                body,
                note,
                tool,
                created,
            })
            .collect())
    }

    /// One revision by id, so `skill revert` can name the version it means.
    pub async fn revision(&self, shelf: &str, skill: &str, id: i64) -> Result<Revision> {
        self.revisions(shelf, skill)
            .await?
            .into_iter()
            .find(|item| item.id == id)
            .with_context(|| format!("`{skill}` has no revision {id}"))
    }

    /// Forget a skill's whole history, for when the skill itself is gone.
    pub async fn forgethistory(&self, shelf: &str, skill: &str) -> Result<()> {
        sqlx::query("DELETE FROM skillrevision WHERE shelf = ? AND skill = ?")
            .bind(shelf)
            .bind(skill)
            .execute(&self.pool)
            .await?;
        self.settle(shelf, skill).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn receipts() -> (Receipts, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let receipts = Receipts::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        (receipts, directory)
    }

    #[tokio::test]
    async fn a_proposal_waits_until_it_is_settled() {
        let (receipts, _directory) = receipts().await;
        receipts
            .propose("", "release", "", "Claude Code", "worked it out twice")
            .await
            .unwrap();

        let waiting = receipts.proposals().await.unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].skill, "release");
        assert_eq!(waiting[0].scope, "global");
        assert_eq!(waiting[0].note, "worked it out twice");
        assert!(receipts.proposed("", "release").await.unwrap().is_some());

        receipts.settle("", "release").await.unwrap();
        assert!(receipts.proposals().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_same_name_on_two_shelves_is_two_proposals() {
        let (receipts, _directory) = receipts().await;
        receipts
            .propose("", "release", "", "Codex", "")
            .await
            .unwrap();
        receipts
            .propose("api-1234abcd", "release", "/repos/api", "Codex", "")
            .await
            .unwrap();

        let waiting = receipts.proposals().await.unwrap();
        assert_eq!(waiting.len(), 2);
        assert_eq!(
            waiting
                .iter()
                .find(|item| item.scope == "project")
                .unwrap()
                .project,
            "/repos/api"
        );
    }

    #[tokio::test]
    async fn revisions_are_kept_newest_first_and_bounded() {
        let (receipts, _directory) = receipts().await;
        for index in 0..KEPT + 5 {
            receipts
                .revised("", "release", &format!("body {index}"), "wrong", "Codex")
                .await
                .unwrap();
        }

        let history = receipts.revisions("", "release").await.unwrap();
        assert_eq!(history.len() as i64, KEPT);
        assert_eq!(history[0].body, format!("body {}", KEPT + 4));
        assert!(history[0].id > history[1].id);

        let named = receipts
            .revision("", "release", history[1].id)
            .await
            .unwrap();
        assert_eq!(named.body, history[1].body);
        assert!(receipts.revision("", "release", 0).await.is_err());
    }

    #[tokio::test]
    async fn deleting_a_skill_takes_its_history_with_it() {
        let (receipts, _directory) = receipts().await;
        receipts
            .propose("", "release", "", "Codex", "")
            .await
            .unwrap();
        receipts
            .revised("", "release", "old", "wrong", "Codex")
            .await
            .unwrap();

        receipts.forgethistory("", "release").await.unwrap();

        assert!(receipts.revisions("", "release").await.unwrap().is_empty());
        assert!(receipts.proposals().await.unwrap().is_empty());
    }
}
