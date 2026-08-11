//! The log. One table, append-only, and every row opaque.
//!
//! The client's `database` module is deliberately not shared here. Its
//! migrations describe memory, vault, and mesh tables this process has no
//! business holding, and a crate straddling both would serve neither. What is
//! copied is the set of habits worth keeping: owner-only permissions, a lock
//! so two servers cannot open one file, an integrity check before use, a
//! backup before any migration, and a bound on how many backups pile up.

use anyhow::{Context, Result};
use fs2::FileExt;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use synapsesync::{Numbered, Op};

/// Schema version. Append a new entry to `MIGRATIONS` and bump this; never
/// edit one that has shipped.
const LATEST: i64 = 2;

const MIGRATIONS: &[&[&str]] = &[
    // v1
    &[
        // seq is AUTOINCREMENT rather than a bare INTEGER PRIMARY KEY on
        // purpose: a plain rowid is reused after the highest row is removed,
        // and a reused sequence number silently hides an operation from every
        // client already past it.
        //
        // opid is UNIQUE, which is what makes a retried push idempotent. It is
        // the operation's identity rather than the memory's, so the put and
        // the delete of one memory are separate rows and neither can overwrite
        // the other.
        "CREATE TABLE op(\
         seq INTEGER PRIMARY KEY AUTOINCREMENT, \
         opid TEXT NOT NULL UNIQUE, \
         envelope BLOB NOT NULL, \
         received INTEGER NOT NULL)",
    ],
    // v2: more than one log in one file.
    &[
        // Who a log belongs to. The token is stored as a digest, so a stolen
        // database is not a set of working credentials.
        "CREATE TABLE tenant(\
         id INTEGER PRIMARY KEY AUTOINCREMENT, \
         label TEXT NOT NULL, \
         tokenhash TEXT NOT NULL UNIQUE, \
         created INTEGER NOT NULL)",
        // `opid` was UNIQUE across the whole table, which was right while
        // there was one log and is a data-loss bug the moment there is more
        // than one: an identity is derived from a memory's content, so two
        // people who record the same sentence in the same scope produce the
        // same opid — and `INSERT OR IGNORE` would silently drop the second
        // one's copy. The constraint has to be per tenant.
        //
        // SQLite cannot widen a UNIQUE constraint in place, so the table is
        // rebuilt. Existing rows belong to tenant 1, which is the tenant the
        // server's own configured token is given on startup.
        "CREATE TABLE opnext(\
         seq INTEGER PRIMARY KEY AUTOINCREMENT, \
         tenant INTEGER NOT NULL, \
         opid TEXT NOT NULL, \
         envelope BLOB NOT NULL, \
         received INTEGER NOT NULL, \
         UNIQUE(tenant, opid))",
        "INSERT INTO opnext(seq, tenant, opid, envelope, received) \
         SELECT seq, 1, opid, envelope, received FROM op",
        "DROP TABLE op",
        "ALTER TABLE opnext RENAME TO op",
        // Every read is "this tenant, above this cursor".
        "CREATE INDEX op_tenant_seq ON op(tenant, seq)",
    ],
];

/// How many pre-migration backups to keep.
const BACKUPS: usize = 3;

pub struct Store {
    pool: SqlitePool,
    // Held for the life of the process so a second server cannot open the same
    // file and hand out conflicting sequence numbers. Dropped on shutdown.
    _lock: File,
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        let existed = prepare(path)?;
        let lock = exclusivelock(path)?;

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .with_context(|| format!("could not open {}", path.display()))?;

