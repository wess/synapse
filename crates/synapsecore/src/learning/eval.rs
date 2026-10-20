use super::project;
use crate::brain::{Brain, Case, Evaluation, Optimization};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::Serialize;
use sqlx::FromRow;
use std::path::Path;

#[derive(Clone, Debug, Serialize, JsonSchema, FromRow)]
pub struct Regression {
    pub id: i64,
    pub project: String,
    pub feedback: Option<i64>,
    pub query: String,
    pub expected: String,
    pub reviewed: bool,
    pub dismissed: bool,
    pub created: i64,
}

impl Brain {
    pub async fn learningcases(&self, folder: &Path) -> Result<Vec<Regression>> {
        Ok(sqlx::query_as(
            "SELECT * FROM learningcase WHERE project = ? AND dismissed = 0 ORDER BY id LIMIT 500",
        )
        .bind(project(Some(folder))?)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn reviewlearningcase(&self, id: i64, folder: &Path, accepted: bool) -> Result<()> {
        let case: Regression =
            sqlx::query_as("SELECT * FROM learningcase WHERE id = ? AND project = ?")
                .bind(id)
                .bind(project(Some(folder))?)
                .fetch_optional(self.pool())
                .await?
                .context("case does not exist in this project")?;
        if !accepted {
            sqlx::query("UPDATE learningcase SET dismissed = 1, reviewed = 0 WHERE id = ?")
                .bind(id)
                .execute(self.pool())
                .await?;
            return Ok(());
        }
        if case.reviewed {
            return Ok(());
        }
        let record = self
            .feedbackrecord(
                case.feedback.context("supporting evidence expired")?,
                folder,
            )
            .await?;
        anyhow::ensure!(
            record.qualified(),
            "the supporting correction is not verified"
        );
        anyhow::ensure!(
            record.target()?.fingerprint(self, folder).await? == record.digest,
            "the target changed; collect fresh evidence"
        );
        sqlx::query("UPDATE learningcase SET reviewed = 1, dismissed = 0 WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn evaluatelearning(
        &self,
        folder: &Path,
        limit: u32,
        budget: Option<Optimization>,
    ) -> Result<Evaluation> {
        let reviewed: Vec<Regression> = sqlx::query_as(
            "SELECT * FROM learningcase WHERE project = ? AND reviewed = 1 AND dismissed = 0 \
             ORDER BY id LIMIT 500",
        )
        .bind(project(Some(folder))?)
        .fetch_all(self.pool())
        .await?;
        anyhow::ensure!(
            !reviewed.is_empty(),
            "no reviewed learning cases in this project"
        );
        let cases = reviewed
            .into_iter()
            .map(|case| Case {
                query: case.query,
                project: Some(folder.to_path_buf()),
                expect: vec![case.expected],
            })
            .collect::<Vec<_>>();
        self.evaluate(&cases, limit, budget).await
    }
}
