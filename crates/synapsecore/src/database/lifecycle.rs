use crate::database::{backup, connect, integrity, migration, permission, readonly, version};
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
pub struct Report {
    pub path: String,
    pub version: i64,
    pub integrity: &'static str,
}

pub async fn check(path: &Path) -> Result<Report> {
    let opened = super::open(path).await?;
    integrity(&opened.pool).await?;
    Ok(Report {
        path: path.display().to_string(),
        version: version(&opened.pool).await?,
        integrity: "ok",
    })
}

pub async fn export(database: &Path, target: &Path) -> Result<()> {
    anyhow::ensure!(!target.exists(), "{} already exists", target.display());
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let opened = super::open(database).await?;
    backup::vacuum(&opened.pool, target).await?;
    permission::securefile(target)?;
    let exported = readonly(target).await?;
    integrity(&exported).await?;
    exported.close().await;
    Ok(())
}

pub async fn restore(database: &Path, source: &Path) -> Result<Option<PathBuf>> {
    anyhow::ensure!(source.is_file(), "{} is not a file", source.display());
    let sourcepool = readonly(source).await?;
    integrity(&sourcepool).await?;
    let sourceversion = version(&sourcepool).await?;
    anyhow::ensure!(
        sourceversion == migration::LATEST,
        "backup version {sourceversion} is not supported; expected {}",
        migration::LATEST
    );

    let existed = permission::prepare(database)?;
    let _lock = permission::exclusivelock(database)?;
    let backup = if existed {
        let current = connect(database, crate::database::Schema::Brain).await?;
        integrity(&current).await?;
        migration::run(&current, database, true, crate::database::Schema::Brain).await?;
        let backup = backup::create(&current, database, "restore").await?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&current)
            .await?;
        current.close().await;
        Some(backup)
    } else {
        None
    };

    let parent = database.parent().context("database path has no parent")?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_millis();
    let temporary = parent.join(format!("brain.{stamp}.restore.db"));
    backup::vacuum(&sourcepool, &temporary).await?;
    sourcepool.close().await;
    permission::securefile(&temporary)?;
    File::open(&temporary)?.sync_all()?;

    for sidecar in [
        permission::sidecar(database, "wal"),
        permission::sidecar(database, "shm"),
    ] {
        if sidecar.is_file() {
            fs::remove_file(&sidecar)
                .with_context(|| format!("could not remove {}", sidecar.display()))?;
        }
    }
    replace(&temporary, database)?;
    permission::securefiles(database)?;
    File::open(parent)?.sync_all()?;
    Ok(backup)
}

fn replace(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("could not replace {}", target.display()))?;
    }
    fs::rename(source, target).with_context(|| format!("could not replace {}", target.display()))
}
