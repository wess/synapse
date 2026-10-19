use crate::brain::{Brain, Memory};
use anyhow::{Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

/// Clients whose SessionStart hook already counts the session. The server
/// skips them so nothing is counted twice; health checks also start servers
/// that never reach a model.
const HOOKED: &[&str] = &["claude-code", "pi"];

/// Keep the ledger bounded while retaining enough history for long reports.
pub const RETENTIONDAYS: i64 = 730;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Session start, or the restart after a compaction.
    Session,
    Recall,
    /// A `readmemory` page.
    Read,
}

impl Kind {
    fn value(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Recall => "recall",
            Self::Read => "read",
        }
    }
}

/// Sizes are in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    pub memories: usize,
    /// All memory the session could see. Sessions only.
    pub reach: usize,
    /// Full size of the matches before the recall budget trimmed them.
    pub matched: usize,
    pub sent: usize,
    /// Tool definitions and guidance per session, plus response framing.
    pub overhead: usize,
}

impl Entry {
    pub fn session(reach: usize, overhead: usize) -> Self {
        Self {
            kind: Kind::Session,
            memories: 0,
            reach,
            matched: 0,
            sent: 0,
            overhead,
        }
    }

    pub fn recall(memories: usize, sent: usize, matched: usize, response: usize) -> Self {
        Self {
            kind: Kind::Recall,
            memories,
            reach: 0,
            matched,
            sent,
            overhead: response.saturating_sub(sent),
        }
    }

    pub fn read(page: usize, response: usize) -> Self {
        Self {
            kind: Kind::Read,
            memories: 0,
            reach: 0,
            matched: 0,
            sent: page,
            overhead: response.saturating_sub(page),
        }
    }
}

pub fn bodies(memories: &[Memory]) -> usize {
    memories
        .iter()
        .map(|memory| memory.body.chars().count())
        .sum()
}

pub fn hooked(client: &str) -> bool {
    HOOKED.contains(&client)
}

/// Synapse's fixed cost per session: tool definitions plus its own guidance.
pub fn standing(guidance: &str, mesh: bool, learn: bool) -> usize {
    let tools: usize = crate::mcp::toolcost(mesh, learn)
        .iter()
        .map(|(_, size)| size)
        .sum();
    tools + crate::instructions::added(guidance, mesh, learn)
}

pub async fn record(brain: &Brain, entry: Entry) -> Result<()> {
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default();
    let mut transaction = brain.pool().begin().await?;
    sqlx::query(
        "INSERT INTO usage(kind, memories, reach, matched, sent, overhead, created) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.kind.value())
    .bind(entry.memories as i64)
    .bind(entry.reach as i64)
    .bind(entry.matched as i64)
    .bind(entry.sent as i64)
    .bind(entry.overhead as i64)
    .bind(created)
    .execute(&mut *transaction)
    .await
    .context("could not record usage")?;
    sqlx::query("DELETE FROM usage WHERE created < ?")
        .bind(created - RETENTIONDAYS * 86_400)
        .execute(&mut *transaction)
        .await
        .context("could not prune old usage")?;
    transaction.commit().await?;
    Ok(())
}
