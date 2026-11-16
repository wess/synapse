use super::{Backend, PolicyChoice, Preview, Runner};
use anyhow::{Context, Result};

const RUNNERLIMIT: i64 = 256;
const PREVIEWLIMIT: i64 = 100;
const TEXTLIMIT: usize = 64 * 1024;
const RUNNERRETENTION: i64 = 30 * 86400;

impl Backend {
    pub async fn putrunner(
        &self,
        id: &str,
        name: &str,
        endpoint: &str,
        capabilities: serde_json::Value,
        local: bool,
    ) -> Result<Runner> {
        validateid(id, "runner id")?;
        anyhow::ensure!(name.len() <= 256, "runner name is too large");
        anyhow::ensure!(endpoint.len() <= 4_096, "runner endpoint is too large");
        anyhow::ensure!(
            redact(endpoint) == endpoint,
            "runner endpoint contains a credential"
        );
        let time = now();
        sqlx::query("DELETE FROM runner WHERE status='offline' AND updated<?")
            .bind(time - RUNNERRETENTION)
            .execute(&self.pool)
            .await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runner WHERE id=?)")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        if !exists {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runner")
                .fetch_one(&self.pool)
                .await?;
            anyhow::ensure!(count < RUNNERLIMIT, "runner limit reached");
        }
        sqlx::query(
            "INSERT INTO runner(id,name,endpoint,capabilities,status,local,lastseen,created,updated) \
             VALUES(?,?,?,?,'online',?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,\
             endpoint=excluded.endpoint,capabilities=excluded.capabilities,status='online',\
             local=excluded.local,lastseen=excluded.lastseen,updated=excluded.updated",
        )
        .bind(id)
        .bind(redact(name))
        .bind(redact(endpoint))
        .bind(cleanjson(capabilities)?)
        .bind(local)
        .bind(time)
        .bind(time)
        .bind(time)
        .execute(&self.pool)
        .await?;
        self.runner(id).await?.context("stored runner is missing")
    }

    pub async fn heartbeat(&self, id: &str, status: &str) -> Result<Runner> {
        anyhow::ensure!(
            matches!(status, "online" | "busy" | "draining" | "offline"),
            "invalid runner status"
        );
        let time = now();
        let changed = sqlx::query("UPDATE runner SET status=?,lastseen=?,updated=? WHERE id=?")
            .bind(status)
            .bind(time)
            .bind(time)
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        anyhow::ensure!(changed == 1, "runner `{id}` does not exist");
        self.runner(id).await?.context("updated runner is missing")
    }

    pub async fn runner(&self, id: &str) -> Result<Option<Runner>> {
        sqlx::query_as(
            "SELECT id,name,endpoint,capabilities,status,local,lastseen,created,updated FROM runner \
             WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn runners(&self) -> Result<Vec<Runner>> {
        sqlx::query_as(
            "SELECT id,name,endpoint,capabilities,status,local,lastseen,created,updated FROM runner \
             ORDER BY local DESC,name,id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Select an eligible runner for a run. Capabilities are a deterministic gate; the policy only
    /// ranks the rows left after it. An input shaped as `{ "capabilities": ["shell"] }` requests
    /// capabilities, while an absent list accepts every online runner.
    pub async fn selectrunner(&self, runid: &str, local: Option<bool>) -> Result<Option<Runner>> {
        let run = self
            .run(runid)
            .await?
            .with_context(|| format!("run `{runid}` does not exist"))?;
        let required = requiredcapabilities(&run.input)?;
        let mut runners = self.runners().await?;
        runners.retain(|runner| {
            runner.status == "online"
                && local.is_none_or(|local| runner.local == local)
                && hascapabilities(&runner.capabilities, &required)
        });
        runners.sort_by(|left, right| {
            right
                .local
                .cmp(&left.local)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        if runners.is_empty() {
            return Ok(None);
        }
        let choices = runners
            .iter()
            .map(|runner| runner.id.clone())
            .collect::<Vec<_>>();
        let PolicyChoice { action, .. } =
            self.choose("runner", &run.harness, &choices, runid).await?;
        Ok(runners.into_iter().find(|runner| runner.id == action))
    }

    pub async fn putpreview(
        &self,
        id: &str,
        runid: &str,
        runnerid: &str,
        url: &str,
        status: &str,
        expires: i64,
    ) -> Result<Preview> {
        validateid(id, "preview id")?;
        validpreviewstatus(status)?;
        let parsed = reqwest::Url::parse(url).context("preview URL is invalid")?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "http" | "https"),
            "preview URL must use HTTP or HTTPS"
        );
        anyhow::ensure!(url.len() <= 4_096, "preview URL is too large");
        anyhow::ensure!(
            parsed.username().is_empty() && parsed.password().is_none() && parsed.query().is_none(),
            "preview URL cannot contain credentials or a query"
        );
        anyhow::ensure!(redact(url) == url, "preview URL contains a credential");
        let time = now();
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runpreview WHERE id=?)")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        if !exists {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runpreview WHERE runid=?")
                .bind(runid)
                .fetch_one(&self.pool)
                .await?;
            anyhow::ensure!(count < PREVIEWLIMIT, "preview limit reached for this run");
        }
        sqlx::query(
            "INSERT INTO runpreview(id,runid,runnerid,url,status,expires,created,updated) \
             VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET url=excluded.url,\
             status=excluded.status,expires=excluded.expires,updated=excluded.updated",
        )
        .bind(id)
        .bind(runid)
        .bind(runnerid)
        .bind(redact(url))
        .bind(status)
        .bind(expires.max(0))
        .bind(time)
        .bind(time)
        .execute(&self.pool)
        .await?;
        self.preview(id).await?.context("stored preview is missing")
    }

    pub async fn preview(&self, id: &str) -> Result<Option<Preview>> {
        sqlx::query_as(
            "SELECT id,runid,runnerid,url,status,expires,created,updated FROM runpreview WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn previews(&self, runid: &str) -> Result<Vec<Preview>> {
        sqlx::query_as(
            "SELECT id,runid,runnerid,url,status,expires,created,updated FROM runpreview \
             WHERE runid=? ORDER BY updated DESC,id",
        )
        .bind(runid)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn recentpreviews(&self, runid: &str, limit: i64) -> Result<Vec<Preview>> {
        sqlx::query_as(
            "SELECT id,runid,runnerid,url,status,expires,created,updated FROM runpreview \
             WHERE runid=? ORDER BY updated DESC,id LIMIT ?",
        )
        .bind(runid)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }
}

fn validateid(value: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 96
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "{label} contains unsupported characters"
    );
    Ok(())
}

