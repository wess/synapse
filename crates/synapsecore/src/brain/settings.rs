use crate::brain::{Optimization, Settings};
pub use crate::relay::{DEFAULTWORKERS, WORKERCEILING};
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

/// The most background workers one session may run at once.
///
/// Read at spawn time rather than baked in, because how many agents a machine
/// can usefully carry is a fact about the machine. Clamped to [`WORKERCEILING`]
/// on the way out: the setting is a preference, and the ceiling is the promise
/// that a supervisor which has talked itself into wanting forty agents still
/// cannot have them.
pub async fn maxworkers(pool: &SqlitePool) -> Result<usize> {
    let stored = value(pool, "maxworkers")
        .await?
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULTWORKERS);
    Ok(stored.clamp(1, WORKERCEILING))
}

pub async fn writemaxworkers(pool: &SqlitePool, workers: usize) -> Result<()> {
    anyhow::ensure!(
        (1..=WORKERCEILING).contains(&workers),
        "the worker limit has to be between 1 and {WORKERCEILING}"
    );
    writevalue(pool, "maxworkers", &workers.to_string()).await
}

/// Whether the console draws its reactor. On unless somebody turned it off:
/// the fact strip beside it says the same things, so this is a preference about
/// what a window looks like rather than about what it can do.
pub async fn reactor(pool: &SqlitePool) -> Result<bool> {
    Ok(value(pool, "reactor").await?.as_deref() != Some("off"))
}

pub async fn writereactor(pool: &SqlitePool, enabled: bool) -> Result<()> {
    writevalue(pool, "reactor", if enabled { "on" } else { "off" }).await
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> (SqlitePool, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let opened = crate::database::open(&directory.path().join("brain.db"))
            .await
            .unwrap();
        (opened.pool, directory)
    }

    #[tokio::test]
    async fn the_worker_limit_defaults_and_then_follows_the_setting() {
        let (pool, _directory) = pool().await;
        assert_eq!(maxworkers(&pool).await.unwrap(), DEFAULTWORKERS);

        writemaxworkers(&pool, 3).await.unwrap();
        assert_eq!(maxworkers(&pool).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn the_ceiling_holds_whatever_the_setting_says() {
        let (pool, _directory) = pool().await;

        // Refused on the way in...
        assert!(writemaxworkers(&pool, WORKERCEILING + 1).await.is_err());
        assert!(writemaxworkers(&pool, 0).await.is_err());

        // ...and clamped on the way out, so a row written by anything other
        // than that function — a hand-edited database, an older build — still
        // cannot buy more agents than the ceiling allows.
        writevalue(&pool, "maxworkers", "5000").await.unwrap();
        assert_eq!(maxworkers(&pool).await.unwrap(), WORKERCEILING);

        writevalue(&pool, "maxworkers", "0").await.unwrap();
        assert_eq!(maxworkers(&pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn a_setting_that_is_not_a_number_reads_as_the_default() {
        let (pool, _directory) = pool().await;
        writevalue(&pool, "maxworkers", "lots").await.unwrap();
        assert_eq!(maxworkers(&pool).await.unwrap(), DEFAULTWORKERS);
    }

    #[tokio::test]
    async fn the_reactor_is_on_until_it_is_turned_off() {
        let (pool, _directory) = pool().await;
        assert!(reactor(&pool).await.unwrap());

        writereactor(&pool, false).await.unwrap();
        assert!(!reactor(&pool).await.unwrap());
        writereactor(&pool, true).await.unwrap();
        assert!(reactor(&pool).await.unwrap());

        // Only the exact word turns it off. A setting written by hand, or by an
        // older build, should not silently mean "off".
        writevalue(&pool, "reactor", "maybe").await.unwrap();
        assert!(reactor(&pool).await.unwrap());
    }
}