        integrity(&pool, existed).await?;
        migrate(&pool, path, existed).await?;
        securefiles(path)?;
        Ok(Self { pool, _lock: lock })
    }

    /// Store operations this log has not seen, and report how many were new.
    ///
    /// A push the client repeats after a dropped response accepts nothing the
    /// second time and is not an error: delivery is at-least-once, so a client
    /// that cannot tell whether its request landed has to be free to send it
    /// again.
    pub async fn push(&self, tenant: i64, ops: &[Op]) -> Result<(usize, i64)> {
        let received = now()?;
        let mut transaction = self.pool.begin().await?;
        let mut accepted = 0;
        for op in ops {
            let result = sqlx::query(
                "INSERT OR IGNORE INTO op(tenant, opid, envelope, received) \
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(tenant)
            .bind(&op.opid)
            .bind(&op.envelope)
            .bind(received)
            .execute(&mut *transaction)
            .await
            .context("could not store an operation")?;
            accepted += result.rows_affected() as usize;
        }
        let head = head(&mut *transaction, tenant).await?;
        transaction.commit().await?;
        Ok((accepted, head))
    }

    /// Everything numbered above `since`, oldest first.
    ///
    /// Returns whether more remain, so a client advances its cursor and asks
    /// again rather than reading a short page as the end of the log.
    pub async fn pull(
        &self,
        tenant: i64,
        since: i64,
        limit: u32,
    ) -> Result<(Vec<Numbered>, i64, bool)> {
        let limit = limit.clamp(1, synapsesync::wire::MAXOPS as u32);
        // One more than asked for, to learn whether another page exists
        // without a second query.
        let mut rows: Vec<Numbered> = sqlx::query_as::<_, (i64, String, Vec<u8>)>(
            "SELECT seq, opid, envelope FROM op \
             WHERE tenant = ?1 AND seq > ?2 ORDER BY seq ASC LIMIT ?3",
        )
        .bind(tenant)
        .bind(since)
        .bind(limit as i64 + 1)
        .fetch_all(&self.pool)
        .await
        .context("could not read the log")?
        .into_iter()
        .map(|(seq, opid, envelope)| Numbered {
            seq,
            opid,
            envelope,
        })
        .collect();

        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let head = head(&self.pool, tenant).await?;
        Ok((rows, head, more))
    }

    pub async fn head(&self, tenant: i64) -> Result<i64> {
        head(&self.pool, tenant).await
    }

    /// The tenant a presented token belongs to, or `None`.
    ///
    /// The digest is the lookup key, so the table can be read by anyone who
    /// gets the file and still yields no usable credential.
    pub async fn tenantfor(&self, presented: &str) -> Result<Option<i64>> {
        sqlx::query_scalar::<_, i64>("SELECT id FROM tenant WHERE tokenhash = ?1")
            .bind(digest(presented))
            .fetch_optional(&self.pool)
            .await
            .context("could not look up the tenant")
    }

    /// Adds a tenant, or returns the one this token already names.
    ///
    /// Idempotent because the server calls it on every startup for its own
    /// configured token: an existing deployment keeps the log it already has
    /// rather than starting a second one beside it.
    pub async fn addtenant(&self, label: &str, token: &str) -> Result<i64> {
        if let Some(existing) = self.tenantfor(token).await? {
            return Ok(existing);
        }
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO tenant(label, tokenhash, created) VALUES (?1, ?2, ?3) RETURNING id",
        )
        .bind(label)
        .bind(digest(token))
        .bind(now()?)
        .fetch_one(&self.pool)
        .await
        .context("could not add a tenant")?;
        Ok(id)
    }
}

/// Hex SHA-256, which is what the tenant table stores in place of a token.
fn digest(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// The furthest this tenant has been written to.
///
/// Per tenant, not global: `seq` is one sequence shared by every log in the
/// file, so a client told the global head would believe it was behind by
/// however many operations other tenants had written, ask for them, and be
/// handed nothing — forever. Gaps in a tenant's own numbering are harmless;
/// a cursor only has to increase.
async fn head<'c, E>(executor: E, tenant: i64) -> Result<i64>
where
    E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(seq), 0) FROM op WHERE tenant = ?1")
        .bind(tenant)
        .fetch_one(executor)
        .await
        .context("could not read the head of the log")
}

async fn integrity(pool: &SqlitePool, existed: bool) -> Result<()> {
    if !existed {
        return Ok(());
    }
    let result = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_one(pool)
        .await
        .context("could not check the log")?;
    anyhow::ensure!(
        result == "ok",
        "the log failed its integrity check: {result}"
    );
    Ok(())
}

async fn migrate(pool: &SqlitePool, path: &Path, existed: bool) -> Result<()> {
    let current = sqlx::query_scalar::<_, i64>("PRAGMA user_version")
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        current <= LATEST,
        "this log is version {current} and this build understands {LATEST}"
    );
    if current == LATEST {
        return Ok(());
    }
    if existed {
        backup(pool, path, current).await?;
    }

    let mut transaction = pool.begin().await?;
    for (index, statements) in MIGRATIONS.iter().enumerate() {
        let version = index as i64 + 1;
        if version <= current {
            continue;
        }
        for statement in *statements {
            sqlx::query(statement).execute(&mut *transaction).await?;
        }
        sqlx::query(&format!("PRAGMA user_version = {version}"))
            .execute(&mut *transaction)
            .await?;
    }
    transaction
        .commit()
        .await
        .context("could not apply migrations to the log")
}