fn validpreviewstatus(value: &str) -> Result<()> {
    anyhow::ensure!(
        matches!(
            value,
            "starting" | "ready" | "stopped" | "failed" | "expired"
        ),
        "invalid preview status"
    );
    Ok(())
}

fn cleanjson(value: serde_json::Value) -> Result<String> {
    let text = serde_json::to_string(&value)?;
    anyhow::ensure!(text.len() <= TEXTLIMIT, "runner capabilities are too large");
    let body = redact(&text);
    serde_json::from_str::<serde_json::Value>(&body).context("redaction made invalid JSON")?;
    Ok(body)
}

fn requiredcapabilities(input: &str) -> Result<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(input).context("run input is not JSON")?;
    let Some(value) = value.get("capabilities") else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .context("run capabilities must be an array of names")?;
    anyhow::ensure!(items.len() <= 64, "run requests too many capabilities");
    items
        .iter()
        .map(|item| {
            let item = item
                .as_str()
                .context("run capabilities must be an array of names")?;
            anyhow::ensure!(
                !item.is_empty() && item.len() <= 128,
                "run capability name is invalid"
            );
            Ok(item.to_owned())
        })
        .collect()
}

fn hascapabilities(raw: &str, required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return false;
    };
    let held: std::collections::HashSet<&str> = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect();
    required.iter().all(|item| held.contains(item.as_str()))
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
