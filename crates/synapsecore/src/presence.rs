//! transport presence is independent of opting into agent coordination.
//!
//! A session is live while the process that joined is. Nothing here keeps a
//! clock: a heartbeat is a write from every connected session for as long as it
//! is open, to answer a question the operating system already answers for free.
mod tests;

use crate::backend::{EventDraft, Level};
use anyhow::Result;
use serde::Serialize;
use sqlx::{Connection, FromRow, QueryBuilder, Sqlite, SqliteConnection, SqlitePool};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// How long an ended session remains available for recent-activity views.
const RETENTION: i64 = 86400;

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
    pub fn online(&self) -> bool {
        self.ended == 0
    }

    pub fn state(&self) -> State {
        if !self.online() {
            return State::Disconnected;
        }
        if ["working", "busy", "running"]
            .iter()
            .any(|value| self.status.eq_ignore_ascii_case(value))
        {
            State::Working
        } else if ["waiting", "idle"]
            .iter()
            .any(|value| self.status.eq_ignore_ascii_case(value))
        {
            State::Waiting
        } else if ["blocked", "error"]
            .iter()
            .any(|value| self.status.eq_ignore_ascii_case(value))
        {
            State::Blocked
        } else if ["done", "complete", "completed"]
            .iter()
            .any(|value| self.status.eq_ignore_ascii_case(value))
        {
            State::Done
        } else {
            State::Connected
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

/// A session id for this process. The process id leads it, which is what lets
/// anybody reading the row ask whether its owner is still running.
pub fn identify() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

/// Whether the process that joined as `id` is still running. An id that names
/// no process cannot be asked, and a session nobody can ask about is not one
/// to draw as connected.
fn alive(id: &str) -> bool {
    id.split_once('-')
        .and_then(|(process, _)| process.parse().ok())
        .is_some_and(crate::relay::process::alive)
}

/// End every session whose process is gone. A session killed rather than closed
/// never says goodbye, so whoever looks next says it for it.
async fn retire(connection: &mut SqliteConnection, time: i64) -> Result<()> {
    let open: Vec<String> = sqlx::query_scalar("SELECT id FROM presence WHERE ended = 0")
        .fetch_all(&mut *connection)
        .await?;
    let dead: Vec<String> = open.into_iter().filter(|id| !alive(id)).collect();
    if !dead.is_empty() {
        let mut query = QueryBuilder::<Sqlite>::new("UPDATE presence SET ended = ");
        query.push_bind(time).push(" WHERE ended = 0 AND id IN (");
        let mut ids = query.separated(", ");
        for id in dead {
            ids.push_bind(id);
        }
        ids.push_unseparated(")");
        query.build().execute(&mut *connection).await?;
    }
    Ok(())
}

pub async fn join(pool: &SqlitePool, id: &str, tool: &str, project: &str) -> Result<()> {
    let time = now();
    // reserve the lowest free room atomically across simultaneous MCP processes
    let mut connection = pool.acquire().await?;
    let mut transaction = connection.begin_with("BEGIN IMMEDIATE").await?;
    retire(&mut transaction, time).await?;
    let slots: HashSet<i64> = sqlx::query_scalar(
        "SELECT slot FROM presence WHERE tool = ? AND project = ? AND ended = 0",
    )
    .bind(tool)
    .bind(project)
    .fetch_all(&mut *transaction)
    .await?
    .into_iter()
    .collect();
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
    // Only what has ended: a session open and quiet for a week is still open.
    sqlx::query("DELETE FROM presence WHERE ended > 0 AND ended < ?")
        .bind(time - RETENTION)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    let _ = crate::backend::recordpool(
        pool,
        EventDraft {
            source: "harness".into(),
            project: project.into(),
            agent: tool.into(),
            session: id.into(),
            kind: "harness.join".into(),
            message: format!("{tool} connected"),
            fields: serde_json::json!({ "slot": slot }),
            ..Default::default()
        },
    )
    .await;
    Ok(())
}

/// harnesses that start MCP outside the checkout can identify it on their first scoped call
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
        retire(&mut transaction, now()).await?;
        let slots: HashSet<i64> = sqlx::query_scalar(
            "SELECT slot FROM presence WHERE tool = ? AND project = ? AND ended = 0 AND id != ?",
        )
        .bind(&tool)
        .bind(project)
        .bind(id)
        .fetch_all(&mut *transaction)
        .await?
        .into_iter()
        .collect();
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

pub async fn report(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    status: &str,
    note: &str,
) -> Result<()> {
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
    if let Ok(Some((tool, project))) = sqlx::query_as::<_, (String, String)>(
        "SELECT tool,project FROM presence WHERE id = ? AND ended = 0",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    {
        let level = if matches!(status, "failed" | "blocked" | "error" | "crashed") {
            Level::Error
        } else {
            Level::Info
        };
        let _ = crate::backend::recordpool(
            pool,
            EventDraft {
                source: "harness".into(),
                project,
                agent: if name.is_empty() { tool } else { name.into() },
                session: id.into(),
                level,
                kind: "harness.status".into(),
                message: format!(
                    "{} is {status}",
                    if name.is_empty() { "harness" } else { name }
                ),
                fields: serde_json::json!({ "status": status, "note": note }),
                ..Default::default()
            },
        )
        .await;
    }
    Ok(())
}

pub async fn leave(pool: &SqlitePool, id: &str) -> Result<()> {
    let session = sqlx::query_as::<_, (String, String, String)>(
        "SELECT tool,project,name FROM presence WHERE id = ? AND ended = 0",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    sqlx::query("UPDATE presence SET ended = ? WHERE id = ?")
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;
    if let Some((tool, project, name)) = session {
        let agent = if name.is_empty() { tool.clone() } else { name };
        let _ = crate::backend::recordpool(
            pool,
            EventDraft {
                source: "harness".into(),
                project,
                agent,
                session: id.into(),
                kind: "harness.leave".into(),
                message: format!("{tool} disconnected"),
                ..Default::default()
            },
        )
        .await;
    }
    Ok(())
}

pub async fn sessions(pool: &SqlitePool) -> Result<Vec<Session>> {
    let time = now();
    // One liveness pass per read. Rendering consumes this snapshot and never
    // reaches into the process table itself.
    if let Ok(mut connection) = pool.acquire().await {
        let _ = retire(&mut connection, time).await;
    }
    let mut sessions: Vec<Session> = sqlx::query_as(
        "SELECT * FROM (SELECT rowid AS ordinal, * FROM presence WHERE ended = 0 OR ended > ? ORDER BY (ended = 0) DESC, seen DESC, rowid DESC LIMIT 256) ORDER BY tool,project,slot,(ended = 0) DESC,started DESC,ordinal DESC",
    )
    .bind(time - RETENTION)
    .fetch_all(pool)
    .await?;
    // a reconnect reuses its room; a still-live previous transport keeps its own
    let mut identities = std::collections::HashSet::new();
    sessions.retain(|session| identities.insert(session.identity()));
    Ok(sessions)
}
