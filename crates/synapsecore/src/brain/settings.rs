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

/// Whether the agent mesh is switched on. Off by default: its tools cost
/// context in every session that loads them, so a user who does not run agent
/// teams never sees them.
pub async fn mesh(pool: &SqlitePool) -> Result<bool> {
    Ok(value(pool, "mesh").await?.as_deref() == Some("on"))
}

pub async fn writemesh(pool: &SqlitePool, enabled: bool) -> Result<()> {
    writevalue(pool, "mesh", if enabled { "on" } else { "off" }).await
}

/// Whether agents may write skills. Off by default for the same reason the mesh
/// is: two more tool definitions in every session, and guidance to go with
/// them, that a user who does not want agents editing a library should never
/// have to pay for.
pub async fn learn(pool: &SqlitePool) -> Result<bool> {
    Ok(value(pool, "learn").await?.as_deref() == Some("on"))
}

pub async fn writelearn(pool: &SqlitePool, enabled: bool) -> Result<()> {
    writevalue(pool, "learn", if enabled { "on" } else { "off" }).await
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
