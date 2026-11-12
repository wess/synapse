use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::str::FromStr;

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Level {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl FromStr for Level {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => anyhow::bail!("expected trace, debug, info, warn, or error"),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct EventDraft {
    pub source: String,
    pub project: String,
    pub agent: String,
    pub session: String,
    pub level: Level,
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub fields: serde_json::Value,
    pub runid: String,
    pub traceid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq)]
pub struct Event {
    pub id: i64,
    pub created: i64,
    pub source: String,
    pub project: String,
    pub agent: String,
    pub session: String,
    pub level: Level,
    pub kind: String,
    pub message: String,
    pub fields: String,
    pub runid: String,
    pub traceid: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct EventFilter {
    pub after: i64,
    pub limit: i64,
    pub project: Option<String>,
    pub agent: Option<String>,
    pub source: Option<String>,
    pub level: Option<Level>,
    pub runid: Option<String>,
    pub query: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
pub struct Summary {
    pub liveagents: usize,
    pub running: usize,
    pub failed: usize,
    pub queued: usize,
    pub errors: usize,
    pub lastevent: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum RunStatus {
    Queued,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
}

impl FromStr for RunStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting" => Ok(Self::Waiting),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => anyhow::bail!("invalid run status"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Run {
    pub id: String,
    pub project: String,
    pub harness: String,
    pub status: RunStatus,
    pub input: String,
    pub output: String,
    pub error: String,
    pub replayof: String,
    pub parent: String,
    pub title: String,
    pub leaseowner: String,
    pub leaseuntil: i64,
    pub attempts: i64,
    pub maxattempts: i64,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Change {
    pub id: i64,
    pub runid: String,
    pub stepid: Option<i64>,
    pub path: String,
    pub operation: String,
    pub previouspath: String,
    pub beforedigest: String,
    pub afterdigest: String,
    pub summary: String,
    pub created: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Checkpoint {
    pub id: String,
    pub runid: String,
    pub parent: String,
    pub label: String,
    pub root: String,
    pub manifest: String,
    pub state: String,
    pub restored: i64,
    pub created: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub runid: String,
    pub root: String,
    pub path: String,
    pub branch: String,
    pub base: String,
    pub status: String,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Review {
    pub id: i64,
    pub runid: String,
    pub reviewer: String,
    pub verdict: String,
    pub summary: String,
    pub checks: String,
    pub reasoning: String,
    pub concerns: String,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Comment {
    pub id: i64,
    pub runid: String,
    pub reviewid: Option<i64>,
    pub author: String,
    pub targetkind: String,
    pub target: String,
    pub line: i64,
    pub body: String,
    pub parent: Option<i64>,
    pub resolved: bool,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Runner {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub capabilities: String,
    pub status: String,
    pub local: bool,
    pub lastseen: i64,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Preview {
    pub id: String,
    pub runid: String,
    pub runnerid: String,
    pub url: String,
    pub status: String,
    pub expires: i64,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct PortableArtifact {
    pub artifact: Artifact,
    pub roles: Vec<String>,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct RunBundle {
    pub version: u32,
    pub run: Run,
    pub steps: Vec<Step>,
    pub changes: Vec<Change>,
    pub reviews: Vec<Review>,
    pub comments: Vec<Comment>,
    pub evaluations: Vec<Evaluation>,
    pub artifacts: Vec<PortableArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct RunStats {
    pub runid: String,
    pub changes: i64,
    pub reviews: i64,
    pub opencomments: i64,
    pub checkpoints: i64,
    pub previews: i64,
    pub workspace: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum StepStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Step {
    pub id: i64,
    pub runid: String,
    pub name: String,
    pub status: StepStatus,
    pub input: String,
    pub output: String,
    pub error: String,
    pub started: i64,
    pub ended: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Artifact {
    pub digest: String,
    pub name: String,
    pub kind: String,
    pub media: String,
    pub path: String,
    pub size: i64,
    pub created: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Deny,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Capability {
    pub subject: String,
    pub kind: String,
    pub resource: String,
    pub decision: Decision,
    pub digest: String,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Trigger {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub spec: String,
    pub enabled: bool,
    pub next: i64,
    pub lastrun: i64,
    pub claimed: i64,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq, Eq)]
pub struct Route {
    pub name: String,
    pub endpoint: String,
    pub model: String,
    pub priority: i64,
    pub local: bool,
    pub enabled: bool,
    pub created: i64,
    pub updated: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, FromRow, PartialEq)]
pub struct Evaluation {
    pub id: i64,
    pub runid: String,
    pub expected: String,
    pub actual: String,
    pub score: f64,
    pub note: String,
    pub created: i64,
}
