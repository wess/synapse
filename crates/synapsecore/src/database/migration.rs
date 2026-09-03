use crate::database::{Schema, backup, version};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::path::Path;

pub const LATEST: i64 = 12;

/// The vault's own schema, in its own file. One table today, with the numbered
/// migration around it anyway: the day it grows a column is not the day to
/// start keeping track of what a file has already had done to it.
pub const VAULTLATEST: i64 = 1;

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
    Migration {
        version: 6,
        statements: &[
            // Whether this roster row is a person rather than an agent. Agents
            // are told a human is reachable and by what name, and the roster
            // has to be able to say which name that is — an agent that mistakes
            // a person for a worker delegates to them and parks.
            "ALTER TABLE meshagent ADD COLUMN human INTEGER NOT NULL DEFAULT 0 \
             CHECK(human IN (0, 1))",
        ],
    },
    Migration {
        version: 7,
        statements: &[
            // What an agent is doing, beside the state it is in. `working` says
            // a worker has not stalled; it does not say what it is working on,
            // and the only place that was legible before was its own log — which
            // meant reading another tool's stream format to find out.
            "ALTER TABLE meshagent ADD COLUMN note TEXT NOT NULL DEFAULT ''",
        ],
    },
    Migration {
        version: 8,
        statements: &[
            // What sync knows about a memory, beside the memory.
            //
            // Its own table rather than columns on `memory`, which is an FTS5
            // virtual table and a poor place for anything that is not searched,
            // and rather than `memorymeta`, which describes what a memory *is*
            // where this describes what has been done with it.
            //
            // `opid` is stored rather than recomputed. It is derived from the
            // record's content, so it could be worked out again on demand — but
            // applying a remote deletion would then mean recomputing an identity
            // for every memory in the store to find the one being removed, and
            // any later change to how identities are derived would quietly stop
            // deletions matching anything at all.
            //
            // `pushed` exists so a sync sends what is new instead of the whole
            // store. Re-sending everything would be correct — the server rejects
            // an identity it already holds — and would move the entire library
            // on every run.
            "CREATE TABLE memorysync(\
             memoryid INTEGER PRIMARY KEY REFERENCES memorymeta(memoryid) ON DELETE CASCADE, \
             opid TEXT NOT NULL UNIQUE, \
             pushed INTEGER NOT NULL DEFAULT 0 CHECK(pushed IN (0, 1)))",
            "CREATE INDEX memorysync_unpushed ON memorysync(pushed) WHERE pushed = 0",
        ],
    },
    Migration {
        version: 9,
        statements: &[
            // Which memory replaced this one, or 0 while it still stands.
            //
            // A correction used to be a second memory contradicting the first,
            // with nothing to say which one was current — recall returned both
            // and the ranking decided, so an agent could act on the version its
            // author had already retracted. Pointing at the replacement rather
            // than setting a flag means `memory show` can name it, and it costs
            // no second column.
            //
            // Superseded is not deleted. The memory stays readable, keeps its
            // id, and comes back with `memory restore` — the whole point of
            // hiding rather than removing is that a wrong correction is
            // reversible.
            "ALTER TABLE memorymeta ADD COLUMN superseded INTEGER NOT NULL DEFAULT 0",
            // Partial, because the rows worth finding this way are the live
            // ones and almost every row is live.
            "CREATE INDEX memorymetalive ON memorymeta(scope, project) WHERE superseded = 0",
        ],
    },
    Migration {
        version: 10,
        statements: &[
            // Which shelf an installed copy came from. A skill can now belong to
            // one project as well as to everybody, and the same name can exist
            // on both shelves at once in two different folders — so the tool and
            // the name no longer identify a copy on their own. SQLite cannot
            // widen a primary key in place, hence the rebuild.
            "ALTER TABLE skillinstall RENAME TO skillinstallold",
            "CREATE TABLE skillinstall(\
             shelf TEXT NOT NULL DEFAULT '', \
             skill TEXT NOT NULL, \
             tool TEXT NOT NULL, \
             path TEXT NOT NULL, \
             digest TEXT NOT NULL, \
             source TEXT NOT NULL DEFAULT '', \
             installed INTEGER NOT NULL, \
             PRIMARY KEY(shelf, skill, tool))",
            "INSERT INTO skillinstall(shelf, skill, tool, path, digest, source, installed) \
             SELECT '', skill, tool, path, digest, source, installed FROM skillinstallold",
            "DROP TABLE skillinstallold",
            // A skill an agent wrote, waiting for a person to approve it.
            //
            // The row *is* the proposal: approving deletes it and installs the
            // skill, rejecting deletes it and the skill with it. Nothing reaches
            // a tool's own skills folder while a row is here, which is what
            // makes teaching one free — a session that writes a mediocre skill
            // has cost the user a line in a list, not context in every session
            // on the machine.
            "CREATE TABLE skillproposal(\
             shelf TEXT NOT NULL DEFAULT '', \
             skill TEXT NOT NULL, \
             project TEXT NOT NULL DEFAULT '', \
             tool TEXT NOT NULL DEFAULT '', \
             note TEXT NOT NULL DEFAULT '', \
             created INTEGER NOT NULL, \
             PRIMARY KEY(shelf, skill))",
            // What a skill said before it was revised.
            //
            // A revision reaches every copy Synapse installed, so it changes how
            // sessions behave without anybody watching it happen. Keeping the
            // previous text is what makes that reversible, and it is the same
            // bargain `memorymeta.superseded` makes for a corrected memory:
            // nothing is hidden without a way back.
            "CREATE TABLE skillrevision(\
             id INTEGER PRIMARY KEY AUTOINCREMENT, \
             shelf TEXT NOT NULL DEFAULT '', \
             skill TEXT NOT NULL, \
             body TEXT NOT NULL, \
             note TEXT NOT NULL DEFAULT '', \
             tool TEXT NOT NULL DEFAULT '', \
             created INTEGER NOT NULL)",
            "CREATE INDEX skillrevisionskill ON skillrevision(shelf, skill, id DESC)",
        ],
    },
    Migration {
        version: 11,
        statements: &[
            // Where secret values are kept became a choice in this release, and
            // the answer for a machine that already has some is settled here
            // rather than guessed at every read: anything stored before now is
            // in the Keychain, because that is the only place there was. A
            // fresh install runs this against an empty table, writes nothing,
            // and gets the encrypted store. Either way `vault migrate` is what
            // changes the answer, and it moves the values with it.
            "INSERT OR IGNORE INTO setting(key, value) \
             SELECT 'vault.backend', 'keychain' WHERE EXISTS(SELECT 1 FROM secret)",
        ],
    },
    Migration {
        version: 12,
        statements: &[
            // What a connection was made from, so a descriptor that moves in a
            // later release can say so instead of waiting to be guessed at. The
            // digest is of the descriptor text as it stood when Synapse set the
            // tool up; a different one now means this release would connect it
            // differently. Same bargain `skillinstall` makes for a skill.
            //
            // A machine that connected its tools before this release has no
            // rows here, and an absent row reads as "nothing to compare",
            // never as out of date — inventing a stale connection for every
            // existing user would be a worse lie than saying nothing.
            "CREATE TABLE IF NOT EXISTS toolconnect(             slug TEXT PRIMARY KEY, descriptor TEXT NOT NULL, connected INTEGER NOT NULL)",
        ],
    },
];

