use super::{Backend, PortableArtifact, Run, RunBundle};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fmt::Write;

const VERSION: u32 = 1;
const RECORDLIMIT: usize = 10_000;
const BUNDLELIMIT: usize = 16 * 1024 * 1024;

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
        anyhow::ensure!(
            bundle.steps.len() <= 1_000
                && bundle.changes.len() <= RECORDLIMIT
                && bundle.reviews.len() <= 1_000
                && bundle.comments.len() <= RECORDLIMIT
                && bundle.evaluations.len() <= 1_000
                && bundle.artifacts.len() <= 1_000,
            "run bundle contains too many records"
        );
        let encoded = bundle.artifacts.iter().try_fold(0usize, |total, item| {
            validatehex(&item.content)?;
            total
                .checked_add(item.content.len())
                .context("run bundle is too large")
        })?;
        anyhow::ensure!(
            encoded <= BUNDLELIMIT * 2,
            "run bundle artifacts are larger than 16 MiB"
        );
        preflight(&bundle)?;
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
            let stepid = match change.stepid {
                Some(id) => Some(
                    steps
                        .get(&id)
                        .copied()
                        .context("bundle change names a missing step")?,
                ),
                None => None,
            };
            self.recordchange(
                &run.id,
                stepid,
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
            let reviewid = match comment.reviewid {
                Some(id) => Some(
                    reviews
                        .get(&id)
                        .copied()
                        .context("bundle comment names a missing review")?,
                ),
                None => None,
            };
            let parent = match comment.parent {
                Some(id) => Some(
                    comments
                        .get(&id)
                        .copied()
                        .context("bundle comment names a missing parent")?,
                ),
                None => None,
            };
            let added = self
                .comment(
                    &run.id,
                    reviewid,
                    &comment.author,
                    &comment.targetkind,
                    &comment.target,
                    comment.line,
                    &comment.body,
                    parent,
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
    let mut encoded = String::with_capacity(content.len() * 2);
    for byte in content {
        write!(&mut encoded, "{byte:02x}").expect("writing to a string cannot fail");
    }
    encoded
}

fn decodehex(content: &str) -> Result<Vec<u8>> {
    validatehex(content)?;
    (0..content.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&content[at..at + 2], 16).context("artifact is not valid hex"))
        .collect()
}

fn validatehex(content: &str) -> Result<()> {
    anyhow::ensure!(content.len().is_multiple_of(2), "artifact is not valid hex");
    anyhow::ensure!(
        content.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "artifact is not valid hex"
    );
    Ok(())
}

fn preflight(bundle: &RunBundle) -> Result<()> {
    serde_json::from_str::<serde_json::Value>(&bundle.run.input)
        .context("bundle input is not JSON")?;
    let mut steps = std::collections::HashSet::new();
    for step in &bundle.steps {
        anyhow::ensure!(steps.insert(step.id), "bundle repeats a step id");
        serde_json::from_str::<serde_json::Value>(&step.input)
            .context("bundle step input is not JSON")?;
    }
    for change in &bundle.changes {
        anyhow::ensure!(
            change.stepid.is_none_or(|id| steps.contains(&id)),
            "bundle change names a missing step"
        );
    }
    let mut reviews = std::collections::HashSet::new();
    for review in &bundle.reviews {
        anyhow::ensure!(reviews.insert(review.id), "bundle repeats a review id");
        serde_json::from_str::<serde_json::Value>(&review.checks)
            .context("review checks are invalid")?;
        serde_json::from_str::<serde_json::Value>(&review.concerns)
            .context("review concerns are invalid")?;
    }
    let mut comments = std::collections::HashSet::new();
    for comment in &bundle.comments {
        anyhow::ensure!(
            comment.reviewid.is_none_or(|id| reviews.contains(&id)),
            "bundle comment names a missing review"
        );
        anyhow::ensure!(
            comment.parent.is_none_or(|id| comments.contains(&id)),
            "bundle comment names a missing or later parent"
        );
        anyhow::ensure!(comments.insert(comment.id), "bundle repeats a comment id");
    }
    Ok(())
}
