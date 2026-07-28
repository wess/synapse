use crate::brain::{Brain, RecallRequest, RecallResponse, RememberRequest, RememberResponse};
use crate::vault::{VaultStatusRequest, VaultStatusResponse, VaultStore};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{Json, ServerHandler, ServiceExt, tool, tool_handler, tool_router, transport::stdio};

#[derive(Clone)]
struct MemoryServer {
    brain: Brain,
    vaults: VaultStore,
    toolrouter: ToolRouter<Self>,
}

#[tool_router(router = toolrouter)]
impl MemoryServer {
    fn new(brain: Brain, vaults: VaultStore) -> Self {
        Self {
            brain,
            vaults,
            toolrouter: Self::toolrouter(),
        }
    }

    #[tool(description = "Store a durable fact, decision, preference, convention, or correction.")]
    async fn remember(
        &self,
        Parameters(request): Parameters<RememberRequest>,
    ) -> Result<Json<RememberResponse>, String> {
        self.brain
            .remember(&request.content, request.source.as_deref())
            .await
            .map(|id| Json(RememberResponse { id, stored: true }))
            .map_err(|error| error.to_string())
    }

    #[tool(
        description = "Recall durable context with a focused query and the smallest practical limit. Use the lean budget first to minimize token use; a per-call budget can only reduce the user-configured response size."
    )]
    async fn recall(
        &self,
        Parameters(request): Parameters<RecallRequest>,
    ) -> Result<Json<RecallResponse>, String> {
        self.brain
            .recallwith(&request.query, request.limit.unwrap_or(8), request.budget)
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

#[tool_handler(router = self.toolrouter)]
impl ServerHandler for MemoryServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(crate::instructions::MEMORY)
            .with_server_info(
                Implementation::new("synapse", env!("CARGO_PKG_VERSION"))
                    .with_title("Synapse")
                    .with_description("Local memory and scoped credential metadata"),
            )
    }
}

pub async fn run(brain: Brain, vaults: VaultStore) -> anyhow::Result<()> {
    let service = MemoryServer::new(brain, vaults).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
