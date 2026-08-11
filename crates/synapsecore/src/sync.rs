//! Carrying memories between machines through a server that cannot read them.
//!
//! The server holds sealed envelopes and a sequence number. It cannot tell a
//! stored memory from a deleted one, cannot say how many anyone has, and
//! resolves nothing — clients do that, because only they can see the contents.
//! Everything that makes that true lives in `synapsesync`; this is the half
//! that reads and writes the local store.
//!
//! Nothing here is required to use Synapse. A machine with no server configured
//! keeps every memory locally and loses no capability, which is also why an
//! outage is not an emergency.
//!
//! ## Where the key lives
//!
//! In the local database, beside the memories it protects.
//!
//! That reads wrong for about a second and then stops: `brain.db` already holds
//! every memory in plaintext. A key stored next to them protects nothing that
//! was not already readable by anyone holding the file, and it is not what the
//! key is for. The key exists so the *server* cannot read what it stores —
//! including a server somebody else runs, which is the whole reason the
//! envelopes are sealed at all.
//!
//! Keychain would be marginally better and is worth doing when a device
//! keypair exists to seal it to. It would not change what an attacker with the
//! database can read today.

use crate::brain::{Brain, MemoryScope};
use anyhow::{Context, Result};
use synapsesync::{
    Key, Op, PROTOCOL, PullRequest, PullResponse, PushRequest, PushResponse, Record, Scope, open,
    opid, seal,
};

/// Where the server is, what it calls this client, and the key its envelopes
/// are sealed with. All three in settings, all three absent by default.
pub const SERVER: &str = "sync.server";
pub const TOKEN: &str = "sync.token";
pub const KEY: &str = "sync.key";
pub const CURSOR: &str = "sync.cursor";

/// How many operations to move in one request. The server's own ceiling is
/// higher; this keeps a first sync from building one enormous body.
const BATCH: u32 = 200;

pub struct Config {
    pub server: String,
    pub token: String,
    pub key: Key,
}

/// What a sync did, for a caller that wants to say so.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub pushed: usize,
    pub pulled: usize,
    pub applied: usize,
    /// Pulled operations that could not be opened, which is not an error —
    /// see `apply`.
    pub unreadable: usize,
}

/// Reads the configuration, or `None` when this machine syncs nowhere.
pub async fn configured(brain: &Brain) -> Result<Option<Config>> {
    let (Some(server), Some(token), Some(key)) = (
        brain.preference(SERVER).await?,
        brain.preference(TOKEN).await?,
        brain.preference(KEY).await?,
    ) else {
        return Ok(None);
    };
    let bytes = hex(&key).context("the sync key is not 32 bytes of hex")?;
    Ok(Some(Config {
        server: server.trim_end_matches('/').to_string(),
        token,
        key: Key::new(bytes),
    }))
}

