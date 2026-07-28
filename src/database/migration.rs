use crate::database::{backup, version};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::path::Path;

pub const LATEST: i64 = 1;

struct Migration {
    version: i64,
    statements: &'static [&'static str],
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    statements: &[
        "CREATE VIRTUAL TABLE IF NOT EXISTS memory USING fts5(\
         body, source UNINDEXED, created UNINDEXED, tokenize='unicode61')",
        "CREATE TABLE IF NOT EXISTS setting(\
         key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS vault(\
         id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, created INTEGER NOT NULL)",
        "CREATE TABLE IF NOT EXISTS secret(\
         id INTEGER PRIMARY KEY AUTOINCREMENT, vaultid INTEGER NOT NULL REFERENCES vault(id) ON DELETE CASCADE, \
         name TEXT NOT NULL, env TEXT NOT NULL, account TEXT NOT NULL UNIQUE, created INTEGER NOT NULL, \
         UNIQUE(vaultid, name), UNIQUE(vaultid, env))",
        "CREATE TABLE IF NOT EXISTS globalenv(\
         env TEXT PRIMARY KEY, secretid INTEGER NOT NULL UNIQUE REFERENCES secret(id) ON DELETE CASCADE)",
        "CREATE TABLE IF NOT EXISTS trust(\
         path TEXT PRIMARY KEY, digest TEXT NOT NULL, updated INTEGER NOT NULL)",
    ],
}];

pub async fn run(pool: &SqlitePool, path: &Path, existed: bool) -> Result<()> {
    let current = version(pool).await?;
    anyhow::ensure!(
        current <= LATEST,
        "database version {current} is newer than this Synaps release supports"
    );
    if current == LATEST {
        return Ok(());
    }
    if existed {
        backup::create(pool, path, &format!("v{current}"))
            .await
            .context("could not create the pre-migration backup")?;
    }

    let mut transaction = pool.begin().await?;
    let current = sqlx::query_scalar::<_, i64>("PRAGMA user_version")
        .fetch_one(&mut *transaction)
        .await?;
    for migration in MIGRATIONS.iter().filter(|item| item.version > current) {
        for statement in migration.statements {
            sqlx::query(statement).execute(&mut *transaction).await?;
        }
        sqlx::query(&format!("PRAGMA user_version = {}", migration.version))
            .execute(&mut *transaction)
            .await?;
    }
    transaction
        .commit()
        .await
        .context("could not commit database migrations")
}
