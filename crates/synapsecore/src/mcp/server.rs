use crate::brain::{
    Brain, MemoryScope, ReadRequest, ReadResponse, RecallRequest, RecallResponse, Recalled,
    RememberRequest, RememberResponse,
};
use crate::relay::{Mesh, Supervisor};
use crate::skill::Receipts;
use crate::usage::Entry;
use crate::vault::{VaultStatusRequest, VaultStatusResponse, VaultStore};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::service::{NotificationContext, RoleServer};
use rmcp::{Json, ServerHandler, ServiceExt, tool, tool_handler, tool_router, transport::stdio};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Server {
    pub(super) brain: Brain,
    pub(super) vaults: VaultStore,
    /// Present only when the mesh is switched on. Every mesh tool is absent from
    /// the router otherwise, so a user who does not run agent teams never pays
    /// for their definitions in context.
    pub(super) mesh: Option<Mesh>,
    /// Present only when the learn setting is on, and the whole of what the
    /// self-improvement tools need: the library is the filesystem, and this is
    /// everything Synapse records about it.
    pub(super) receipts: Option<Receipts>,
    /// The background workers this session owns.
    pub(super) supervisor: Supervisor,
    /// The name this session registered on the mesh under. One session is one
    /// agent, so binding it here is the whole of identity.
    pub(super) identity: Arc<Mutex<Option<String>>>,
    pub(super) learningid: Arc<String>,
    instructions: String,
    /// Fixed per-session cost in characters, for the savings ledger.
    standing: usize,
    toolrouter: ToolRouter<Self>,
}

