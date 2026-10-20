use super::{Assessment, Outcome, Promotion, Record, Target, Verification};
use crate::brain::{Brain, MemoryScope};
use crate::skill::{self, Shelf};
use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::FromRow;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Serialize, FromRow)]
pub struct Evidence {
    pub id: i64,
    pub session: String,
    pub verification: Verification,
    pub outcome: Outcome,
    pub judgment: String,
    pub checkstate: String,
    pub unstable: bool,
    pub evidence: String,
}

#[derive(Debug, Serialize)]
pub struct Review {
    pub proposal: Promotion,
    pub feedback: Record,
    pub current: String,
    pub currenttruncated: bool,
    pub redacted: usize,
    pub fresh: bool,
    pub evidence: Vec<Evidence>,
    pub assessment: Assessment,
}

impl Brain {
    pub async fn reviewlearning(&self, id: i64, folder: &Path) -> Result<Review> {
        let proposal = self.learningproposal(id, folder).await?;
        let feedback = self.feedbackrecord(proposal.feedback, folder).await?;
        let target = feedback.target()?;
        let fresh = target
            .fingerprint(self, folder)
            .await
            .is_ok_and(|fingerprint| fingerprint == feedback.digest);
        let (current, currenttruncated) = match target {
            Target::Memory { id } => {
                let memory = self
                    .memory(id)
                    .await?
                    .context("the supporting memory no longer exists")?;
                // The stored target may have moved to another project since the proposal.
                anyhow::ensure!(
                    memory.scope == MemoryScope::Global || memory.project == feedback.project,
                    "the supporting memory is outside this project"
                );
                (
                    memory.body.chars().take(8_000).collect(),
                    memory.body.chars().count() > 8_000,
                )
            }
            Target::Skill { name, scope } => {
                let shelf = match scope {
                    MemoryScope::Global => Shelf::Global,
                    MemoryScope::Project => Shelf::project(folder),
                };
                let path = skill::library::path(&shelf, &name)?.join(skill::ENTRY);
                let mut text = String::new();
                std::fs::File::open(path)?
                    .take(65_537)
                    .read_to_string(&mut text)?;
                anyhow::ensure!(
                    text.len() <= 65_536,
                    "skill is too large for an automatic review"
                );
                (text, false)
            }
        };
        let evidence = sqlx::query_as(
            "SELECT id,session,verification,outcome,judgment,checkstate,unstable,evidence \
             FROM learningfeedback WHERE project = ? AND target = ? AND digest = ? AND lesson = ? \
             ORDER BY id DESC LIMIT 20",
        )
        .bind(&feedback.project)
        .bind(&feedback.target)
        .bind(&feedback.digest)
        .bind(&feedback.lesson)
        .fetch_all(self.pool())
        .await?;
        let assessment = self.assessevidence(&feedback).await?;
        let current = crate::brain::redact(&current);
        Ok(Review {
            proposal,
            feedback,
            current: current.body,
            redacted: current.count,
            currenttruncated,
            fresh,
            evidence,
            assessment,
        })
    }
}
