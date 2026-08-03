use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::str::FromStr;

/// One message as stored and delivered.
#[derive(Clone, Debug, FromRow, Serialize, JsonSchema, PartialEq, Eq)]
pub struct Message {
    pub id: i64,
    pub sender: String,
    pub kind: MessageKind,
    /// Agent name for a direct message, channel name for a channel post, absent
    /// for a broadcast.
    pub target: Option<String>,
    pub body: String,
    pub created: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum MessageKind {
    Direct,
    Channel,
    Broadcast,
}

impl MessageKind {
    pub fn value(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Channel => "channel",
            Self::Broadcast => "broadcast",
        }
    }
}

impl FromStr for MessageKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "direct" => Ok(Self::Direct),
            "channel" => Ok(Self::Channel),
            "broadcast" => Ok(Self::Broadcast),
            _ => anyhow::bail!("expected direct, channel, or broadcast"),
        }
    }
}

/// What `register` records about an agent joining the mesh.
#[derive(Clone, Debug, Default)]
pub struct Registration {
    pub name: String,
    pub role: String,
    pub capabilities: String,
    /// Absolute project root the agent is working in, so the roster shows which
    /// checkout each one is standing in.
    pub project: String,
    /// The connected tool, when it identified itself (`Claude Code`, `Codex`).
    pub tool: String,
    /// Whether this is a person at a keyboard rather than an agent. Agents
    /// address a human for decisions they cannot make alone, and never delegate
    /// work to one.
    pub human: bool,
}

/// A roster row, as returned to agents and the dashboard.
#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct AgentView {
    pub name: String,
    pub role: String,
    /// Last self-reported semantic work state, empty when never reported.
    pub status: String,
    /// What the agent said it was doing when it last reported, empty when it
    /// said only which state it was in. A state tells you a worker has not
    /// stalled; this is the part that tells you whether to leave it alone.
    pub note: String,
    pub project: String,
    pub tool: String,
    /// Whether this row is a person rather than an agent.
    pub human: bool,
    /// Whether this agent has actually called `register`, rather than being a
    /// placeholder created to hold a message addressed to it.
    pub registered: bool,
    /// Whether it has been heard from recently enough to count as reachable.
    pub online: bool,
    pub channels: i64,
    pub seen: i64,
}

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ChannelView {
    pub channel: String,
    pub subscribers: i64,
}

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct WorkerView {
    pub name: String,
    pub role: String,
    pub status: String,
    pub process: u32,
    pub restarts: i64,
    pub keepalive: bool,
    pub directory: String,
    pub log: String,
    /// Process id of the session supervising this worker. Zero once no live
    /// session owns it.
    pub supervisor: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_kinds_round_trip_through_their_stored_text() {
        for kind in [
            MessageKind::Direct,
            MessageKind::Channel,
            MessageKind::Broadcast,
        ] {
            assert_eq!(kind.value().parse::<MessageKind>().unwrap(), kind);
        }
        assert!("shout".parse::<MessageKind>().is_err());
    }
}
