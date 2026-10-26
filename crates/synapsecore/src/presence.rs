//! Transport presence is independent of opting into agent coordination.
mod tests;

use anyhow::Result;
use serde::Serialize;
use sqlx::{Connection, FromRow, SqlitePool};
use std::time::{SystemTime, UNIX_EPOCH};

pub const LIVESECONDS: i64 = 45;

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct Session {
    pub id: String,
    pub tool: String,
    pub project: String,
    pub name: String,
    pub status: String,
    pub note: String,
    pub slot: i64,
    pub started: i64,
    pub seen: i64,
    pub ended: i64,
    pub appearance: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Connected,
    Working,
    Waiting,
    Blocked,
    Done,
    Disconnected,
}

impl Session {
    pub fn online(&self, now: i64) -> bool {
        self.ended == 0 && self.seen > now - LIVESECONDS
    }

    pub fn state(&self, now: i64) -> State {
        if !self.online(now) {
            return State::Disconnected;
        }
        match self.status.to_lowercase().as_str() {
            "working" | "busy" | "running" => State::Working,
            "waiting" | "idle" => State::Waiting,
            "blocked" | "error" => State::Blocked,
            "done" | "complete" | "completed" => State::Done,
            _ => State::Connected,
        }
    }

    pub fn identity(&self) -> String {
        identity(&self.tool, &self.project, self.slot)
    }
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            &self.tool
        } else {
            &self.name
        }
    }
}

fn identity(tool: &str, project: &str, slot: i64) -> String {
    serde_json::to_string(&(tool, project, slot)).unwrap_or_default()
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub async fn join(pool: &SqlitePool, id: &str, tool: &str, project: &str) -> Result<()> {
    let time = now();
    // Reserve the lowest free room atomically across simultaneous MCP processes.
    let mut connection = pool.acquire().await?;
    let mut transaction = connection.begin_with("BEGIN IMMEDIATE").await?;
    let slots: Vec<i64> = sqlx::query_scalar(
        "SELECT slot FROM presence WHERE tool = ? AND project = ? AND ended = 0 AND seen > ?",
    )
    .bind(tool)
    .bind(project)
    .bind(time - LIVESECONDS)
    .fetch_all(&mut *transaction)
    .await?;
    let slot = (0..).find(|slot| !slots.contains(slot)).unwrap_or(0);
    sqlx::query(
        "INSERT OR IGNORE INTO presence(id,tool,project,slot,started,seen) VALUES(?,?,?,?,?,?)",
    )
    .bind(id)
    .bind(tool)
    .bind(project)
    .bind(slot)
    .bind(time)
    .bind(time)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM presence WHERE seen < ?")
        .bind(time - 86400)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

/// Harnesses that start MCP outside the checkout can identify it on their first scoped call.
pub async fn relocate(pool: &SqlitePool, id: &str, project: &str) -> Result<()> {
    let previous: Option<String> =
        sqlx::query_scalar("SELECT project FROM presence WHERE id = ? AND ended = 0")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    if previous.as_deref() == Some(project) || previous.is_none() {
        return Ok(());
    }
    let mut connection = pool.acquire().await?;
    let mut transaction = connection.begin_with("BEGIN IMMEDIATE").await?;
    let current: Option<(String, String)> =
        sqlx::query_as("SELECT tool,project FROM presence WHERE id = ? AND ended = 0")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
    if let Some((tool, old)) = current
        && old != project
    {
        let slots:Vec<i64>=sqlx::query_scalar("SELECT slot FROM presence WHERE tool = ? AND project = ? AND ended = 0 AND seen > ? AND id != ?")
                .bind(&tool).bind(project).bind(now()-LIVESECONDS).bind(id).fetch_all(&mut *transaction).await?;
        let slot = (0..).find(|slot| !slots.contains(slot)).unwrap_or(0);
        sqlx::query("UPDATE presence SET project = ?, slot = ? WHERE id = ?")
            .bind(project)
            .bind(slot)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn touch(pool: &SqlitePool, id: &str) -> Result<()> {
    let time = now();
    let mut connection = pool.acquire().await?;
    let mut transaction = connection.begin_with("BEGIN IMMEDIATE").await?;
    let current: Option<(String, String, i64)> =
        sqlx::query_as("SELECT tool,project,slot FROM presence WHERE id = ? AND ended = 0")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
    if let Some((tool, project, slot)) = current {
        let used:Vec<i64>=sqlx::query_scalar("SELECT slot FROM presence WHERE tool = ? AND project = ? AND ended = 0 AND seen > ? AND id != ?")
            .bind(tool).bind(project).bind(time-LIVESECONDS).bind(id).fetch_all(&mut *transaction).await?;
        // A suspended transport can return after another session reclaimed its room.
        let slot = if used.contains(&slot) {
            (0..).find(|slot| !used.contains(slot)).unwrap_or(0)
        } else {
            slot
        };
        sqlx::query("UPDATE presence SET seen = ?, slot = ? WHERE id = ? AND ended = 0")
            .bind(time)
            .bind(slot)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn report(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    status: &str,
    note: &str,
) -> Result<()> {
    touch(pool, id).await?;
    sqlx::query(
        "UPDATE presence SET name = ?, status = ?, note = ?, seen = ? WHERE id = ? AND ended = 0",
    )
    .bind(name)
    .bind(status)
    .bind(note)
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn leave(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE presence SET ended = ? WHERE id = ?")
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn sessions(pool: &SqlitePool) -> Result<Vec<Session>> {
    let mut sessions: Vec<Session> = sqlx::query_as("SELECT *, 0 AS appearance FROM (SELECT rowid AS ordinal, * FROM presence WHERE seen > ? ORDER BY (ended = 0 AND seen > ?) DESC, seen DESC, rowid DESC LIMIT 256) ORDER BY tool,project,slot,(ended = 0 AND seen > ?) DESC,started DESC,ordinal DESC")
        .bind(now() - 86400).bind(now() - LIVESECONDS).bind(now() - LIVESECONDS).fetch_all(pool).await?;
    // A reconnect reuses its room. A still-live previous transport keeps its own.
    let mut identities = std::collections::HashSet::new();
    sessions.retain(|session| identities.insert(session.identity()));
    let avatars: Vec<(String, i64)> =
        sqlx::query_as("SELECT identity, appearance FROM sceneavatar")
            .fetch_all(pool)
            .await?;
    for session in &mut sessions {
        session.appearance = avatars
            .iter()
            .find(|(id, _)| *id == session.identity())
            .map(|(_, n)| *n)
            .unwrap_or_else(|| {
                session.identity().bytes().fold(0u64, |hash, byte| {
                    hash.wrapping_mul(31).wrapping_add(byte as u64)
                }) as i64
                    & 31
            });
    }
    Ok(sessions)
}

pub async fn appearance(pool: &SqlitePool, identity: &str, value: i64) -> Result<()> {
    anyhow::ensure!(
        (0..32).contains(&value),
        "appearance must be between 0 and 31"
    );
    sqlx::query("INSERT INTO sceneavatar(identity,appearance) VALUES(?,?) ON CONFLICT(identity) DO UPDATE SET appearance=excluded.appearance")
        .bind(identity).bind(value).execute(pool).await?;
    Ok(())
}
