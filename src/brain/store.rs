use crate::brain::{Memory, Optimization, Settings, Stats};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Brain {
    pool: SqlitePool,
    path: PathBuf,
    _lock: Arc<File>,
}

impl Brain {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let opened = crate::database::open(&path).await?;
        Ok(Self {
            pool: opened.pool,
            path,
            _lock: opened.lock,
        })
    }

    pub async fn remember(&self, body: &str, source: Option<&str>) -> Result<i64> {
        let body = body.trim();
        anyhow::ensure!(!body.is_empty(), "memory content cannot be empty");

        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_secs() as i64;
        let result = sqlx::query("INSERT INTO memory(body, source, created) VALUES (?, ?, ?)")
            .bind(body)
            .bind(source.unwrap_or(""))
            .bind(created)
            .execute(&self.pool)
            .await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn recall(&self, query: &str, limit: u32) -> Result<Vec<Memory>> {
        let settings = self.settings().await?;
        let memories = self
            .search(query, limit.clamp(1, settings.resultlimit))
            .await?;
        Ok(crate::brain::optimize::recall(memories, settings))
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<Memory>> {
        let limit = limit.clamp(1, 200) as i64;
        let query = query.trim();
        if query.is_empty() {
            sqlx::query_as::<_, Memory>(
                "SELECT rowid AS id, body, source, CAST(created AS INTEGER) AS created \
                 FROM memory ORDER BY created DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not read recent memories")
        } else {
            let expression = search_expression(query);
            sqlx::query_as::<_, Memory>(
                "SELECT rowid AS id, body, source, CAST(created AS INTEGER) AS created \
                 FROM memory WHERE memory MATCH ? ORDER BY rank LIMIT ?",
            )
            .bind(expression)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not search memories")
        }
    }

    pub async fn memory(&self, id: i64) -> Result<Option<Memory>> {
        sqlx::query_as(
            "SELECT rowid AS id, body, source, CAST(created AS INTEGER) AS created \
             FROM memory WHERE rowid = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("could not read memory")
    }

    pub async fn updatememory(
        &self,
        id: i64,
        body: &str,
        source: Option<&str>,
    ) -> Result<Option<Memory>> {
        let body = body.trim();
        anyhow::ensure!(!body.is_empty(), "memory content cannot be empty");
        let result =
            sqlx::query("UPDATE memory SET body = ?, source = COALESCE(?, source) WHERE rowid = ?")
                .bind(body)
                .bind(source.map(str::trim))
                .bind(id)
                .execute(&self.pool)
                .await
                .context("could not update memory")?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.memory(id).await
    }

    pub async fn deletememory(&self, id: i64) -> Result<Option<Memory>> {
        let memory = self.memory(id).await?;
        if memory.is_some() {
            sqlx::query("DELETE FROM memory WHERE rowid = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .context("could not delete memory")?;
        }
        Ok(memory)
    }

    pub async fn wipememories(&self) -> Result<u64> {
        sqlx::query("DELETE FROM memory")
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected())
            .context("could not wipe memories")
    }

    pub async fn settings(&self) -> Result<Settings> {
        crate::brain::settings::read(&self.pool).await
    }

    pub async fn setoptimization(&self, optimization: Optimization) -> Result<()> {
        crate::brain::settings::write(&self.pool, optimization).await
    }

    pub async fn preference(&self, key: &str) -> Result<Option<String>> {
        crate::brain::settings::value(&self.pool, key).await
    }

    pub async fn setpreference(&self, key: &str, value: &str) -> Result<()> {
        crate::brain::settings::writevalue(&self.pool, key, value).await
    }

    pub async fn stats(&self) -> Result<Stats> {
        let entries = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory")
            .fetch_one(&self.pool)
            .await?;
        let bytes = std::fs::metadata(&self.path)
            .map(|item| item.len())
            .unwrap_or(0);
        Ok(Stats { entries, bytes })
    }
}

fn search_expression(query: &str) -> String {
    query
        .split_whitespace()
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_and_finds_memory() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .remember("Prefer small focused Rust modules", Some("preferences"))
            .await
            .unwrap();

        let matches = brain.recall("focused modules", 8).await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].source, "preferences");
        assert_eq!(brain.stats().await.unwrap().entries, 1);
    }

    #[tokio::test]
    async fn rejects_empty_memory() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        assert!(brain.remember("  ", None).await.is_err());
    }

    #[tokio::test]
    async fn optimization_changes_recall_not_stored_memory() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .remember("One   durable fact.", Some("test"))
            .await
            .unwrap();
        brain.setoptimization(Optimization::Lean).await.unwrap();
        assert_eq!(
            brain.recall("durable", 8).await.unwrap()[0].body,
            "One durable fact."
        );

        brain.setoptimization(Optimization::Full).await.unwrap();
        assert_eq!(
            brain.recall("durable", 8).await.unwrap()[0].body,
            "One   durable fact."
        );
        brain.setpreference("appearance", "dark").await.unwrap();
        assert_eq!(
            brain.preference("appearance").await.unwrap().as_deref(),
            Some("dark")
        );
    }

    #[tokio::test]
    async fn updates_deletes_and_wipes_memories() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let first = brain.remember("first", Some("old")).await.unwrap();
        brain.remember("second", Some("test")).await.unwrap();

        let updated = brain
            .updatememory(first, "updated", Some("new"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.body, "updated");
        assert_eq!(updated.source, "new");
        assert_eq!(brain.search("updated", 100).await.unwrap(), vec![updated]);
        assert!(brain.deletememory(first).await.unwrap().is_some());
        assert_eq!(brain.wipememories().await.unwrap(), 1);
        assert!(brain.search("", 100).await.unwrap().is_empty());
    }
}
