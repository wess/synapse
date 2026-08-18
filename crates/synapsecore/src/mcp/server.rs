use crate::brain::{
    Brain, MemoryScope, RecallRequest, RecallResponse, RememberRequest, RememberResponse,
};
use crate::relay::{Mesh, Supervisor};
use crate::vault::{VaultStatusRequest, VaultStatusResponse, VaultStore};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
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
    /// The background workers this session owns.
    pub(super) supervisor: Supervisor,
    /// The name this session registered on the mesh under. One session is one
    /// agent, so binding it here is the whole of identity.
    pub(super) identity: Arc<Mutex<Option<String>>>,
    instructions: String,
    toolrouter: ToolRouter<Self>,
}

#[tool_router(router = memorytools)]
impl Server {
    fn new(brain: Brain, vaults: VaultStore, mesh: Option<Mesh>, instructions: String) -> Self {
        let mut toolrouter = Self::memorytools();
        if mesh.is_some() {
            toolrouter += Self::meshtools();
        }
        Self {
            brain,
            vaults,
            mesh,
            supervisor: Supervisor::new(),
            identity: Arc::new(Mutex::new(None)),
            instructions,
            toolrouter,
        }
    }

    #[tool(
        description = "Store a durable fact, decision, preference, convention, or correction. When it corrects something an earlier recall returned, pass that memory's id in `supersedes` so the outdated version stops being recalled; nothing is deleted."
    )]
    async fn remember(
        &self,
        Parameters(request): Parameters<RememberRequest>,
    ) -> Result<Json<RememberResponse>, String> {
        let project = projectpath(request.project.as_deref());
        self.brain
            .rememberreplacing(
                &request.content,
                request.source.as_deref(),
                request.scope.unwrap_or(MemoryScope::Project),
                project.as_deref(),
                request.supersedes.as_deref().unwrap_or_default(),
            )
            .await
            .map(|(id, superseded)| {
                Json(RememberResponse {
                    id,
                    stored: true,
                    superseded,
                })
            })
            .map_err(|error| error.to_string())
    }

    #[tool(
        description = "Recall durable context with a focused query and the smallest practical limit. Use the lean budget first to minimize token use; a per-call budget can only reduce the user-configured response size. A result marked `abridged` is that memory's opening line only, because the budget could not carry the rest — read the whole of it by recalling again with a narrower query or a larger budget."
    )]
    async fn recall(
        &self,
        Parameters(request): Parameters<RecallRequest>,
    ) -> Result<Json<RecallResponse>, String> {
        let project = projectpath(request.project.as_deref());
        self.brain
            .recallscoped(
                &request.query,
                request.limit.unwrap_or(8),
                request.budget,
                project.as_deref(),
            )
            .await
            .map(|(settings, memories)| {
                Json(RecallResponse {
                    optimization: settings.optimization,
                    memories,
                })
            })
            .map_err(|error| error.to_string())
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
        let resolved = crate::vault::resolve(&self.vaults, &path)
            .await
            .map_err(|error| error.to_string())?;
        let ambient = if resolved.scopes.is_empty() {
            "inactive"
        } else if resolved.warnings.is_empty() {
            "ready"
        } else {
            "blocked"
        };
        Ok(Json(VaultStatusResponse {
            path: path.display().to_string(),
            available: resolved.env.keys().cloned().collect(),
            scopes: resolved.scopes.into_iter().map(Into::into).collect(),
            warnings: resolved.warnings,
            ambient: ambient.to_owned(),
            shell: std::env::var("SYNAPSE_SHELL_ACTIVE").ok(),
            note: "Values stay in Keychain. Use `synapse run -- <command>` for one child or an installed shell hook for an approved directory."
                .to_owned(),
        }))
    }
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
}

pub async fn run(
    brain: Brain,
    vaults: VaultStore,
    mesh: Option<Mesh>,
    instructions: String,
) -> anyhow::Result<()> {
    let server = Server::new(brain, vaults, mesh, instructions);
    let closing = server.clone();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    closing.depart().await;
    Ok(())
}

impl Server {
    /// Take this session off the mesh when it ends: stop the workers it owns and
    /// drop its roster row, so a closed session neither leaves headless agents
    /// running nor keeps answering as a live teammate.
    async fn depart(&self) {
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
