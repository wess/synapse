use crate::brain::{
    Brain, MAXBODY, Optimization, ReadResponse, RecallResponse, Recalled, RememberResponse,
};
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::{async_rw::AsyncRwTransport, stdio_pipes};
use rmcp::{Json, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::{Semaphore, SemaphorePermit};

const SESSIONS: usize = 8;
const CALLS: usize = 2;
const FRAME: usize = 2 * 1024 * 1024;

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
    calls: Semaphore,
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
        let (project, _permit) = self.project()?;
        if request.query.len() > 4096 {
            return Err("a recall query can be at most 4096 bytes".to_owned());
        }
        let (settings, mut memories, _) = self
            .brain
            .recallkeyed(
                &request.query,
                request.limit.unwrap_or(8),
                Some(Optimization::Balanced.constrained(request.budget)),
                project,
                false,
            )
            .await
            .map_err(|_| "could not recall project memory".to_owned())?;
        if Path::new(project).is_absolute() {
            for memory in &mut memories {
                memory.project.clear();
            }
        }
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
        let (project, _permit) = self.project()?;
        let mut response = self
            .brain
            .readkeyed(
                request.id,
                request.offset.unwrap_or(0),
                request.budget,
                project,
                false,
            )
            .await
            .map_err(|error| error.to_string())?;
        if Path::new(project).is_absolute()
            && let Some(memory) = &mut response.memory
        {
            memory.project.clear();
        }
        Ok(Json(response))
    }

    #[tool(
        description = "Save a durable decision or correction for the project approved on the host."
    )]
    async fn remember(
        &self,
        Parameters(request): Parameters<Remember>,
    ) -> Result<Json<RememberResponse>, String> {
        let (project, _permit) = self.project()?;
        let supersedes = request.supersedes.as_deref().unwrap_or_default();
        if request.content.len() > MAXBODY {
            return Err(format!("a memory can be at most {} KiB", MAXBODY / 1024));
        }
        if request
            .source
            .as_ref()
            .is_some_and(|source| source.len() > 2048)
        {
            return Err("a source label can be at most 2048 bytes".to_owned());
        }
        if supersedes.len() > 64 {
            return Err("a correction can replace at most 64 memories".to_owned());
        }
        let redacted = crate::brain::redact(&request.content).count;
        let (id, superseded) = self
            .brain
            .rememberproject(
                &request.content,
                request.source.as_deref(),
                project,
                supersedes,
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
    fn project(&self) -> Result<(&str, SemaphorePermit<'_>), String> {
        let permit = self
            .calls
            .try_acquire()
            .map_err(|_| "the devcontainer session is busy; retry the call".to_owned())?;
        if Grant::load(&self.path).is_ok_and(|current| {
            current.generation == self.grant.generation
                && current.project == self.grant.project
                && current.database == self.grant.database
        }) {
            Ok((&self.grant.project, permit))
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
                concat!(
                    "Recall relevant project memory before making decisions. Use a focused query and ",
                    "a small Lean budget. Store durable project decisions and corrections with ",
                    "remember. The host has already selected the project for this connection."
                ).to_owned(),
            )
    }
}

pub(crate) async fn run(path: PathBuf, generation: &str) -> Result<()> {
    let grant = Grant::load(&path).context("could not read the devcontainer grant")?;
    anyhow::ensure!(
        grant.generation == generation,
        "this devcontainer grant was replaced"
    );
    let _slot = session(&path)?;
    // Setup already scans the host store. Connection startup still checks
    // relationships and migrations without rescanning every SQLite page.
    let brain = Brain::glance(&grant.database).await?;
    let server = Server {
        brain,
        path,
        grant,
        calls: Semaphore::new(CALLS),
        toolrouter: Server::toolrouter(),
    };
    let (input, output) = stdio_pipes();
    let transport = AsyncRwTransport::new_server(input, output).with_max_line_length(FRAME);
    tokio::time::timeout(Duration::from_secs(10), server.serve(transport))
        .await
        .context("the devcontainer MCP handshake timed out")??
        .waiting()
        .await?;
    Ok(())
}

fn session(path: &Path) -> Result<File> {
    let directory = path
        .parent()
        .and_then(Path::parent)
        .context("invalid devcontainer grant path")?
        .join("sessions");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)?;
    for slot in 0..SESSIONS {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join(slot.to_string()))?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => return Ok(file),
            Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
            Err(error) => return Err(error).context("could not reserve a devcontainer session"),
        }
    }
    bail!("all {SESSIONS} devcontainer sessions are in use on this host account")
}
