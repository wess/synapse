//! Delivery across the shared database.
//!
//! Every connected tool runs its own `synapse mcp` process, so the mesh has no
//! single process to hold a wake signal in. The bus is the database instead: a
//! parked `wait` re-reads its inbox on a short tick, which costs one indexed
//! query per agent per tick and needs no daemon, no port, and no token.
//!
//! Delivery is at-least-once. [`awaitmessages`] only *reads* pending messages;
//! the read cursor advances through [`ack`] once the reply carrying them has
//! been built. A drain whose caller died before that is simply re-read by the
//! next `wait`, so duplicates are possible on that rare path and loss needs a
//! process to die inside the moment between building a reply and writing it.

use crate::relay::{Mesh, Message, MessageKind};
use anyhow::Result;
use std::time::Duration;
use tokio::time::Instant;

/// How long one `wait` parks before returning an empty list. The agent's
/// protocol is to call `wait` again.
///
/// This must stay under the shortest tool-call timeout any connected client
/// applies, because a call the client abandons reaches the agent as an *error*
/// rather than an empty result, and an agent that sees an error stops looping.
/// Returning empty is cheap: one tool call per agent per four idle minutes.
pub const PARKSECONDS: u64 = 240;

/// The shortest tool-call timeout a supported client applies. [`PARKSECONDS`]
/// must stay below it.
pub const CLIENTIDLEFLOOR: u64 = 300;

/// How often a parked `wait` reports progress to a client that asked for it.
/// Protocol traffic, unlike transport keepalives, is what stops a client from
/// aging the call out.
pub const PROGRESSSECONDS: u64 = 30;

const _: () = assert!(
    PARKSECONDS < CLIENTIDLEFLOOR,
    "the park deadline must stay under the shortest client tool-call timeout"
);
const _: () = assert!(
    PARKSECONDS / PROGRESSSECONDS >= 4,
    "a park should report progress several times, not once"
);

/// How often a parked agent re-reads its inbox.
const TICK: Duration = Duration::from_millis(750);

/// How often a parked agent refreshes its liveness stamp. Far below the roster's
/// window, so a live agent never ages out, and far above the poll tick, so a
/// quiet mesh is not a stream of writes.
const HEARTBEAT: Duration = Duration::from_secs(15);

/// Store a message and make sure its recipient can receive it.
///
/// A direct message may be addressed to an agent that has not registered yet — a
/// supervisor assigns work the instant it spawns a worker, racing that worker's
/// `register`. The placeholder row is created *before* the message is written so
/// its seeded read cursor sits just below this message's id; otherwise the new
/// agent, whose cursor starts at the tip, would never see the task.
pub async fn deliver(
    mesh: &Mesh,
    from: &str,
    kind: MessageKind,
    target: Option<&str>,
    body: &str,
) -> Result<i64> {
    if let (MessageKind::Direct, Some(recipient)) = (kind, target) {
        mesh.placeholder(recipient).await?;
    }
    mesh.insert(from, kind, target, body).await
}

/// Park until messages addressed to `name` arrive, then return them *without*
/// advancing the read cursor — the caller acknowledges with [`ack`] once the
/// reply has actually reached the client. Returns empty on timeout, and
/// immediately when `block` is false and the inbox is empty.
pub async fn awaitmessages(
    mesh: &Mesh,
    name: &str,
    block: bool,
    maxwait: Duration,
) -> Result<Vec<Message>> {
    let deadline = Instant::now() + maxwait;
    let mut beat = Instant::now();
    loop {
        let pending = mesh.pending(name).await?;
        if !pending.is_empty() {
            return Ok(pending);
        }
        if !block {
            return Ok(Vec::new());
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(Vec::new());
        }
        // A parked agent is still a live agent. Refreshing while it blocks is
        // what keeps it on the roster, and lets a crashed one drop off.
        if now.duration_since(beat) >= HEARTBEAT {
            mesh.touch(name).await?;
            beat = now;
        }
        tokio::time::sleep(TICK.min(deadline - now)).await;
    }
}