fn hex(value: &str) -> Option<[u8; 32]> {
    let value = value.trim();
    if value.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in value.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

fn scope(scope: MemoryScope) -> Scope {
    match scope {
        MemoryScope::Project => Scope::Project,
        MemoryScope::Global => Scope::Global,
    }
}

fn localscope(scope: Scope) -> MemoryScope {
    match scope {
        Scope::Project => MemoryScope::Project,
        Scope::Global => MemoryScope::Global,
    }
}

/// The identity of a memory, as the log understands it.
pub fn identity(
    body: &str,
    source: &str,
    memoryscope: MemoryScope,
    project: &str,
    created: i64,
) -> (String, Record) {
    let record = Record::Put {
        body: body.to_string(),
        source: source.to_string(),
        scope: scope(memoryscope),
        project: project.to_string(),
        created,
    };
    (opid(&record), record)
}

// ---- push ------------------------------------------------------------------

/// Gives every memory an identity, for memories that predate sync.
///
/// Runs before a push rather than at write time, so turning sync on works on a
/// store that has been in use for months. Idempotent: a memory that already has
/// a row keeps it, including whether it has been sent.
pub async fn identify(brain: &Brain) -> Result<usize> {
    let rows = sqlx::query_as::<_, (i64, String, String, String, String, i64)>(
        "SELECT meta.memoryid, memory.body, memory.source, meta.scope, meta.project, memory.created \
         FROM memorymeta meta JOIN memory ON memory.rowid = meta.memoryid \
         WHERE meta.memoryid NOT IN (SELECT memoryid FROM memorysync)",
    )
    .fetch_all(brain.pool())
    .await
    .context("could not read memories to identify")?;

    let mut done = 0;
    for (id, body, source, scopename, project, created) in rows {
        let memoryscope = if scopename == "project" {
            MemoryScope::Project
        } else {
            MemoryScope::Global
        };
        let (id_hex, _) = identity(&body, &source, memoryscope, &project, created);
        // OR IGNORE: two memories with identical content in the same scope are
        // one identity, and the second is already covered by the first.
        let result = sqlx::query(
            "INSERT OR IGNORE INTO memorysync(memoryid, opid, pushed) VALUES (?1, ?2, 0)",
        )
        .bind(id)
        .bind(&id_hex)
        .execute(brain.pool())
        .await
        .context("could not record a memory's identity")?;
        done += result.rows_affected() as usize;
    }
    Ok(done)
}

/// Sends everything this machine has not sent.
pub async fn push(brain: &Brain, config: &Config) -> Result<usize> {
    identify(brain).await?;

    let mut sent = 0;
    loop {
        let rows = sqlx::query_as::<_, (i64, String, String, String, String, String, i64)>(
            "SELECT sync.memoryid, sync.opid, memory.body, memory.source, meta.scope, meta.project, memory.created \
             FROM memorysync sync \
             JOIN memorymeta meta ON meta.memoryid = sync.memoryid \
             JOIN memory ON memory.rowid = sync.memoryid \
             WHERE sync.pushed = 0 ORDER BY sync.memoryid LIMIT ?1",
        )
        .bind(BATCH as i64)
        .fetch_all(brain.pool())
        .await
        .context("could not read memories to send")?;
        if rows.is_empty() {
            break;
        }

        let mut ops = Vec::with_capacity(rows.len());
        let mut ids = Vec::with_capacity(rows.len());
        for (memoryid, opid_hex, body, source, scopename, project, created) in &rows {
            let memoryscope = if scopename == "project" {
                MemoryScope::Project
            } else {
                MemoryScope::Global
            };
            let (_, record) = identity(body, source, memoryscope, project, *created);
            let envelope =
                seal(&config.key, opid_hex, &record).context("could not seal a memory")?;
            ops.push(Op {
                opid: opid_hex.clone(),
                envelope,
            });
            ids.push(*memoryid);
        }

        let response: PushResponse = post(
            config,
            "/push",
            &PushRequest {
                protocol: PROTOCOL,
                ops,
            },
        )
        .await?;
        sent += response.accepted;

        // Marked after the server has it, never before. A push that fails is
        // sent again; a push marked early is a memory this machine believes it
        // has shared and never will.
        for id in ids {
            sqlx::query("UPDATE memorysync SET pushed = 1 WHERE memoryid = ?1")
                .bind(id)
                .execute(brain.pool())
                .await
                .context("could not mark a memory as sent")?;
        }
    }
    Ok(sent)
}

// ---- pull ------------------------------------------------------------------

/// Takes everything the log has that this machine has not seen.
pub async fn pull(brain: &Brain, config: &Config) -> Result<(usize, usize, usize)> {
    let mut cursor: i64 = brain
        .preference(CURSOR)
        .await?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    let (mut seen, mut applied, mut unreadable) = (0, 0, 0);
    loop {
        let response: PullResponse = post(
            config,
            "/pull",
            &PullRequest {
                protocol: PROTOCOL,
                since: cursor,
                limit: BATCH,
            },
        )
        .await?;
        if response.ops.is_empty() {
            cursor = cursor.max(response.head);
            break;
        }

        for numbered in &response.ops {
            seen += 1;
            match open(&config.key, &numbered.opid, &numbered.envelope) {
                Ok(record) => {
                    if apply(brain, &numbered.opid, &record).await? {
                        applied += 1;
                    }
                }
                // A record this key cannot open is not an error to stop on. It
                // is somebody else's log, or this machine's own from before a
                // key was rotated, and either way the cursor has to move past
                // it or every later sync stops here for good.
                Err(_) => unreadable += 1,
            }
            cursor = cursor.max(numbered.seq);
        }

        brain.setpreference(CURSOR, &cursor.to_string()).await?;
        if !response.more {
            cursor = cursor.max(response.head);
            break;
        }
    }

    brain.setpreference(CURSOR, &cursor.to_string()).await?;
    Ok((seen, applied, unreadable))
}

/// Puts one record into the local store. Returns whether anything changed.
async fn apply(brain: &Brain, opid_hex: &str, record: &Record) -> Result<bool> {
    match record {
        Record::Put {
            body,
            source,
            scope: recordscope,
            project,
            created,
        } => {
            // Already here, by identity rather than by text: the same memory
            // arriving from three machines is one memory.
            let existing =
                sqlx::query_scalar::<_, i64>("SELECT memoryid FROM memorysync WHERE opid = ?1")
                    .bind(opid_hex)
                    .fetch_optional(brain.pool())
                    .await?;
            if existing.is_some() {
                return Ok(false);
            }

            let stored = brain
                .storeforeign(body, source, localscope(*recordscope), project, *created)
                .await
                .context("could not store a memory from the log")?;

            // Marked pushed, because it came from the log: sending it back
            // would be accepted as a duplicate and cost a round trip to learn
            // what this row already knows.
            sqlx::query(
                "INSERT OR IGNORE INTO memorysync(memoryid, opid, pushed) VALUES (?1, ?2, 1)",
            )
            .bind(stored)
            .bind(opid_hex)
            .execute(brain.pool())
            .await
            .context("could not record an arriving memory's identity")?;
            Ok(true)
        }
        Record::Del { uid, .. } => {
            let Some(memoryid) =
                sqlx::query_scalar::<_, i64>("SELECT memoryid FROM memorysync WHERE opid = ?1")
                    .bind(uid)
                    .fetch_optional(brain.pool())
                    .await?
            else {
                // A deletion for something this machine never had. Not an
                // error: the log is shared and arrives in whatever order it
                // arrives in.
                return Ok(false);
            };
            brain
                .deletememory(memoryid)
                .await
                .context("could not remove a memory the log deleted")?;
            Ok(true)
        }
    }
}

/// Push then pull, which is the order that matters: a machine that pulls first
/// can be handed a deletion for a memory it has not yet shared, and the record
/// of it goes with it.
pub async fn once(brain: &Brain) -> Result<Option<Summary>> {
    let Some(config) = configured(brain).await? else {
        return Ok(None);
    };
    let pushed = push(brain, &config).await?;
    let (pulled, applied, unreadable) = pull(brain, &config).await?;
    Ok(Some(Summary {
        pushed,
        pulled,
        applied,
        unreadable,
    }))
}

async fn post<B: serde::Serialize, R: serde::de::DeserializeOwned>(
    config: &Config,
    path: &str,
    body: &B,
) -> Result<R> {
    let response = reqwest::Client::new()
        .post(format!("{}{path}", config.server))
        .bearer_auth(&config.token)
        .json(body)
        .send()
        .await
        .with_context(|| format!("could not reach the sync server at {}", config.server))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    anyhow::ensure!(
        status.is_success(),
        "the sync server answered {status}: {text}"
    );
    serde_json::from_str(&text).context("the sync server sent something this client cannot read")
}
