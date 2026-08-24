use crate::relay::{AgentView, ChannelView, Message, WorkerView};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegisterRequest {
    /// Your unique name on the mesh, such as `supervisor` or `frontend`.
    pub name: String,
    /// Short description of what you are here to do.
    pub role: Option<String>,
    /// Optional free text describing what you can do.
    pub capabilities: Option<String>,
    /// Absolute project root you are working in.
    pub project: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RegisterResponse {
    pub name: String,
    /// Everyone already on the mesh, so you know who to delegate to.
    pub roster: Vec<AgentView>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendRequest {
    /// Recipient agent name.
    pub to: String,
    pub body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PostRequest {
    /// Channel name, such as `frontend`.
    pub channel: String,
    pub body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BroadcastRequest {
    pub body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChannelRequest {
    pub channel: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeliveredResponse {
    pub id: i64,
    /// Where the message went, for your own log.
    pub delivered: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MessagesResponse {
    pub messages: Vec<Message>,
    /// Present when the park ended with nothing to report.
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReportStatusRequest {
    /// One of `working`, `idle`, `blocked`, `done`, or a short label of your own.
    pub status: String,
    /// One line saying what you are actually doing, such as `rewriting the auth
    /// middleware` or `blocked: need the staging database name`. Omit it to keep
    /// the note you last gave.
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WaitStatusRequest {
    /// The agent to watch.
    pub name: String,
    /// States to wait for, such as `done` or `blocked`. Empty matches any
    /// reported state.
    pub status: Option<Vec<String>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StatusResponse {
    pub name: String,
    pub status: String,
    /// What the agent said it was doing, empty when it gave no note.
    pub note: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AgentsResponse {
    pub agents: Vec<AgentView>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChannelsResponse {
    pub channels: Vec<ChannelView>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WhoamiResponse {
    pub name: String,
    pub channels: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpawnRequest {
    /// Unique name for the new worker.
    pub name: String,
    /// Role it launches with, such as `backend`. Defaults to `worker`.
    pub role: Option<String>,
    /// Standing focus for this worker, distinct from its durable role.
    pub task: Option<String>,
    /// Which connected tool to run: `claude`, `codex`, or `pi`.
    pub tool: Option<String>,
    /// Channels the worker should join.
    pub channels: Option<Vec<String>>,
    /// Tool rules to pre-grant, such as `Read` or `Bash(git:*)`.
    pub tools: Option<Vec<String>>,
    /// Model override.
    pub model: Option<String>,
    /// Working directory. Defaults to the project you registered with.
    pub directory: Option<String>,
    /// Restart the worker if it exits. Defaults to true.
    pub keepalive: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SpawnResponse {
    pub name: String,
    /// Where the worker's output is written.
    pub log: String,
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkerRequest {
    pub name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WorkersResponse {
    pub workers: Vec<WorkerView>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DoneResponse {
    pub done: bool,
    pub note: String,
}

/// Where a taught skill belongs. The same split memory makes, and the same
/// default: a procedure worked out in a repository is usually about it.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillScope {
    Global,
    Project,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeachRequest {
    /// Short lowercase name, words joined by hyphens, such as
    /// `cut-a-release` or `debug-a-flaky-test`.
    pub name: String,
    /// One line saying what the skill does and when to reach for it. Agents read
    /// this line alone to decide whether to load the rest, so lead with the case
    /// that matters most.
    pub description: String,
    /// The procedure itself, as Markdown. Write it for somebody doing this for
    /// the first time: what it is for, the steps in order, and what to watch out
    /// for. Do not include a frontmatter fence; Synapse writes that.
    pub instructions: String,
    /// `project` by default. Use `global` only for a procedure that holds away
    /// from this repository.
    pub scope: Option<SkillScope>,
    /// Absolute project root this belongs to.
    pub project: Option<String>,
    /// One line for the user saying why this is worth keeping.
    pub note: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TeachResponse {
    pub skill: String,
    pub scope: String,
    pub path: String,
    /// Always true: a taught skill waits for the user, by design.
    pub proposed: bool,
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReviseRequest {
    /// The skill to correct, by name.
    pub name: String,
    /// The corrected instructions, in full. This replaces the body rather than
    /// being appended to it.
    pub instructions: String,
    /// A new one-line description, when the old one no longer fits.
    pub description: Option<String>,
    /// One line saying what was wrong with the version being replaced.
    pub note: Option<String>,
    /// Absolute project root, so a project's own copy is found before the
    /// shared one.
    pub project: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ReviseResponse {
    pub skill: String,
    pub scope: String,
    /// The id holding what it used to say, restorable with `synapse skill
    /// revert`.
    pub revision: i64,
    /// The tools this correction reached.
    pub updated: Vec<String>,
}