/// Acknowledge delivery up to `id`. Call this only once the reply has reached
/// the client; an unacknowledged drain is redelivered by the next `wait`.
pub async fn ack(mesh: &Mesh, name: &str, id: i64) -> Result<()> {
    mesh.ack(name, id).await
}

/// Record an agent's semantic work state.
pub async fn reportstatus(mesh: &Mesh, name: &str, status: &str) -> Result<()> {
    let status = status.trim();
    anyhow::ensure!(!status.is_empty(), "a status cannot be empty");
    anyhow::ensure!(
        status.len() <= 64,
        "a status cannot be longer than 64 characters"
    );
    mesh.setstatus(name, status).await
}

/// Park until `target` reports one of `want`, then return the matching status.
/// Returns the current status immediately when it already matches, and the
/// current status — matching or not — on timeout. An empty `want` matches any
/// reported state.
pub async fn awaitstatus(
    mesh: &Mesh,
    target: &str,
    want: &[String],
    block: bool,
    maxwait: Duration,
) -> Result<String> {
    let deadline = Instant::now() + maxwait;
    loop {
        let status = mesh.statusof(target).await?;
        if matches(&status, want) {
            return Ok(status);
        }
        if !block {
            return Ok(status);
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(status);
        }
        tokio::time::sleep(TICK.min(deadline - now)).await;
    }
}

/// Whether `status` satisfies `want`, case-insensitively. An empty `want` means
/// any reported state.
fn matches(status: &str, want: &[String]) -> bool {
    if status.is_empty() {
        return false;
    }
    want.is_empty() || want.iter().any(|item| item.eq_ignore_ascii_case(status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::Registration;

    async fn mesh() -> (Mesh, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let mesh = Mesh::open(directory.path().join("brain.db")).await.unwrap();
        (mesh, directory)
    }

    async fn join(mesh: &Mesh, name: &str) {
        mesh.register(&Registration {
            name: name.to_owned(),
            role: "worker".to_owned(),
            ..Registration::default()
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_parked_wait_returns_as_soon_as_a_message_lands() {
        let (mesh, _directory) = mesh().await;
        join(&mesh, "backend").await;
        let writer = mesh.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            deliver(&writer, "lead", MessageKind::Direct, Some("backend"), "go")
                .await
                .unwrap();
        });

        let messages = awaitmessages(&mesh, "backend", true, Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].body, "go");
    }

    #[tokio::test]
    async fn an_idle_park_ends_empty_rather_than_failing() {
        let (mesh, _directory) = mesh().await;
        join(&mesh, "backend").await;

        let messages = awaitmessages(&mesh, "backend", true, Duration::from_millis(200))
            .await
            .unwrap();

        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn a_park_never_drains_a_message_the_agent_sent_itself() {
        let (mesh, _directory) = mesh().await;
        join(&mesh, "lead").await;
        deliver(&mesh, "lead", MessageKind::Broadcast, None, "standup")
            .await
            .unwrap();

        let messages = awaitmessages(&mesh, "lead", false, Duration::from_millis(10))
            .await
            .unwrap();

        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn waiting_on_a_status_wakes_when_the_target_reports_it() {
        let (mesh, _directory) = mesh().await;
        join(&mesh, "builder").await;
        let writer = mesh.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            reportstatus(&writer, "builder", "done").await.unwrap();
        });

        let status = awaitstatus(
            &mesh,
            "builder",
            &["done".to_owned()],
            true,
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert_eq!(status, "done");
    }

    #[tokio::test]
    async fn a_status_wait_reports_the_current_state_when_it_does_not_match() {
        let (mesh, _directory) = mesh().await;
        join(&mesh, "builder").await;
        reportstatus(&mesh, "builder", "working").await.unwrap();

        let status = awaitstatus(
            &mesh,
            "builder",
            &["done".to_owned()],
            false,
            Duration::from_millis(10),
        )
        .await
        .unwrap();

        assert_eq!(status, "working");
    }

    #[test]
    fn an_empty_wanted_set_matches_any_reported_state() {
        assert!(matches("working", &[]));
        assert!(!matches("", &[]));
        assert!(matches("Done", &["done".to_owned()]));
        assert!(!matches("working", &["done".to_owned()]));
    }
}
