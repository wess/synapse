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
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub struct Opened {
    pub pool: SqlitePool,
    pub lock: Arc<File>,
}

/// Open the store for use: verified, migrated, and owner-only.
pub async fn open(path: &Path) -> Result<Opened> {
    opened(path, Scan::Whole).await
}

/// Open the store to report on it.
///
/// Identical to [`open`] except that it skips the whole-file scan. That scan
/// reads every page, so it costs time in proportion to everything stored, and
/// `statusline` runs on every turn of every connected session: a reporting
/// command would spend far longer asking whether the store is sound than
/// reading the one number it came for, and would spend longer the more the
/// person had stored. The relationship check still runs, damage that a query
/// actually reaches is still reported, and `synapse data check` is still the
/// way to ask the whole question.
pub async fn glance(path: &Path) -> Result<Opened> {
    opened(path, Scan::None).await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scan {
    Whole,
    None,
}

async fn opened(path: &Path, scan: Scan) -> Result<Opened> {
    // Snapshot the file before opening it, so that connecting — which touches
    // the write-ahead log whether or not anything is stored — cannot make the
    // store look changed to the next handle this process asks for.
    let state = identity(path);
    let existed = permission::prepare(path)?;
    let lock = Arc::new(permission::sharedlock(path)?);
    let pool = connect(path).await?;
    if unverified(path, state) {
        relationships(&pool).await?;
        if scan == Scan::Whole {
            pages(&pool).await?;
            verified(path, state);
        }
    }
    migration::run(&pool, path, existed).await?;
    permission::securefiles(path)?;
    Ok(Opened { pool, lock })
}

/// The size and modification time of the database and its write-ahead log:
/// enough to tell a store that has already been verified from one that has been
/// written since.
type Identity = [(u64, Option<SystemTime>); 2];

/// Databases scanned in this process, and the file state each scan covered. One
/// command opens the store more than once — memory, the vault, the mesh, and
/// skill receipts each want their own handle — and reading every page three
/// times to learn the same thing about the same bytes is the whole of what the
/// second and third opens cost.
static VERIFIED: Mutex<Vec<(PathBuf, Identity)>> = Mutex::new(Vec::new());

fn unverified(path: &Path, state: Identity) -> bool {
    let seen = VERIFIED.lock().unwrap_or_else(|error| error.into_inner());
    !seen
        .iter()
        .any(|(known, known_state)| known == path && *known_state == state)
}

fn verified(path: &Path, state: Identity) {
    let mut seen = VERIFIED.lock().unwrap_or_else(|error| error.into_inner());
    seen.retain(|(known, _)| known != path);
    seen.push((path.to_path_buf(), state));
}

fn identity(path: &Path) -> Identity {
    let mut state = [path.to_path_buf(), permission::sidecar(path, "wal")].map(|file| {
        let metadata = std::fs::metadata(&file).ok();
        (
            metadata.as_ref().map_or(0, std::fs::Metadata::len),
            metadata.and_then(|item| item.modified().ok()),
        )
    });
    // Opening a store creates its write-ahead log whether or not anything is
    // written, and a checkpoint empties it again, so an empty log's timestamp
    // moves on its own. An empty log holds no committed frames and so describes
    // no state to verify; timing it would make every command look changed.
    if state[1].0 == 0 {
        state[1].1 = None;
    }
    state
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
    pages(pool).await?;
    relationships(pool).await
}

/// Read every page and report damage. Costs time in proportion to the whole
/// store, so it belongs where the question is worth asking rather than on the
/// way to every answer.
async fn pages(pool: &SqlitePool) -> Result<()> {
    let results = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_all(pool)
        .await
        .context("could not check database integrity")?;
    anyhow::ensure!(
        results.len() == 1 && results[0] == "ok",
        "database integrity check failed: {}",
        results.join("; ")
    );
    Ok(())
}

/// Check that every reference still points at a row. Index-driven, so it stays
/// cheap however much is stored, and it runs on every open.
async fn relationships(pool: &SqlitePool) -> Result<()> {
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
