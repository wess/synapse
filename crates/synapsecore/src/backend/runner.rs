use super::{Backend, Preview, Runner};
use anyhow::{Context, Result};

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
        let time = now();
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
        let changed = sqlx::query("UPDATE runner SET status=?,lastseen=?,updated=? WHERE id=?")
            .bind(status)
            .bind(now())
            .bind(now())
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
        let time = now();
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
    let body = redact(&text);
    serde_json::from_str::<serde_json::Value>(&body).context("redaction made invalid JSON")?;
    Ok(body)
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