async fn backup(pool: &SqlitePool, path: &Path, version: i64) -> Result<()> {
    let target = PathBuf::from(format!("{}.v{version}.{}.backup", path.display(), now()?));
    sqlx::query("VACUUM INTO ?1")
        .bind(target.to_string_lossy().as_ref())
        .execute(pool)
        .await
        .with_context(|| format!("could not back up to {}", target.display()))?;
    securefile(&target)?;
    prune(path)
}

/// Keep the newest few and drop the rest. Anything the server writes without a
/// bound eventually fills the disk it is running on.
fn prune(path: &Path) -> Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok(());
    };
    let prefix = format!("{}.v", name.to_string_lossy());
    let mut found: Vec<PathBuf> = fs::read_dir(parent)
        .with_context(|| format!("could not read {}", parent.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|item| {
            item.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
                && item.extension().is_some_and(|item| item == "backup")
        })
        .collect();
    if found.len() <= BACKUPS {
        return Ok(());
    }
    found.sort();
    for stale in &found[..found.len() - BACKUPS] {
        let _ = fs::remove_file(stale);
    }
    Ok(())
}

fn prepare(path: &Path) -> Result<bool> {
    let existed = path.is_file() && fs::metadata(path).is_ok_and(|item| item.len() > 0);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
        securedirectory(parent)?;
    }
    Ok(existed)
}

fn exclusivelock(path: &Path) -> Result<File> {
    let target = path.with_extension("lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&target)
        .with_context(|| format!("could not open {}", target.display()))?;
    securefile(&target)?;
    FileExt::try_lock_exclusive(&file).context("another synapseserve is already using this log")?;
    Ok(file)
}

fn securefiles(path: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let item = PathBuf::from(format!("{}{suffix}", path.display()));
        if item.is_file() {
            securefile(&item)?;
        }
    }
    Ok(())
}

fn securefile(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("could not secure {}", path.display()))?;
    }
    Ok(())
}

fn securedirectory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("could not secure {}", path.display()))?;
    }
    Ok(())
}

fn now() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapsesync::Op;

    /// A log written by the version that had one tenant and no tenant column.
    async fn version1(path: &Path) {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        for statement in MIGRATIONS[0] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO op(opid, envelope, received) VALUES ('a', X'01', 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA user_version = 1")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn a_log_written_before_tenants_keeps_every_operation() {
        // The migration rebuilds `op` to widen a UNIQUE constraint, which is
        // the one migration here that could lose somebody's memories. What
        // was already in the file belongs to the first tenant — the one the
        // server's own configured token is given on startup.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("log.db");
        version1(&path).await;

        let store = Store::open(&path).await.unwrap();
        let mine = store
            .addtenant("configured", "0123456789abcdef0123456789abcdef")
            .await
            .unwrap();
        assert_eq!(mine, 1, "the configured token has to inherit the old log");

        let (ops, head, _) = store.pull(mine, 0, 10).await.unwrap();
        assert_eq!(ops.len(), 1, "an operation was lost in the migration");
        assert_eq!(ops[0].opid, "a");
        assert_eq!(head, 1);
    }

    #[tokio::test]
    async fn the_sequence_does_not_restart_after_the_rebuild() {
        // `seq` is AUTOINCREMENT so a number is never handed out twice. If the
        // rebuild reset the sequence, the next operation would take a number a
        // client is already past and would never be pulled.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("log.db");
        version1(&path).await;

        let store = Store::open(&path).await.unwrap();
        let mine = store
            .addtenant("configured", "0123456789abcdef0123456789abcdef")
            .await
            .unwrap();
        let op = Op {
            opid: "b".into(),
            envelope: vec![2],
        };
        let (accepted, head) = store.push(mine, std::slice::from_ref(&op)).await.unwrap();
        assert_eq!(accepted, 1);
        assert!(head > 1, "the sequence restarted: {head}");
    }
}
