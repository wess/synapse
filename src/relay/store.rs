use crate::relay::{AgentView, ChannelView, Message, MessageKind, Registration, WorkerView};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// An agent heard from within this many seconds counts as reachable. A parked
/// `wait` refreshes its own timestamp while it blocks, so a live agent never
/// ages out and a dead one leaves the roster inside the window.
const LIVEWINDOW: i64 = 90;

/// Delivered history kept for the feed once every reader has consumed it.
const RETENTION: i64 = 10_000;

/// Absolute backlog bound. A lagging reader's unread messages are preserved past
/// [`RETENTION`], but never beyond this.
const MAXBACKLOG: i64 = 50_000;

#[derive(Clone)]
pub struct Mesh {
    pool: SqlitePool,
    _lock: Arc<File>,
}

impl Mesh {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let opened = crate::database::open(path.as_ref()).await?;
        Ok(Self {
            pool: opened.pool,
            _lock: opened.lock,
        })
    }

    /// Join the mesh under a name, keeping the read cursor of an agent that is
    /// re-registering so it does not lose its backlog.
    pub async fn register(&self, registration: &Registration) -> Result<()> {
        let name = validname(&registration.name)?;
        let tip = self.tip().await?;
        sqlx::query(
            "INSERT INTO meshagent(name, role, capabilities, project, tool, cursor, registered, seen) \
             VALUES (?, ?, ?, ?, ?, ?, 1, ?) \
             ON CONFLICT(name) DO UPDATE SET \
             role = excluded.role, capabilities = excluded.capabilities, \
             project = excluded.project, tool = excluded.tool, registered = 1, seen = excluded.seen",
        )
        .bind(&name)
        .bind(&registration.role)
        .bind(&registration.capabilities)
        .bind(&registration.project)
        .bind(&registration.tool)
        .bind(tip)
        .bind(now()?)
        .execute(&self.pool)
        .await
        .context("could not join the mesh")?;
        Ok(())
    }

    /// Create a placeholder row so a direct message addressed to an agent that
    /// has not registered yet is still delivered once it does. The cursor is
    /// seeded to the current tip, captured before the triggering message is
    /// written, and `register` preserves it on conflict.
    pub async fn placeholder(&self, name: &str) -> Result<()> {
        let tip = self.tip().await?;
        sqlx::query(
            "INSERT OR IGNORE INTO meshagent(name, cursor, registered, seen) VALUES (?, ?, 0, ?)",
        )
        .bind(name)
        .bind(tip)
        .bind(now()?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Prove this agent's process is alive and looping. Every tool call does
    /// this, as does a parked `wait` while it blocks.
    pub async fn touch(&self, name: &str) -> Result<()> {
        sqlx::query("UPDATE meshagent SET seen = ? WHERE name = ?")
            .bind(now()?)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn setstatus(&self, name: &str, status: &str) -> Result<()> {
        self.placeholder(name).await?;
        sqlx::query("UPDATE meshagent SET status = ?, seen = ? WHERE name = ?")
            .bind(status)
            .bind(now()?)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn statusof(&self, name: &str) -> Result<String> {
        Ok(
            sqlx::query_scalar::<_, String>("SELECT status FROM meshagent WHERE name = ?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or_default(),
        )
    }

    pub async fn insert(
        &self,
        sender: &str,
        kind: MessageKind,
        target: Option<&str>,
        body: &str,
    ) -> Result<i64> {
        anyhow::ensure!(!body.trim().is_empty(), "a message cannot be empty");
        let id = sqlx::query(
            "INSERT INTO meshmessage(sender, kind, target, body, created) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(sender)
        .bind(kind.value())
        .bind(target)
        .bind(body)
        .bind(now()?)
        .execute(&self.pool)
        .await
        .context("could not post to the mesh")?
        .last_insert_rowid();
        self.prune().await?;
        Ok(id)
    }

    /// Messages addressed to `name` beyond its read cursor. Reading the cursor
    /// and scanning happen in one statement, so a concurrent acknowledgement
    /// cannot interleave between them.
    pub async fn pending(&self, name: &str) -> Result<Vec<Message>> {
        sqlx::query_as::<_, Message>(
            "SELECT id, sender, kind, target, body, created FROM meshmessage \
             WHERE id > COALESCE((SELECT cursor FROM meshagent WHERE name = ?1), 0) \
             AND sender != ?1 AND (\
             (kind = 'direct' AND target = ?1) OR kind = 'broadcast' OR \
             (kind = 'channel' AND target IN (SELECT channel FROM meshsub WHERE agent = ?1))) \
             ORDER BY id ASC",
        )
        .bind(name)
        .fetch_all(&self.pool)
        .await
        .context("could not read the mesh inbox")
    }

    /// Confirm delivery up to `id`, advancing the read cursor monotonically so a
    /// stale acknowledgement can never rewind it and replay consumed history.
    pub async fn ack(&self, name: &str, id: i64) -> Result<()> {
        sqlx::query("UPDATE meshagent SET cursor = MAX(cursor, ?), seen = ? WHERE name = ?")
            .bind(id)
            .bind(now()?)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn subscribe(&self, agent: &str, channel: &str) -> Result<()> {
        let channel = validname(channel)?;
        sqlx::query("INSERT OR IGNORE INTO meshsub(agent, channel) VALUES (?, ?)")
            .bind(agent)
            .bind(&channel)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn unsubscribe(&self, agent: &str, channel: &str) -> Result<()> {
        sqlx::query("DELETE FROM meshsub WHERE agent = ? AND channel = ?")
            .bind(agent)
            .bind(channel)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn subscriptions(&self, agent: &str) -> Result<Vec<String>> {
        sqlx::query_scalar("SELECT channel FROM meshsub WHERE agent = ? ORDER BY channel")
            .bind(agent)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn channels(&self) -> Result<Vec<ChannelView>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT channel, COUNT(*) FROM meshsub GROUP BY channel ORDER BY channel",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(channel, subscribers)| ChannelView {
                channel,
                subscribers,
            })
            .collect())
    }

    pub async fn agents(&self) -> Result<Vec<AgentView>> {
        let horizon = now()? - LIVEWINDOW;
        let rows: Vec<(String, String, String, String, String, i64, i64, i64)> = sqlx::query_as(
            "SELECT a.name, a.role, a.status, a.project, a.tool, a.registered, \
             (SELECT COUNT(*) FROM meshsub s WHERE s.agent = a.name), a.seen \
             FROM meshagent a ORDER BY a.registered DESC, a.name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("could not read the mesh roster")?;
        Ok(rows
            .into_iter()
            .map(
                |(name, role, status, project, tool, registered, channels, seen)| AgentView {
                    name,
                    role,
                    status,
                    project,
                    tool,
                    registered: registered != 0,
                    online: seen > horizon,
                    channels,
                    seen,
                },
            )
            .collect())
    }

    /// Forget an agent that has left, so a session that ended stops appearing on
    /// the roster. Its subscriptions go with it; its messages stay in the feed.
    pub async fn forget(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM meshsub WHERE agent = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM meshagent WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Messages after `id` in ascending order; the most recent `limit` when `id`
    /// is zero or negative.
    pub async fn feed(&self, id: i64, limit: i64) -> Result<Vec<Message>> {
        if id <= 0 {
            return sqlx::query_as::<_, Message>(
                "SELECT id, sender, kind, target, body, created FROM meshmessage \
                 WHERE id > (SELECT COALESCE(MAX(id), 0) - ? FROM meshmessage) ORDER BY id ASC",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into);
        }
        sqlx::query_as::<_, Message>(
            "SELECT id, sender, kind, target, body, created FROM meshmessage \
             WHERE id > ? ORDER BY id ASC LIMIT ?",
        )
        .bind(id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn saveworker(&self, worker: &WorkerRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO meshworker(name, role, program, arguments, directory, keepalive, \
             session, supervisor, process, log, status, restarts, created) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, 'starting', 0, ?) \
             ON CONFLICT(name) DO UPDATE SET \
             role = excluded.role, program = excluded.program, arguments = excluded.arguments, \
             directory = excluded.directory, keepalive = excluded.keepalive, \
             session = excluded.session, supervisor = excluded.supervisor, log = excluded.log",
        )
        .bind(&worker.name)
        .bind(&worker.role)
        .bind(&worker.program)
        .bind(serde_json::to_string(&worker.arguments)?)
        .bind(&worker.directory)
        .bind(worker.keepalive as i64)
        .bind(&worker.session)
        .bind(worker.supervisor as i64)
        .bind(&worker.log)
        .bind(now()?)
        .execute(&self.pool)
        .await
        .context("could not record the worker")?;
        Ok(())
    }

    pub async fn workerstate(
        &self,
        name: &str,
        status: &str,
        process: u32,
        restarts: i64,
    ) -> Result<()> {
        sqlx::query("UPDATE meshworker SET status = ?, process = ?, restarts = ? WHERE name = ?")
            .bind(status)
            .bind(process as i64)
            .bind(restarts)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn deleteworker(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM meshworker WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn workers(&self) -> Result<Vec<WorkerView>> {
        let rows: Vec<(String, String, String, i64, String, String, i64, i64, i64)> =
            sqlx::query_as(
                "SELECT name, role, status, process, directory, log, restarts, keepalive, \
                 supervisor FROM meshworker ORDER BY name",
            )
            .fetch_all(&self.pool)
            .await
            .context("could not read the worker list")?;
        Ok(rows
            .into_iter()
            .map(
                |(name, role, status, process, directory, log, restarts, keepalive, supervisor)| {
                    WorkerView {
                        name,
                        role,
                        status,
                        process: process as u32,
                        restarts,
                        keepalive: keepalive != 0,
                        directory,
                        log,
                        supervisor: supervisor as u32,
                    }
                },
            )
            .collect())
    }

    pub async fn worker(&self, name: &str) -> Result<Option<WorkerView>> {
        Ok(self
            .workers()
            .await?
            .into_iter()
            .find(|worker| worker.name == name))
    }

    async fn tip(&self) -> Result<i64> {
        sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM meshmessage")
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// Trim delivered history. Nothing above the slowest reader's cursor is
    /// touched, so unread messages survive past the feed retention, except that
    /// [`MAXBACKLOG`] is a hard bound: when it forces the floor past a cursor,
    /// that cursor moves up with it so the next drain is consistent.
    async fn prune(&self) -> Result<()> {
        let tip = self.tip().await?;
        if tip <= RETENTION {
            return Ok(());
        }
        let slowest: Option<i64> =
            sqlx::query_scalar("SELECT MIN(cursor) FROM meshagent WHERE registered = 1")
                .fetch_one(&self.pool)
                .await?;
        let floor = prunefloor(tip, slowest);
        if floor <= 0 {
            return Ok(());
        }
        sqlx::query("UPDATE meshagent SET cursor = ? WHERE cursor < ?")
            .bind(floor)
            .bind(floor)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM meshmessage WHERE id <= ?")
            .bind(floor)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// A background worker as persisted, so the roster survives the session that
/// started it and `synapse relay ps` can report on it.
pub struct WorkerRecord {
    pub name: String,
    pub role: String,
    pub program: String,
    pub arguments: Vec<String>,
    pub directory: String,
    pub keepalive: bool,
    pub session: Option<String>,
    pub supervisor: u32,
    pub log: String,
}

/// Highest message id safe to delete: everything the slowest reader has
/// consumed, keeping at least [`RETENTION`] rows for the feed, but never letting
/// the backlog exceed [`MAXBACKLOG`].
fn prunefloor(tip: i64, slowest: Option<i64>) -> i64 {
    let retention = tip - RETENTION;
    let soft = slowest.map_or(retention, |cursor| retention.min(cursor));
    soft.max(tip - MAXBACKLOG)
}

/// Agent and channel names address rows and appear in prompts, so they stay
/// short, printable, and free of the whitespace that would break a roster line.
fn validname(value: &str) -> Result<String> {
    let value = value.trim();
    anyhow::ensure!(!value.is_empty(), "a name cannot be empty");
    anyhow::ensure!(
        value.len() <= 64,
        "a name cannot be longer than 64 characters"
    );
    anyhow::ensure!(
        value
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '-' | '.' | '_')),
        "a name may only contain letters, digits, dashes, dots, and underscores"
    );
    Ok(value.to_owned())
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

    async fn mesh() -> (Mesh, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let mesh = Mesh::open(directory.path().join("brain.db")).await.unwrap();
        (mesh, directory)
    }

    fn registration(name: &str) -> Registration {
        Registration {
            name: name.to_owned(),
            role: "worker".to_owned(),
            ..Registration::default()
        }
    }

    #[tokio::test]
    async fn a_direct_message_sent_before_registration_still_arrives() {
        let (mesh, _directory) = mesh().await;
        mesh.placeholder("backend").await.unwrap();
        mesh.insert(
            "lead",
            MessageKind::Direct,
            Some("backend"),
            "build the api",
        )
        .await
        .unwrap();

        mesh.register(&registration("backend")).await.unwrap();

        let pending = mesh.pending("backend").await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].body, "build the api");
    }

    #[tokio::test]
    async fn a_newcomer_does_not_replay_earlier_broadcasts() {
        let (mesh, _directory) = mesh().await;
        mesh.insert("lead", MessageKind::Broadcast, None, "standup")
            .await
            .unwrap();

        mesh.register(&registration("late")).await.unwrap();

        assert!(mesh.pending("late").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_unacknowledged_drain_is_redelivered_until_it_is_acknowledged() {
        let (mesh, _directory) = mesh().await;
        mesh.register(&registration("backend")).await.unwrap();
        mesh.insert("lead", MessageKind::Direct, Some("backend"), "ship it")
            .await
            .unwrap();

        let first = mesh.pending("backend").await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(mesh.pending("backend").await.unwrap().len(), 1);

        mesh.ack("backend", first[0].id).await.unwrap();
        assert!(mesh.pending("backend").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_stale_acknowledgement_never_replays_consumed_history() {
        let (mesh, _directory) = mesh().await;
        mesh.register(&registration("backend")).await.unwrap();
        mesh.insert("lead", MessageKind::Direct, Some("backend"), "one")
            .await
            .unwrap();
        let second = mesh
            .insert("lead", MessageKind::Direct, Some("backend"), "two")
            .await
            .unwrap();

        mesh.ack("backend", second).await.unwrap();
        mesh.ack("backend", 1).await.unwrap();

        assert!(mesh.pending("backend").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn channel_posts_only_reach_subscribers() {
        let (mesh, _directory) = mesh().await;
        mesh.register(&registration("backend")).await.unwrap();
        mesh.register(&registration("frontend")).await.unwrap();
        mesh.subscribe("backend", "devops").await.unwrap();
        mesh.insert("lead", MessageKind::Channel, Some("devops"), "deploy")
            .await
            .unwrap();

        assert_eq!(mesh.pending("backend").await.unwrap().len(), 1);
        assert!(mesh.pending("frontend").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn re_registering_keeps_the_read_cursor() {
        let (mesh, _directory) = mesh().await;
        mesh.register(&registration("backend")).await.unwrap();
        let id = mesh
            .insert("lead", MessageKind::Direct, Some("backend"), "queued")
            .await
            .unwrap();

        mesh.register(&registration("backend")).await.unwrap();

        let pending = mesh.pending("backend").await.unwrap();
        assert_eq!(pending.len(), 1, "a reconnect must not skip unread work");
        assert_eq!(pending[0].id, id);
    }

    #[tokio::test]
    async fn leaving_the_mesh_clears_the_roster_row_and_its_channels() {
        let (mesh, _directory) = mesh().await;
        mesh.register(&registration("backend")).await.unwrap();
        mesh.subscribe("backend", "devops").await.unwrap();

        mesh.forget("backend").await.unwrap();

        assert!(mesh.agents().await.unwrap().is_empty());
        assert!(mesh.channels().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_sender_never_receives_its_own_broadcast() {
        let (mesh, _directory) = mesh().await;
        mesh.register(&registration("lead")).await.unwrap();
        mesh.insert("lead", MessageKind::Broadcast, None, "standup")
            .await
            .unwrap();

        assert!(mesh.pending("lead").await.unwrap().is_empty());
    }

    #[test]
    fn the_prune_floor_respects_cursors_up_to_the_hard_cap() {
        assert_eq!(prunefloor(20_000, Some(20_000)), 20_000 - RETENTION);
        assert_eq!(prunefloor(20_000, None), 20_000 - RETENTION);
        assert_eq!(prunefloor(20_000, Some(5)), 5);
        assert_eq!(prunefloor(100_000, Some(5)), 100_000 - MAXBACKLOG);
    }

    #[test]
    fn names_that_would_break_a_roster_line_are_refused() {
        assert!(validname("frontend").is_ok());
        assert!(validname("build-2.a_b").is_ok());
        assert!(validname("  ").is_err());
        assert!(validname("two words").is_err());
        assert!(validname(&"x".repeat(65)).is_err());
    }
}
