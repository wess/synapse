use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// What Synapse wrote into a tool, and what it was copied from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    /// Digest of the copy as Synapse left it. A different digest on disk means
    /// somebody has edited it since.
    pub digest: String,
    /// Digest of the library skill it came from. A different one now means the
    /// library has moved on and the copy is out of date.
    pub source: String,
    pub path: String,
}

#[derive(Clone)]
pub struct Receipts {
    pool: SqlitePool,
    _lock: Arc<File>,
}

impl Receipts {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let opened = crate::database::open(path.as_ref()).await?;
        Ok(Self {
            pool: opened.pool,
            _lock: opened.lock,
        })
    }

    pub async fn receipt(&self, skill: &str, tool: &str) -> Result<Option<Receipt>> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT digest, source, path FROM skillinstall WHERE skill = ? AND tool = ?",
        )
        .bind(skill)
        .bind(tool)
        .fetch_optional(&self.pool)
        .await
        .context("could not read the skill install record")?;
        Ok(row.map(|(digest, source, path)| Receipt {
            digest,
            source,
            path,
        }))
    }

    pub async fn record(
        &self,
        skill: &str,
        tool: &str,
        path: &Path,
        digest: &str,
        source: &str,
    ) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_secs() as i64;
        sqlx::query(
            "INSERT INTO skillinstall(skill, tool, path, digest, source, installed) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(skill, tool) DO UPDATE SET \
             path = excluded.path, digest = excluded.digest, source = excluded.source, \
             installed = excluded.installed",
        )
        .bind(skill)
        .bind(tool)
        .bind(path.display().to_string())
        .bind(digest)
        .bind(source)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("could not record the skill install")?;
        Ok(())
    }

    pub async fn forget(&self, skill: &str, tool: &str) -> Result<()> {
        sqlx::query("DELETE FROM skillinstall WHERE skill = ? AND tool = ?")
            .bind(skill)
            .bind(tool)
            .execute(&self.pool)
            .await?;
        Ok(())
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
    async fn a_receipt_round_trips_and_is_replaced_rather_than_duplicated() {
        let (receipts, _directory) = receipts().await;

        receipts
            .record(
                "mine",
                "Claude Code",
                Path::new("/tools/mine"),
                "aaa",
                "bbb",
            )
            .await
            .unwrap();
        let stored = receipts
            .receipt("mine", "Claude Code")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.digest, "aaa");
        assert_eq!(stored.source, "bbb");
        assert_eq!(stored.path, "/tools/mine");

        receipts
            .record(
                "mine",
                "Claude Code",
                Path::new("/tools/mine"),
                "ccc",
                "ddd",
            )
            .await
            .unwrap();
        assert_eq!(
            receipts
                .receipt("mine", "Claude Code")
                .await
                .unwrap()
                .unwrap()
                .digest,
            "ccc"
        );
    }

    #[tokio::test]
    async fn each_tool_keeps_its_own_record() {
        let (receipts, _directory) = receipts().await;
        receipts
            .record("mine", "Claude Code", Path::new("/a"), "aaa", "src")
            .await
            .unwrap();
        receipts
            .record("mine", "Codex", Path::new("/b"), "bbb", "src")
            .await
            .unwrap();

        assert_eq!(
            receipts
                .receipt("mine", "Codex")
                .await
                .unwrap()
                .unwrap()
                .digest,
            "bbb"
        );
        receipts.forget("mine", "Codex").await.unwrap();
        assert!(receipts.receipt("mine", "Codex").await.unwrap().is_none());
        assert!(
            receipts
                .receipt("mine", "Claude Code")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn an_unknown_skill_has_no_receipt() {
        let (receipts, _directory) = receipts().await;
        assert!(receipts.receipt("nobody", "Codex").await.unwrap().is_none());
    }
}