#[tool_router(router = memorytools)]
impl Server {
    fn new(
        brain: Brain,
        vaults: VaultStore,
        mesh: Option<Mesh>,
        receipts: Option<Receipts>,
        knowledge: bool,
        instructions: String,
        standing: usize,
    ) -> Self {
        let mut toolrouter = Self::memorytools();
        if knowledge {
            toolrouter += Self::knowledgetools();
        }
        if mesh.is_some() {
            toolrouter += Self::meshtools();
        }
        if receipts.is_some() {
            toolrouter += Self::learntools();
        }
        Self {
            brain,
            vaults,
            mesh,
            receipts,
            supervisor: Supervisor::new(),
            identity: Arc::new(Mutex::new(None)),
            learningid: Arc::new(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            )),
            instructions,
            standing,
            toolrouter,
        }
    }

    #[tool(
        description = "Store a durable fact, decision, preference, convention, or correction. When it corrects something an earlier recall returned, pass that memory's id in `supersedes` so the outdated version stops being recalled; nothing is deleted. `drift` in the response lists existing memories this one contradicts or repeats: if it replaces one, remember it again with `supersedes` naming both ids."
    )]
    async fn remember(
        &self,
        Parameters(request): Parameters<RememberRequest>,
    ) -> Result<Json<RememberResponse>, String> {
        let project = projectpath(request.project.as_deref());
        self.locate(request.project.as_deref()).await;
        let redacted = crate::brain::redact(&request.content).count;
        let (id, superseded) = self
            .brain
            .rememberreplacing(
                &request.content,
                request.source.as_deref(),
                request.scope.unwrap_or(MemoryScope::Project),
                project.as_deref(),
                request.supersedes.as_deref().unwrap_or_default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let drift = self
            .brain
            .related(id, project.as_deref())
            .await
            .unwrap_or_default();
        Ok(Json(RememberResponse {
            id,
            stored: true,
            superseded,
            redacted,
            drift,
        }))
    }

    #[tool(
        description = "Recall durable context with a focused query and the smallest practical limit. Use the lean budget first; a per-call budget can only reduce the configured ceiling. For an `abridged` hit, use `readmemory` with its id and the same project to read the exact text in bounded pages. `drift` marks hits that look stale or that another memory may contradict or repeat; compare contexts before treating a pair as a correction."
    )]
    async fn recall(
        &self,
        Parameters(request): Parameters<RecallRequest>,
    ) -> Result<Json<RecallResponse>, String> {
        let project = projectpath(request.project.as_deref());
        self.locate(request.project.as_deref()).await;
        let (settings, memories, matched) = self
            .brain
            .recallmeasured(
                &request.query,
                request.limit.unwrap_or(8),
                request.budget,
                project.as_deref(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let drift = self
            .brain
            .drifted(&memories, project.as_deref())
            .await
            .unwrap_or_default();
        let sent = crate::usage::bodies(&memories);
        let response = RecallResponse {
            optimization: settings.optimization,
            memories: memories.into_iter().map(Recalled::from).collect(),
            drift,
            knowledge: if self.brain.knowledge().await.unwrap_or(false) {
                crate::knowledge::work::catalog(project.as_deref()).await
            } else {
                None
            },
        };
        self.count(Entry::recall(
            response.memories.len(),
            sent,
            matched,
            measured(&response),
        ))
        .await;
        Ok(Json(response))
    }

    #[tool(
        description = "Read exact stored text by memory id, using recall's project scope. Follow `next` as the next offset until null. Pages contain at most 6000 UTF-8 bytes, reduced by the configured budget. Missing, superseded, and out-of-scope ids return null. Pages reflect current stored text; restart at zero after an edit."
    )]
    async fn readmemory(
        &self,
        Parameters(request): Parameters<ReadRequest>,
    ) -> Result<Json<ReadResponse>, String> {
        let project = projectpath(request.project.as_deref());
        self.locate(request.project.as_deref()).await;
        let response = self
            .brain
            .readscoped(
                request.id,
                request.offset.unwrap_or(0),
                request.budget,
                project.as_deref(),
            )
            .await
            .map_err(|error| error.to_string())?;
        if let Some(page) = &response.memory {
            self.count(Entry::read(page.body.chars().count(), measured(&response)))
                .await;
        }
        Ok(Json(response))
    }

    #[tool(
        description = "List active Synapse vault variable names and scope trust status for a folder. Secret values are never returned."
    )]
    async fn vaultstatus(
        &self,
        Parameters(request): Parameters<VaultStatusRequest>,
    ) -> Result<Json<VaultStatusResponse>, String> {
        let path = request
            .path
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("SYNAPSE_PROJECT_DIR").map(Into::into))
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| "could not determine the project folder".to_owned())?;
        let mut resolved = crate::vault::resolve(&self.vaults, &path)
            .await
            .map_err(|error| error.to_string())?;
        let ambient = if resolved.scopes.is_empty() {
            "inactive"
        } else if resolved.warnings.is_empty() {
            "ready"
        } else {
            "blocked"
        };
        let (unavailable, elsewhere) = crate::vault::unavailable(&self.vaults, &resolved)
            .await
            .map_err(|error| error.to_string())?;
        // Answering an unreachable secret with four empty lists is what sent
        // somebody to the CLI to find out it existed at all.
        if let Some(advice) = crate::vault::advice(&resolved, &unavailable, elsewhere) {
            resolved.warnings.push(advice);
        }
        let backend = crate::vault::backend()
            .await
            .map_err(|error| error.to_string())?;
        Ok(Json(VaultStatusResponse {
            path: path.display().to_string(),
            backend: backend.name().to_owned(),
            available: resolved.env.keys().cloned().collect(),
            unavailable,
            elsewhere,
            scopes: resolved.scopes.into_iter().map(Into::into).collect(),
            warnings: resolved.warnings,
            ambient: ambient.to_owned(),
            shell: std::env::var("SYNAPSE_SHELL_ACTIVE").ok(),
            note: "Values stay in the vault and never in a response. A name under `available` resolves for this folder; one under `unavailable` is named by a scope file here and needs it approved first. `elsewhere` counts secrets held for other folders, which are never named. Use `synapse run -- <command>` for one child or an installed shell hook for an approved directory."
                .to_owned(),
        }))
    }
}

impl Server {
    /// The skill records, for a session where learning is switched on. A tool
    /// call can only reach this through a router that was merged in for the
    /// same reason, so the error is a guard rather than something a user sees.
    pub(super) fn receipts(&self) -> Result<&Receipts, String> {
        self.receipts.as_ref().ok_or_else(|| {
            "self-improvement is off; turn it on with `synapse settings learn on`".to_owned()
        })
    }
}