/// `vault.db`. One row per secret, holding a sealed envelope and nothing else:
/// the account name is already in `brain.db`, and repeating it here is what
/// lets the envelope authenticate the name it was sealed under.
const VAULTMIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    statements: &["CREATE TABLE IF NOT EXISTS secretvalue(\
         account TEXT PRIMARY KEY, envelope BLOB NOT NULL, updated INTEGER NOT NULL)"],
}];

pub async fn run(pool: &SqlitePool, path: &Path, existed: bool, schema: Schema) -> Result<()> {
    let (migrations, latest) = match schema {
        Schema::Brain => (MIGRATIONS, LATEST),
        Schema::Vault => (VAULTMIGRATIONS, VAULTLATEST),
    };
    let current = version(pool).await?;
    // Name the way out. This is the one error a person cannot reason their way
    // through from the message alone: the store is fine, nothing is corrupt,
    // and the fix is to upgrade the thing reporting it — but "version 12 is
    // newer than this release supports" reads like a broken database and
    // sends people looking for a backup to restore. It happens whenever a
    // newer Synapse has opened the same folder: a development build, or an app
    // upgraded on one machine and syncing to another that was not.
    anyhow::ensure!(
        current <= latest,
        "this store is at schema {current} and Synapse {} only understands {latest}. \
         Something newer has opened it — upgrade Synapse and try again. \
         Your memory is intact, and a pre-migration copy is in the `backups` folder.",
        env!("CARGO_PKG_VERSION")
    );
    if current == latest {
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
    for migration in migrations.iter().filter(|item| item.version > current) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn oldstore(path: &Path, version: i64) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::from_str("sqlite://test")
                    .unwrap()
                    .filename(path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        for migration in MIGRATIONS.iter().filter(|item| item.version <= version) {
            for statement in migration.statements {
                sqlx::query(statement).execute(&pool).await.unwrap();
            }
        }
        sqlx::query(&format!("PRAGMA user_version = {version}"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    /// A machine that stored secrets before the encrypted vault existed has to
    /// keep resolving them, and one that stored none has no reason to be sent
    /// to a Keychain it may not have.
    #[tokio::test]
    async fn only_a_store_that_already_holds_secrets_is_pinned_to_the_keychain() {
        let directory = tempfile::tempdir().unwrap();

        let existing = directory.path().join("existing.db");
        let pool = oldstore(&existing, 10).await;
        sqlx::query("INSERT INTO vault(name, created) VALUES ('work', 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO secret(vaultid, name, env, account, created) \
             VALUES (1, 'token', 'TOKEN', 'work.token', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        run(&pool, &existing, true, Schema::Brain).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM setting WHERE key = 'vault.backend'"
            )
            .fetch_optional(&pool)
            .await
            .unwrap()
            .as_deref(),
            Some("keychain")
        );

        let empty = directory.path().join("empty.db");
        let pool = oldstore(&empty, 10).await;
        run(&pool, &empty, true, Schema::Brain).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM setting WHERE key = 'vault.backend'"
            )
            .fetch_optional(&pool)
            .await
            .unwrap(),
            None
        );
    }

    /// Every shipped migration has to survive being applied to a store that
    /// stopped at the version before it. Creating a fresh database only proves
    /// the statements parse; this proves they run against data somebody has.
    ///
    /// It is written against `LATEST - 1` rather than a fixed number, so it
    /// keeps testing the newest migration without being edited.
    #[tokio::test]
    async fn a_store_from_the_previous_release_migrates_with_its_memories_intact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("brain.db");
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::from_str("sqlite://test")
                    .unwrap()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        let previous = LATEST - 1;
        for migration in MIGRATIONS.iter().filter(|item| item.version <= previous) {
            for statement in migration.statements {
                sqlx::query(statement).execute(&pool).await.unwrap();
            }
        }
        sqlx::query(&format!("PRAGMA user_version = {previous}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO memory(body, source, created) VALUES ('older', 'test', 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO memorymeta(memoryid, scope, project, native, created) \
             VALUES (1, 'global', '', 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        run(&pool, &path, false, Schema::Brain).await.unwrap();

        assert_eq!(version(&pool).await.unwrap(), LATEST);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT body FROM memory WHERE rowid = 1")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "older"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT superseded FROM memorymeta WHERE memoryid = 1")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0,
            "a memory written before the column existed still stands"
        );
    }
}
