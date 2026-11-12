use super::{Backend, PortableArtifact, Run, RunBundle};
use anyhow::{Context, Result};
use std::collections::HashMap;

const VERSION: u32 = 1;

impl Backend {
    pub async fn exportbundle(&self, runid: &str) -> Result<RunBundle> {
        let run = self
            .run(runid)
            .await?
            .with_context(|| format!("run `{runid}` does not exist"))?;
        let mut artifacts = Vec::new();
        for artifact in self.artifacts(runid).await? {
            let roles: Vec<String> = sqlx::query_scalar(
                "SELECT role FROM runartifact WHERE runid=? AND digest=? ORDER BY role",
            )
            .bind(runid)
            .bind(&artifact.digest)
            .fetch_all(&self.pool)
            .await?;
            artifacts.push(PortableArtifact {
                content: hex(&self.readartifact(&artifact.digest).await?),
                artifact,
                roles,
            });
        }
        Ok(RunBundle {
            version: VERSION,
            steps: self.steps(runid).await?,
            changes: self.changes(runid).await?,
            reviews: self.reviews(runid).await?,
            comments: self.comments(runid).await?,
            evaluations: self.evaluations(runid).await?,
            artifacts,
            run,
        })
    }

    pub async fn importbundle(&self, bundle: RunBundle, project: Option<&str>) -> Result<Run> {
        anyhow::ensure!(bundle.version == VERSION, "unsupported run bundle version");
        let input = serde_json::from_str(&bundle.run.input).context("bundle input is not JSON")?;
        let run = self
            .createrun(
                project.unwrap_or(&bundle.run.project),
                &bundle.run.harness,
                input,
            )
            .await?;
        sqlx::query("UPDATE run SET title=?,parent=?,replayof=? WHERE id=?")
            .bind(&bundle.run.title)
            .bind("")
            .bind(&bundle.run.id)
            .bind(&run.id)
            .execute(&self.pool)
            .await?;
        let mut steps = HashMap::new();
        for step in bundle.steps {
            let input =
                serde_json::from_str(&step.input).context("bundle step input is not JSON")?;
            let added = self.addstep(&run.id, &step.name, input).await?;
            self.updatestep(added.id, step.status, &step.output, &step.error)
                .await?;
            steps.insert(step.id, added.id);
        }
        for change in bundle.changes {
            self.recordchange(
                &run.id,
                change.stepid.and_then(|id| steps.get(&id).copied()),
                &change.path,
                &change.operation,
                &change.previouspath,
                &change.beforedigest,
                &change.afterdigest,
                &change.summary,
            )
            .await?;
        }
        let mut reviews = HashMap::new();
        for review in bundle.reviews {
            let checks =
                serde_json::from_str(&review.checks).context("review checks are invalid")?;
            let concerns =
                serde_json::from_str(&review.concerns).context("review concerns are invalid")?;
            let added = self
                .submitreview(
                    &run.id,
                    &review.reviewer,
                    &review.verdict,
                    &review.summary,
                    checks,
                    &review.reasoning,
                    concerns,
                )
                .await?;
            reviews.insert(review.id, added.id);
        }
        let mut comments = HashMap::new();
        for comment in bundle.comments {
            let added = self
                .comment(
                    &run.id,
                    comment.reviewid.and_then(|id| reviews.get(&id).copied()),
                    &comment.author,
                    &comment.targetkind,
                    &comment.target,
                    comment.line,
                    &comment.body,
                    comment.parent.and_then(|id| comments.get(&id).copied()),
                )
                .await?;
            if comment.resolved {
                self.resolvecomment(added.id, true).await?;
            }
            comments.insert(comment.id, added.id);
        }
        for item in bundle.artifacts {
            let content = decodehex(&item.content)?;
            let artifact = self
                .putartifact(
                    &item.artifact.name,
                    &item.artifact.kind,
                    &item.artifact.media,
                    &content,
                )
                .await?;
            anyhow::ensure!(
                artifact.digest == item.artifact.digest,
                "artifact digest changed"
            );
            for role in item.roles {
                self.attach(&run.id, &artifact.digest, &role).await?;
            }
        }
        for evaluation in bundle.evaluations {
            self.evaluate(
                &run.id,
                &evaluation.expected,
                &evaluation.actual,
                evaluation.score,
                &evaluation.note,
            )
            .await?;
        }
        self.updaterun(
            &run.id,
            bundle.run.status,
            &bundle.run.output,
            &bundle.run.error,
        )
        .await
    }
}

fn hex(content: &[u8]) -> String {
    content.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decodehex(content: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(content.len().is_multiple_of(2), "artifact is not valid hex");
    (0..content.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&content[at..at + 2], 16).context("artifact is not valid hex"))
        .collect()
}
