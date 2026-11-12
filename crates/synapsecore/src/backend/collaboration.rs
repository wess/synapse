use super::{Backend, Change, Comment, Review, Run, RunStats};
use anyhow::{Context, Result};

impl Backend {
    pub async fn runstats(&self, limit: i64) -> Result<Vec<RunStats>> {
        sqlx::query_as(
            "SELECT r.id AS runid, \
             (SELECT COUNT(*) FROM runchange c WHERE c.runid=r.id) AS changes, \
             (SELECT COUNT(*) FROM runreview v WHERE v.runid=r.id) AS reviews, \
             (SELECT COUNT(*) FROM runcomment m WHERE m.runid=r.id AND m.resolved=0) AS opencomments, \
             (SELECT COUNT(*) FROM runcheckpoint p WHERE p.runid=r.id) AS checkpoints, \
             (SELECT COUNT(*) FROM runpreview p WHERE p.runid=r.id AND p.status='ready') AS previews, \
             COALESCE((SELECT status FROM runworkspace w WHERE w.runid=r.id),'') AS workspace \
             FROM run r ORDER BY r.updated DESC,r.id DESC LIMIT ?",
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn recordchange(
        &self,
        runid: &str,
        stepid: Option<i64>,
        path: &str,
        operation: &str,
        previouspath: &str,
        beforedigest: &str,
        afterdigest: &str,
        summary: &str,
    ) -> Result<Change> {
        anyhow::ensure!(
            matches!(operation, "create" | "modify" | "delete" | "rename"),
            "invalid change operation"
        );
        anyhow::ensure!(!path.trim().is_empty(), "a change needs a path");
        if let Some(stepid) = stepid {
            let belongs: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runstep WHERE id=? AND runid=?)")
                    .bind(stepid)
                    .bind(runid)
                    .fetch_one(&self.pool)
                    .await?;
            anyhow::ensure!(belongs, "step `{stepid}` does not belong to run `{runid}`");
        }
        let id = sqlx::query(
            "INSERT INTO runchange(runid,stepid,path,operation,previouspath,beforedigest,\
             afterdigest,summary,created) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(runid)
        .bind(stepid)
        .bind(redact(path))
        .bind(operation)
        .bind(redact(previouspath))
        .bind(beforedigest)
        .bind(afterdigest)
        .bind(redact(summary))
        .bind(now())
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        self.change(id).await?.context("created change is missing")
    }

    pub async fn changes(&self, runid: &str) -> Result<Vec<Change>> {
        sqlx::query_as(
            "SELECT id,runid,stepid,path,operation,previouspath,beforedigest,afterdigest,summary,\
             created FROM runchange WHERE runid=? ORDER BY id",
        )
        .bind(runid)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn change(&self, id: i64) -> Result<Option<Change>> {
        sqlx::query_as(
            "SELECT id,runid,stepid,path,operation,previouspath,beforedigest,afterdigest,summary,\
             created FROM runchange WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn forkrun(&self, parent: &str, title: &str) -> Result<Run> {
        let source = self
            .run(parent)
            .await?
            .with_context(|| format!("run `{parent}` does not exist"))?;
        let input = serde_json::from_str(&source.input).context("parent input is not JSON")?;
        let run = self
            .createrun(&source.project, &source.harness, input)
            .await?;
        sqlx::query("UPDATE run SET parent=?,title=?,updated=? WHERE id=?")
            .bind(parent)
            .bind(redact(title))
            .bind(now())
            .bind(&run.id)
            .execute(&self.pool)
            .await?;
        self.run(&run.id).await?.context("forked run is missing")
    }

    pub async fn submitreview(
        &self,
        runid: &str,
        reviewer: &str,
        verdict: &str,
        summary: &str,
        checks: serde_json::Value,
        reasoning: &str,
        concerns: serde_json::Value,
    ) -> Result<Review> {
        anyhow::ensure!(
            matches!(verdict, "pending" | "approved" | "changes"),
            "invalid review verdict"
        );
        let time = now();
        let id = sqlx::query(
            "INSERT INTO runreview(runid,reviewer,verdict,summary,checks,reasoning,concerns,\
             created,updated) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(runid)
        .bind(redact(reviewer))
        .bind(verdict)
        .bind(redact(summary))
        .bind(cleanjson(checks)?)
        .bind(redact(reasoning))
        .bind(cleanjson(concerns)?)
        .bind(time)
        .bind(time)
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        self.review(id).await?.context("created review is missing")
    }

    pub async fn reviews(&self, runid: &str) -> Result<Vec<Review>> {
        sqlx::query_as(
            "SELECT id,runid,reviewer,verdict,summary,checks,reasoning,concerns,created,updated \
             FROM runreview WHERE runid=? ORDER BY id",
        )
        .bind(runid)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn review(&self, id: i64) -> Result<Option<Review>> {
        sqlx::query_as(
            "SELECT id,runid,reviewer,verdict,summary,checks,reasoning,concerns,created,updated \
             FROM runreview WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn comment(
        &self,
        runid: &str,
        reviewid: Option<i64>,
        author: &str,
        targetkind: &str,
        target: &str,
        line: i64,
        body: &str,
        parent: Option<i64>,
    ) -> Result<Comment> {
        anyhow::ensure!(
            matches!(
                targetkind,
                "run" | "event" | "step" | "artifact" | "change" | "file"
            ),
            "invalid comment target"
        );
        anyhow::ensure!(!body.trim().is_empty(), "a comment cannot be empty");
        let time = now();
        let id = sqlx::query(
            "INSERT INTO runcomment(runid,reviewid,author,targetkind,target,line,body,parent,\
             created,updated) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(runid)
        .bind(reviewid)
        .bind(redact(author))
        .bind(targetkind)
        .bind(redact(target))
        .bind(line.max(0))
        .bind(redact(body))
        .bind(parent)
        .bind(time)
        .bind(time)
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        self.onecomment(id)
            .await?
            .context("created comment is missing")
    }

    pub async fn comments(&self, runid: &str) -> Result<Vec<Comment>> {
        sqlx::query_as(
            "SELECT id,runid,reviewid,author,targetkind,target,line,body,parent,resolved,created,\
             updated FROM runcomment WHERE runid=? ORDER BY id",
        )
        .bind(runid)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn resolvecomment(&self, id: i64, resolved: bool) -> Result<Comment> {
        let changed = sqlx::query("UPDATE runcomment SET resolved=?,updated=? WHERE id=?")
            .bind(resolved)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        anyhow::ensure!(changed == 1, "comment `{id}` does not exist");
        self.onecomment(id)
            .await?
            .context("updated comment is missing")
    }

    async fn onecomment(&self, id: i64) -> Result<Option<Comment>> {
        sqlx::query_as(
            "SELECT id,runid,reviewid,author,targetkind,target,line,body,parent,resolved,created,\
             updated FROM runcomment WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }
}

fn cleanjson(value: serde_json::Value) -> Result<String> {
    let text = serde_json::to_string(&value)?;
    let redacted = redact(&text);
    serde_json::from_str::<serde_json::Value>(&redacted).context("redaction made invalid JSON")?;
    Ok(redacted)
}

fn redact(value: &str) -> String {
    crate::brain::redact(value).body
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