/// Serialized size, framing included.
fn measured(response: &impl serde::Serialize) -> usize {
    serde_json::to_string(response)
        .map(|text| text.chars().count())
        .unwrap_or_default()
}

pub(super) fn projectpath(requested: Option<&str>) -> Option<std::path::PathBuf> {
    requested
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("SYNAPSE_PROJECT_DIR").map(Into::into))
        .or_else(|| std::env::current_dir().ok())
}

#[tool_handler(router = self.toolrouter)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(self.instructions.clone())
            .with_server_info(
                Implementation::new("synapse", env!("CARGO_PKG_VERSION"))
                    .with_title("Synapse")
                    .with_description("Local memory, agent mesh, and scoped credential metadata"),
            )
    }

    /// Count the session, unless its tool's SessionStart hook already has.
    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let client = context
            .peer
            .peer_info()
            .map(|info| info.client_info.name.clone())
            .unwrap_or_default();
        let project = projectpath(None)
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let _ = crate::presence::join(self.brain.pool(), &self.learningid, &client, &project).await;
        if crate::usage::hooked(&client) {
            return;
        }
        let project = projectpath(None);
        let reach = self
            .brain
            .reachsize(project.as_deref())
            .await
            .unwrap_or_default();
        self.count(Entry::session(reach.max(0) as usize, self.standing))
            .await;
    }
}

pub async fn run(
    brain: Brain,
    vaults: VaultStore,
    mesh: Option<Mesh>,
    receipts: Option<Receipts>,
    knowledge: bool,
    instructions: String,
    standing: usize,
) -> anyhow::Result<()> {
    let server = Server::new(
        brain,
        vaults,
        mesh,
        receipts,
        knowledge,
        instructions,
        standing,
    );
    let closing = server.clone();
    let service = server.serve(stdio()).await?;
    let heartbeat = closing.clone();
    let pulse = tokio::spawn(async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            timer.tick().await;
            let _ = crate::presence::touch(heartbeat.brain.pool(), &heartbeat.learningid).await;
        }
    });
    let result = service.waiting().await;
    pulse.abort();
    closing.depart().await;
    result?;
    Ok(())
}

impl Server {
    async fn locate(&self, requested: Option<&str>) {
        if let Some(project) = requested.filter(|project| !project.trim().is_empty()) {
            let _ = crate::presence::relocate(self.brain.pool(), &self.learningid, project).await;
        }
    }

    /// Bookkeeping failures are ignored; they must never fail the call.
    async fn count(&self, entry: Entry) {
        let _ = crate::usage::record(&self.brain, entry).await;
    }

    /// Take this session off the mesh when it ends: stop the workers it owns and
    /// drop its roster row, so a closed session neither leaves headless agents
    /// running nor keeps answering as a live teammate.
    async fn depart(&self) {
        let _ = crate::presence::leave(self.brain.pool(), &self.learningid).await;
        let Some(mesh) = &self.mesh else {
            return;
        };
        self.supervisor.stopall(mesh).await;
        let identity = self.identity.lock().await.clone();
        if let Some(name) = identity {
            let _ = mesh.forget(&name).await;
        }
    }
}

impl Server {
    /// The tool definitions a client is sent at these settings, each with the
    /// bytes of JSON it occupies.
    ///
    /// Built from the same three routers `new` composes, because a count taken
    /// any other way is a count of what somebody believed the tool list was.
    /// Nothing is opened and no server is started: this is the shape of the
    /// context cost, not a session.
    pub(super) fn definitions(mesh: bool, learn: bool, knowledge: bool) -> Vec<(String, usize)> {
        let mut router = Self::memorytools();
        if knowledge {
            router += Self::knowledgetools();
        }
        if mesh {
            router += Self::meshtools();
        }
        if learn {
            router += Self::learntools();
        }
        let mut tools: Vec<(String, usize)> = router
            .list_all()
            .into_iter()
            .map(|tool| {
                // What crosses the wire is the serialized definition, so that is
                // what it costs — the name and description alone would miss the
                // input schema, which for a tool with several parameters is most
                // of it.
                let size = serde_json::to_string(&tool)
                    .map(|json| json.len())
                    .unwrap_or_default();
                (tool.name.to_string(), size)
            })
            .collect();
        tools.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
        tools
    }
}
