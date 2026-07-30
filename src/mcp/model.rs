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
    /// Which connected tool to run: `claude` or `codex`.
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
