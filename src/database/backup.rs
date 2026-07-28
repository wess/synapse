use crate::database::permission;
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn create(pool: &SqlitePool, database: &Path, label: &str) -> Result<PathBuf> {
    let parent = database.parent().context("database path has no parent")?;
    let folder = parent.join("backups");
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("could not create {}", folder.display()))?;
    permission::securedirectory(&folder)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_millis();
    let path = folder.join(format!("brain.{stamp}.{label}.db"));
    vacuum(pool, &path).await?;
    permission::securefile(&path)?;
    Ok(path)
}

pub async fn vacuum(pool: &SqlitePool, target: &Path) -> Result<()> {
    anyhow::ensure!(!target.exists(), "{} already exists", target.display());
    sqlx::query("VACUUM INTO ?")
        .bind(target.display().to_string())
        .execute(pool)
        .await
        .with_context(|| format!("could not write {}", target.display()))?;
    Ok(())
}
