use crate::database::{backup, version};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::path::Path;

pub const LATEST: i64 = 5;

struct Migration {
    version: i64,
    statements: &'static [&'static str],
}

const MIGRATIONS: &[Migration] = &[
    Migration {
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
    },
    Migration {
        version: 2,
        statements: &[
            "CREATE TABLE memorymeta(\
             memoryid INTEGER PRIMARY KEY, \
             scope TEXT NOT NULL CHECK(scope IN ('global', 'project')), \
             project TEXT NOT NULL DEFAULT '', \
             native INTEGER NOT NULL DEFAULT 1 CHECK(native IN (0, 1)))",
            "INSERT INTO memorymeta(memoryid, scope, project, native) \
             SELECT rowid, 'global', '', 1 FROM memory",
            "CREATE INDEX memorymetascope ON memorymeta(scope, project)",
            "CREATE TABLE importbatch(\
             id INTEGER PRIMARY KEY AUTOINCREMENT, provider TEXT NOT NULL, created INTEGER NOT NULL, \
             imported INTEGER NOT NULL DEFAULT 0, linked INTEGER NOT NULL DEFAULT 0, \
             skipped INTEGER NOT NULL DEFAULT 0, flagged INTEGER NOT NULL DEFAULT 0, \
             undone INTEGER NOT NULL DEFAULT 0 CHECK(undone IN (0, 1)))",
            "CREATE TABLE memoryorigin(\
             id INTEGER PRIMARY KEY AUTOINCREMENT, \
             memoryid INTEGER NOT NULL REFERENCES memorymeta(memoryid) ON DELETE CASCADE, \
             batchid INTEGER NOT NULL REFERENCES importbatch(id) ON DELETE CASCADE, \
             provider TEXT NOT NULL, externalid TEXT NOT NULL, digest TEXT NOT NULL, path TEXT NOT NULL, \
             UNIQUE(provider, externalid))",
            "CREATE INDEX memoryoriginbatch ON memoryorigin(batchid)",
        ],
    },
    Migration {
        version: 3,
        statements: &[
            "CREATE TABLE meshagent(\
             name TEXT PRIMARY KEY, \
             role TEXT NOT NULL DEFAULT '', \
             capabilities TEXT NOT NULL DEFAULT '', \
             project TEXT NOT NULL DEFAULT '', \
             tool TEXT NOT NULL DEFAULT '', \
             cursor INTEGER NOT NULL DEFAULT 0, \
             registered INTEGER NOT NULL DEFAULT 1 CHECK(registered IN (0, 1)), \
             status TEXT NOT NULL DEFAULT '', \
             seen INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE meshsub(\
             agent TEXT NOT NULL, channel TEXT NOT NULL, PRIMARY KEY(agent, channel))",
            "CREATE INDEX meshsubchannel ON meshsub(channel)",
            "CREATE TABLE meshmessage(\
             id INTEGER PRIMARY KEY AUTOINCREMENT, \
             sender TEXT NOT NULL, \
             kind TEXT NOT NULL CHECK(kind IN ('direct', 'channel', 'broadcast')), \
             target TEXT, \
             body TEXT NOT NULL, \
             created INTEGER NOT NULL)",
            "CREATE INDEX meshmessagetarget ON meshmessage(target, id)",
            "CREATE TABLE meshworker(\
             name TEXT PRIMARY KEY, \
             role TEXT NOT NULL DEFAULT '', \
             program TEXT NOT NULL, \
             arguments TEXT NOT NULL DEFAULT '[]', \
             directory TEXT NOT NULL DEFAULT '', \
             keepalive INTEGER NOT NULL DEFAULT 1 CHECK(keepalive IN (0, 1)), \
             session TEXT, \
             supervisor INTEGER NOT NULL DEFAULT 0, \
             process INTEGER NOT NULL DEFAULT 0, \
             log TEXT NOT NULL DEFAULT '', \
             status TEXT NOT NULL DEFAULT '', \
             restarts INTEGER NOT NULL DEFAULT 0, \
             created INTEGER NOT NULL)",
        ],
    },
    Migration {
        version: 4,
        statements: &[
            // What Synapse copied where, so a later run can tell its own work
            // from a skill somebody wrote by hand under the same name.
            "CREATE TABLE skillinstall(\
             skill TEXT NOT NULL, \
             tool TEXT NOT NULL, \
             path TEXT NOT NULL, \
             digest TEXT NOT NULL, \
             source TEXT NOT NULL DEFAULT '', \
             installed INTEGER NOT NULL, \
             PRIMARY KEY(skill, tool))",
        ],
    },
    Migration {
        version: 5,
        statements: &[
            // When a memory was stored, kept beside its scope so the most
            // recent can be found from an index. It lives in the FTS table too,
            // but only as an unindexed column, so ordering by it there meant
            // reading every memory and sorting the lot to answer a question
            // about the newest handful.
            "ALTER TABLE memorymeta ADD COLUMN created INTEGER NOT NULL DEFAULT 0",
            "UPDATE memorymeta SET created = COALESCE(\
             (SELECT CAST(memory.created AS INTEGER) FROM memory \
             WHERE memory.rowid = memorymeta.memoryid), 0)",
            "CREATE INDEX memorymetacreated ON memorymeta(created DESC, memoryid DESC)",
        ],
    },
];

pub async fn run(pool: &SqlitePool, path: &Path, existed: bool) -> Result<()> {
    let current = version(pool).await?;
    anyhow::ensure!(
        current <= LATEST,
        "database version {current} is newer than this Synapse release supports"
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
