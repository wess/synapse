use crate::brain::{Brain, Optimization, ReadResponse, RecallResponse, Recalled, RememberResponse};
use anyhow::{Context, Result};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{Json, ServerHandler, ServiceExt, tool, tool_handler, tool_router, transport::stdio};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
pub(crate) struct Grant {
    pub project: String,
    pub database: PathBuf,
    pub generation: String,
    pub authorization: String,
}

impl Grant {
    pub fn load(path: &Path) -> Result<Self> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}

struct Server {
    brain: Brain,
    path: PathBuf,
    grant: Grant,
    toolrouter: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
struct Recall {
    /// Words or phrase describing the project memory needed.
    query: String,
    /// Requested match count, subject to the host's configured limit.
    limit: Option<u32>,
    /// Optional smaller response budget.
    budget: Option<Optimization>,
}

#[derive(Deserialize, JsonSchema)]
struct Read {
    /// Memory id returned by recall.
    id: i64,
    /// UTF-8 byte offset from the previous page.
    offset: Option<u32>,
    /// Optional smaller response budget.
    budget: Option<Optimization>,
}

#[derive(Deserialize, JsonSchema)]
struct Remember {
    /// Durable project fact, decision, preference, or correction to store.
    content: String,
    /// Optional source label.
    source: Option<String>,
    /// Ids of current memories in this project that this correction replaces.
    supersedes: Option<Vec<i64>>,
}

#[tool_router(router = toolrouter)]
impl Server {
    #[tool(description = "Recall durable memory for the project approved on the host.")]
    async fn recall(
        &self,
        Parameters(request): Parameters<Recall>,
    ) -> Result<Json<RecallResponse>, String> {
        let project = self.project()?;
        let (settings, memories, _) = self
            .brain
            .recallkeyed(
                &request.query,
                request.limit.unwrap_or(8),
                request.budget,
                project,
                false,
            )
            .await
            .map_err(|_| "could not recall project memory".to_owned())?;
        Ok(Json(RecallResponse {
            optimization: settings.optimization,
            memories: memories.into_iter().map(Recalled::from).collect(),
            evidence: Vec::new(),
            drift: Vec::new(),
            knowledge: None,
        }))
    }

    #[tool(
        description = "Read exact project memory by id. Follow next as the next offset until null."
    )]
    async fn readmemory(
        &self,
        Parameters(request): Parameters<Read>,
    ) -> Result<Json<ReadResponse>, String> {
        let project = self.project()?;
        self.brain
            .readkeyed(
                request.id,
                request.offset.unwrap_or(0),
                request.budget,
                project,
                false,
            )
            .await
            .map(Json)
            .map_err(|error| error.to_string())
    }

    #[tool(
        description = "Save a durable decision or correction for the project approved on the host."
    )]
    async fn remember(
        &self,
        Parameters(request): Parameters<Remember>,
    ) -> Result<Json<RememberResponse>, String> {
        let project = self.project()?;
        let redacted = crate::brain::redact(&request.content).count;
        let (id, superseded) = self
            .brain
            .rememberproject(
                &request.content,
                request.source.as_deref(),
                project,
                request.supersedes.as_deref().unwrap_or_default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(Json(RememberResponse {
            id,
            stored: true,
            superseded,
            redacted,
            evidence: None,
            drift: Vec::new(),
        }))
    }
}

impl Server {
    fn project(&self) -> Result<&str, String> {
        if Grant::load(&self.path).is_ok_and(|current| {
            current.generation == self.grant.generation
                && current.project == self.grant.project
                && current.database == self.grant.database
        }) {
            Ok(&self.grant.project)
        } else {
            Err("this devcontainer grant has been revoked or changed".to_owned())
        }
    }
}

#[tool_handler(router = self.toolrouter)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("synapse", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Recall relevant project memory before making decisions. Use a focused query and +                 a small Lean budget. Store durable project decisions and corrections with +                 remember. The host has already selected the project for this connection."
                    .to_owned(),
            )
    }
}

pub(crate) async fn run(path: PathBuf, generation: &str) -> Result<()> {
    let grant = Grant::load(&path).context("could not read the devcontainer grant")?;
    anyhow::ensure!(
        grant.generation == generation,
        "this devcontainer grant was replaced"
    );
    let brain = Brain::openkeyed(&grant.database).await?;
    let server = Server {
        brain,
        path,
        grant,
        toolrouter: Server::toolrouter(),
    };
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}
