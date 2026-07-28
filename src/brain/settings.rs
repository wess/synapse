use crate::brain::{Optimization, Settings};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::str::FromStr;

pub async fn read(pool: &SqlitePool) -> Result<Settings> {
    let value = value(pool, "optimization")
        .await?
        .unwrap_or_else(|| Optimization::Balanced.value().to_owned());
    Ok(Optimization::from_str(&value)
        .context("stored optimization setting is invalid")?
        .into())
}

pub async fn write(pool: &SqlitePool, optimization: Optimization) -> Result<()> {
    writevalue(pool, "optimization", optimization.value()).await
}

pub async fn value(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    sqlx::query_scalar::<_, String>("SELECT value FROM setting WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub async fn writevalue(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO setting(key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}
