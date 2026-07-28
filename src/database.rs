mod backup;
mod lifecycle;
mod migration;
mod permission;
#[cfg(test)]
mod tests;

pub use lifecycle::{check, export, restore};

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::fs::File;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

pub struct Opened {
    pub pool: SqlitePool,
    pub lock: Arc<File>,
}

pub async fn open(path: &Path) -> Result<Opened> {
    let existed = permission::prepare(path)?;
    let lock = Arc::new(permission::sharedlock(path)?);
    let pool = connect(path).await?;
    integrity(&pool).await?;
    migration::run(&pool, path, existed).await?;
    permission::securefiles(path)?;
    Ok(Opened { pool, lock })
}

async fn connect(path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite://synapse")?
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .with_context(|| format!("could not open {}", path.display()))
}

async fn readonly(path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite://synapse")?
        .filename(path)
        .read_only(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("could not open {}", path.display()))
}

async fn integrity(pool: &SqlitePool) -> Result<()> {
    let results = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_all(pool)
        .await
        .context("could not check database integrity")?;
    anyhow::ensure!(
        results.len() == 1 && results[0] == "ok",
        "database integrity check failed: {}",
        results.join("; ")
    );
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .context("could not check database relationships")?;
    anyhow::ensure!(
        violations.is_empty(),
        "database relationship check failed with {} violation(s)",
        violations.len()
    );
    Ok(())
}

async fn version(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .context("could not read database version")
}
