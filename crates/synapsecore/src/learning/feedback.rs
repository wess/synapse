use super::{Feedback, Record, Verification, clean, digest, now, project, retention};
use crate::brain::Brain;
use anyhow::{Context, Result};
use std::path::Path;

impl Brain {
    /// Record a claim. Verification is supplied separately by a check or a user.
    pub async fn feedback(&self, input: Feedback, folder: &Path, session: &str) -> Result<Record> {
        anyhow::ensure!(self.learn().await?, "self-improvement is off");
        anyhow::ensure!(
            !session.is_empty() && session.len() <= 256,
            "invalid session id"
        );
        let project = project(Some(folder))?;
        let target = serde_json::to_string(&input.target)?;
        let fingerprint = input.target.fingerprint(self, folder).await?;
        let problem = clean(&input.problem, 1_000, "problem")?;
        let correction = clean(&input.correction, 4_000, "correction")?;
        let evidence = clean(&input.evidence, 1_000, "evidence")?;
        let query = clean(&input.query, 1_000, "query")?;
        let expected = clean(&input.expected, 1_000, "expected")?;
        anyhow::ensure!(!problem.is_empty(), "feedback needs a problem or situation");
        anyhow::ensure!(
            query.is_empty() == expected.is_empty(),
            "query and expected belong together"
        );
        let lesson = digest(&(&target, &problem, &correction, &query, &expected))?;
        let session = digest(&session)?;
        let now = now()?;
        let _writer = self.writing().await;
        let mut transaction = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO learningfeedback(project, target, digest, lesson, session, outcome, \
             problem, correction, evidence, query, expected, created, updated) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(project, target, digest, lesson, session) DO UPDATE SET \
             verification = CASE WHEN learningfeedback.outcome = excluded.outcome \
             AND learningfeedback.evidence = excluded.evidence THEN learningfeedback.verification \
             ELSE 'reported' END, \
             judgment = CASE WHEN learningfeedback.outcome = excluded.outcome \
             AND learningfeedback.evidence = excluded.evidence THEN learningfeedback.judgment ELSE 'unreviewed' END, \
             checkstate = CASE WHEN learningfeedback.outcome = excluded.outcome \
             AND learningfeedback.evidence = excluded.evidence THEN learningfeedback.checkstate ELSE 'none' END, \
             checked = CASE WHEN learningfeedback.outcome = excluded.outcome \
             AND learningfeedback.evidence = excluded.evidence THEN learningfeedback.checked ELSE 0 END, \
             reviewed = CASE WHEN learningfeedback.outcome = excluded.outcome \
             AND learningfeedback.evidence = excluded.evidence THEN learningfeedback.reviewed ELSE 0 END, \
             outcome = excluded.outcome, evidence = excluded.evidence, \
             updated = excluded.updated RETURNING id",
        )
        .bind(&project).bind(&target).bind(&fingerprint).bind(&lesson).bind(&session)
        .bind(input.outcome.value()).bind(problem).bind(correction).bind(evidence)
        .bind(query).bind(expected).bind(now).bind(now)
        .fetch_one(&mut *transaction).await?;
        retention::prune(&mut transaction, now).await?;
        transaction.commit().await?;
        self.feedbackrecord(id, folder).await
    }

    pub async fn feedbackrecord(&self, id: i64, folder: &Path) -> Result<Record> {
        sqlx::query_as("SELECT * FROM learningfeedback WHERE id = ? AND project = ?")
            .bind(id)
            .bind(project(Some(folder))?)
            .fetch_optional(self.pool())
            .await?
            .context("feedback does not exist in this project")
    }

    pub async fn feedbackrecords(&self, folder: &Path, limit: u32) -> Result<Vec<Record>> {
        Ok(sqlx::query_as(
            "SELECT * FROM learningfeedback WHERE project = ? ORDER BY id DESC LIMIT ?",
        )
        .bind(project(Some(folder))?)
        .bind(limit.clamp(1, 100))
        .fetch_all(self.pool())
        .await?)
    }

    /// A CLI confirmation is explicit. Protocol callers can only report claims.
    pub async fn confirmfeedback(&self, id: i64, folder: &Path, accepted: bool) -> Result<Record> {
        self.verifyfeedback(
            id,
            folder,
            if accepted {
                Verification::Confirmed
            } else {
                Verification::Rejected
            },
            None,
        )
        .await
    }

    pub(crate) async fn verifyfeedback(
        &self,
        id: i64,
        folder: &Path,
        verification: Verification,
        expected: Option<&Record>,
    ) -> Result<Record> {
        anyhow::ensure!(self.learn().await?, "self-improvement is off");
        let record = self.feedbackrecord(id, folder).await?;
        if let Some(expected) = expected {
            anyhow::ensure!(
                record.digest == expected.digest
                    && record.outcome == expected.outcome
                    && record.evidence == expected.evidence
                    && record.lesson == expected.lesson,
                "feedback changed while the check ran; collect fresh evidence"
            );
        }
        anyhow::ensure!(
            verification == Verification::Rejected
                || record.target()?.fingerprint(self, folder).await? == record.digest,
            "the target changed; collect fresh evidence"
        );
        let now = now()?;
        let _writer = self.writing().await;
        let mut transaction = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut current: Record =
            sqlx::query_as("SELECT * FROM learningfeedback WHERE id = ? AND project = ?")
                .bind(id)
                .bind(&record.project)
                .fetch_one(&mut *transaction)
                .await?;
        anyhow::ensure!(
            current.digest == record.digest
                && current.outcome == record.outcome
                && current.evidence == record.evidence,
            "feedback changed; collect fresh evidence"
        );
        match verification {
            Verification::Confirmed | Verification::Rejected => {
                current.judgment = if verification == Verification::Confirmed {
                    "accepted"
                } else {
                    "rejected"
                }
                .into();
                current.reviewed = now;
                if verification == Verification::Confirmed
                    && matches!(current.checkstate.as_str(), "none" | "passed")
                {
                    current.unstable = false;
                }
            }
            Verification::Checkpassed | Verification::Checkfailed | Verification::Checktimedout => {
                current.checkstate = match verification {
                    Verification::Checkpassed => "passed",
                    Verification::Checktimedout => "timedout",
                    _ => "failed",
                }
                .into();
                current.checked = now;
                if verification != Verification::Checkpassed {
                    current.unstable = true;
                }
            }
            Verification::Reported => {}
        }
        current.verification = if current.judgment == "rejected" {
            Verification::Rejected
        } else if current.checkstate == "failed" {
            Verification::Checkfailed
        } else if current.checkstate == "timedout" {
            Verification::Checktimedout
        } else if current.judgment == "accepted" {
            Verification::Confirmed
        } else if current.checkstate == "passed" {
            Verification::Checkpassed
        } else {
            Verification::Reported
        };
        sqlx::query("UPDATE learningfeedback SET verification = ?, judgment = ?, checkstate = ?, checked = ?, reviewed = ?, unstable = ?, updated = ? WHERE id = ?")
            .bind(current.verification.value()).bind(&current.judgment).bind(&current.checkstate)
            .bind(current.checked).bind(current.reviewed).bind(current.unstable).bind(now).bind(id)
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        drop(_writer);
        let record = self.feedbackrecord(id, folder).await?;
        self.collectlearning(&record, folder).await?;
        Ok(record)
    }
}
