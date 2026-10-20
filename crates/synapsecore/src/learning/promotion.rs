use super::{Record, Target, now, project, retention};
use crate::brain::Brain;
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::Serialize;
use sqlx::FromRow;
use std::path::Path;

/// Independent checked or confirmed sessions needed before suggesting a procedure.
pub const SUPPORT: usize = 3;

#[derive(Clone, Debug, Serialize)]
pub struct Assessment {
    pub support: i64,
    pub blocked: i64,
}

#[derive(Clone, Debug, Serialize, JsonSchema, FromRow)]
pub struct Promotion {
    pub id: i64,
    pub project: String,
    pub lesson: String,
    pub feedback: i64,
    pub skill: String,
    pub description: String,
    pub instructions: String,
    pub origin: String,
    pub state: String,
    pub created: i64,
}

impl Brain {
    pub(crate) async fn assessevidence(&self, record: &Record) -> Result<Assessment> {
        let (support, blocked): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(DISTINCT CASE WHEN outcome = 'helpful' AND \
             verification IN ('checkpassed','confirmed') AND unstable = 0 THEN session END), \
             COALESCE(SUM(verification != 'rejected' AND (unstable = 1 OR \
             (outcome != 'helpful' AND verification != 'reported'))),0) \
             FROM learningfeedback WHERE project = ? AND target = ? AND digest = ? \
             AND lesson = ? AND updated >= ?",
        )
        .bind(&record.project)
        .bind(&record.target)
        .bind(&record.digest)
        .bind(&record.lesson)
        .bind(now()? - retention::RETENTIONDAYS * 86_400)
        .fetch_one(self.pool())
        .await?;
        Ok(Assessment { support, blocked })
    }

    pub(crate) async fn collectlearning(&self, record: &Record, folder: &Path) -> Result<()> {
        if !record.qualified() {
            return Ok(());
        }
        anyhow::ensure!(
            record.project == project(Some(folder))?,
            "feedback project changed"
        );
        let now = now()?;
        let assessment = self.assessevidence(record).await?;
        let mut transaction = self.pool().begin().await?;
        if !record.query.is_empty() {
            sqlx::query(
                "INSERT INTO learningcase(project,feedback,query,expected,created) VALUES (?,?,?,?,?) \
                 ON CONFLICT(project,query,expected) DO NOTHING",
            ).bind(&record.project).bind(record.id).bind(&record.query).bind(&record.expected)
                .bind(now).execute(&mut *transaction).await?;
        }
        if assessment.support >= SUPPORT as i64 && assessment.blocked == 0 {
            let (skill, description, instructions) = match record.target()? {
                Target::Memory { .. } => (
                    format!("learned{}", &record.lesson[..16]),
                    format!(
                        "Use when {}",
                        record.problem.chars().take(500).collect::<String>()
                    ),
                    format!(
                        "## When to use this\n\n{}\n\n## Procedure\n\n{}\n",
                        record.problem, record.correction
                    ),
                ),
                Target::Skill { name, scope } => {
                    let shelf = match scope {
                        crate::brain::MemoryScope::Global => crate::skill::Shelf::Global,
                        crate::brain::MemoryScope::Project => crate::skill::Shelf::project(folder),
                    };
                    let skill = crate::skill::library::read(&shelf, &name)?;
                    (name, skill.description, record.correction.clone())
                }
            };
            sqlx::query(
                "INSERT INTO learningpromotion(project,lesson,feedback,skill,description,instructions,created) \
                 VALUES (?,?,?,?,?,?,?) ON CONFLICT(project,lesson) DO NOTHING",
            ).bind(&record.project).bind(&record.lesson).bind(record.id).bind(skill)
                .bind(description).bind(instructions).bind(now).execute(&mut *transaction).await?;
        }
        retention::prune(&mut transaction, now).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn learningproposals(&self, folder: &Path) -> Result<Vec<Promotion>> {
        Ok(sqlx::query_as(
            "SELECT * FROM learningpromotion WHERE project = ? AND state = 'proposed' \
             ORDER BY id LIMIT 100",
        )
        .bind(project(Some(folder))?)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn learningproposal(&self, id: i64, folder: &Path) -> Result<Promotion> {
        sqlx::query_as("SELECT * FROM learningpromotion WHERE id = ? AND project = ?")
            .bind(id)
            .bind(project(Some(folder))?)
            .fetch_optional(self.pool())
            .await?
            .context("proposal does not exist in this project")
    }

    pub async fn rejectlearning(&self, id: i64, folder: &Path) -> Result<()> {
        let proposal = self.learningproposal(id, folder).await?;
        anyhow::ensure!(proposal.state == "proposed", "proposal is already settled");
        sqlx::query("UPDATE learningpromotion SET state = 'rejected' WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
