//! The mesh half of the tool surface: how agents find each other, hand work
//! back and forth, and park for free between tasks.
//!
//! These tools are only in the router when the mesh setting is on, so a user who
//! never runs agent teams never pays for their definitions in context.

use crate::mcp::model::*;
use crate::mcp::server::{Server, projectpath};
use crate::relay::{self, MessageKind};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ProgressNotificationParam, ProgressToken};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{Json, tool, tool_router};
use std::time::Duration;

/// What a park reports back when it ended with nothing, so an agent reads it as
/// routine rather than as a failure to explain.
const IDLE: &str =
    "no messages yet — this is a normal timeout, so call wait again to stay reachable";

#[tool_router(router = meshtools, vis = "pub(super)")]
impl Server {
    #[tool(
        description = "Join the agent mesh under a unique name. Call this once, before any other mesh tool. Returns the current roster so you know who else is here."
    )]
    async fn register(
        &self,
        Parameters(request): Parameters<RegisterRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<RegisterResponse>, String> {
        let mesh = self.mesh()?;
        let project = projectpath(request.project.as_deref())
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let tool = context
            .peer
            .peer_info()
            .map(|info| info.client_info.name.clone())
            .unwrap_or_default();
        let registration = relay::Registration {
            name: request.name.trim().to_owned(),
            role: request.role.unwrap_or_default(),
            capabilities: request.capabilities.unwrap_or_default(),
            project,
            tool,
            // Only `synapse mux` registers a person; a tool calling `register`
            // is always an agent, whoever is sitting in front of it.
            human: false,
        };
        mesh.register(&registration).await.map_err(text)?;
        *self.identity.lock().await = Some(registration.name.clone());
        Ok(Json(RegisterResponse {
            name: registration.name,
            roster: mesh.agents().await.map_err(text)?,
        }))
    }

    #[tool(description = "Send a direct message to one agent by name.")]
    async fn send(
        &self,
        Parameters(request): Parameters<SendRequest>,
    ) -> Result<Json<DeliveredResponse>, String> {
        let (mesh, me) = self.identify().await?;
        let id = relay::deliver(
            mesh,
            &me,
            MessageKind::Direct,
            Some(&request.to),
            &request.body,
        )
        .await
        .map_err(text)?;
        Ok(Json(DeliveredResponse {
            id,
            delivered: format!("sent to {}", request.to),
        }))
    }

    #[tool(description = "Post a message to a channel. Every subscriber receives it.")]
    async fn post(
        &self,
        Parameters(request): Parameters<PostRequest>,
    ) -> Result<Json<DeliveredResponse>, String> {
        let (mesh, me) = self.identify().await?;
        let id = relay::deliver(
            mesh,
            &me,
            MessageKind::Channel,
            Some(&request.channel),
            &request.body,
        )
        .await
        .map_err(text)?;
        Ok(Json(DeliveredResponse {
            id,
            delivered: format!("posted to #{}", request.channel),
        }))
    }

    #[tool(description = "Send a message to every agent on the mesh.")]
    async fn broadcast(
        &self,
        Parameters(request): Parameters<BroadcastRequest>,
    ) -> Result<Json<DeliveredResponse>, String> {
        let (mesh, me) = self.identify().await?;
        let id = relay::deliver(mesh, &me, MessageKind::Broadcast, None, &request.body)
            .await
            .map_err(text)?;
        Ok(Json(DeliveredResponse {
            id,
            delivered: "broadcast to everyone".to_owned(),
        }))
    }

    #[tool(description = "Subscribe to a channel so you receive its posts.")]
    async fn join(
        &self,
        Parameters(request): Parameters<ChannelRequest>,
    ) -> Result<Json<DoneResponse>, String> {
        let (mesh, me) = self.identify().await?;
        mesh.subscribe(&me, &request.channel).await.map_err(text)?;
        Ok(Json(DoneResponse {
            done: true,
            note: format!("joined #{}", request.channel),
        }))
    }

    #[tool(description = "Unsubscribe from a channel.")]
    async fn leave(
        &self,
        Parameters(request): Parameters<ChannelRequest>,
    ) -> Result<Json<DoneResponse>, String> {
        let (mesh, me) = self.identify().await?;
        mesh.unsubscribe(&me, &request.channel)
            .await
            .map_err(text)?;
        Ok(Json(DoneResponse {
            done: true,
            note: format!("left #{}", request.channel),
        }))
    }

    #[tool(
        description = "Block until messages arrive for you, then return them. Call this whenever you have nothing else to do: it is how you stay reachable, and parking costs nothing. After a few idle minutes it returns an empty list instead. That is a normal timeout, not a failure, so call wait again. Do the same if it returns an error."
    )]
    async fn wait(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<MessagesResponse>, String> {
        let (mesh, me) = self.identify().await?;
        // A park sends nothing for minutes, and a client that hears nothing ages
        // the call out. An aborted call reaches the agent as an error rather
        // than an empty result, which is exactly what makes an agent stop
        // looping, so report progress for as long as the park runs.
        let reporter = context
            .meta
            .get_progress_token()
            .map(|token| tokio::spawn(report(context.peer.clone(), token)));
        let parked =
            relay::awaitmessages(mesh, &me, true, Duration::from_secs(relay::PARKSECONDS)).await;
        if let Some(reporter) = reporter {
            reporter.abort();
        }
        self.drained(&me, parked.map_err(text)?).await
    }

    #[tool(description = "Return any messages waiting for you right now, without blocking.")]
    async fn inbox(&self) -> Result<Json<MessagesResponse>, String> {
        let (mesh, me) = self.identify().await?;
        let messages = relay::awaitmessages(mesh, &me, false, Duration::from_secs(0))
            .await
            .map_err(text)?;
        self.drained(&me, messages).await
    }

    #[tool(
        description = "Report your current work state so the rest of the mesh, and the person watching, can see it at a glance: working, idle, blocked, or done. It is cheap; call it whenever your state changes."
    )]
    async fn reportstatus(
        &self,
        Parameters(request): Parameters<ReportStatusRequest>,
    ) -> Result<Json<StatusResponse>, String> {
        let (mesh, me) = self.identify().await?;
        relay::reportstatus(mesh, &me, &request.status)
            .await
            .map_err(text)?;
        Ok(Json(StatusResponse {
            name: me,
            status: request.status,
        }))
    }

    #[tool(
        description = "Block until another agent reaches one of the given states, then return its status. Use it to coordinate, such as waiting for a worker to be done or blocked. Returns the current status on timeout; call again to keep waiting."
    )]
    async fn waitstatus(
        &self,
        Parameters(request): Parameters<WaitStatusRequest>,
    ) -> Result<Json<StatusResponse>, String> {
        let (mesh, _) = self.identify().await?;
        let want = request.status.unwrap_or_default();
        let status = relay::awaitstatus(
            mesh,
            &request.name,
            &want,
            true,
            Duration::from_secs(relay::PARKSECONDS),
        )
        .await
        .map_err(text)?;
        Ok(Json(StatusResponse {
            name: request.name,
            status,
        }))
    }

    #[tool(
        description = "List every agent on the mesh with its role, project, reported status, and whether it is currently reachable. A row with human set to true is a person at a keyboard: ask them questions, never delegate work to them."
    )]
    async fn agents(&self) -> Result<Json<AgentsResponse>, String> {
        let mesh = self.mesh()?;
        Ok(Json(AgentsResponse {
            agents: mesh.agents().await.map_err(text)?,
        }))
    }

    #[tool(description = "List the mesh channels and how many agents subscribe to each.")]
    async fn channels(&self) -> Result<Json<ChannelsResponse>, String> {
        let mesh = self.mesh()?;
        Ok(Json(ChannelsResponse {
            channels: mesh.channels().await.map_err(text)?,
        }))
    }

    #[tool(description = "Show the name you registered under and the channels you are in.")]
    async fn whoami(&self) -> Result<Json<WhoamiResponse>, String> {
        let (mesh, me) = self.identify().await?;
        Ok(Json(WhoamiResponse {
            channels: mesh.subscriptions(&me).await.map_err(text)?,
            name: me,
        }))
    }

    #[tool(
        description = "Start a headless worker that joins this mesh, registers, and parks on wait. Use it to grow your team. The worker runs for as long as this session does, and the number running at once is capped."
    )]
    async fn spawn(
        &self,
        Parameters(request): Parameters<SpawnRequest>,
    ) -> Result<Json<SpawnResponse>, String> {
        let mesh = self.mesh()?;
        let directory = match request
            .directory
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            Some(value) => std::path::PathBuf::from(value),
            None => projectpath(None)
                .ok_or_else(|| "could not determine the project folder".to_owned())?,
        };
        let role = request.role.unwrap_or_else(|| "worker".to_owned());
        let channels = request.channels.unwrap_or_default();
        let tools = request.tools.unwrap_or_default();
        let built = relay::launch(&relay::Options {
            name: Some(&request.name),
            role: &role,
            root: &directory,
            tool: request.tool.as_deref(),
            task: request.task.as_deref(),
            channels: &channels,
            tools: &tools,
            model: request.model.as_deref(),
            lead: false,
            optimize: false,
            headless: true,
            // A headless worker has no terminal in which to answer a prompt.
            skippermissions: true,
            // It keeps its project and user MCP servers alongside Synapse.
            strict: false,
            command: None,
            extra: &[],
        })
        .map_err(text)?;

        let log = self
            .supervisor
            .launch(
                mesh,
                relay::Spec {
                    name: request.name.clone(),
                    role,
                    program: built.program,
                    arguments: built.arguments,
                    environment: built.environment,
                    directory,
                    keepalive: request.keepalive.unwrap_or(true),
                    session: built.session,
                },
            )
            .await
            .map_err(text)?;

        Ok(Json(SpawnResponse {
            name: request.name,
            log: log.display().to_string(),
            note: "the worker will register and park on wait; send it work by name".to_owned(),
        }))
    }

    #[tool(description = "List the headless workers running under this session.")]
    async fn workers(&self) -> Result<Json<WorkersResponse>, String> {
        let mesh = self.mesh()?;
        Ok(Json(WorkersResponse {
            workers: mesh.workers().await.map_err(text)?,
        }))
    }

    #[tool(description = "Stop a headless worker by name.")]
    async fn stopworker(
        &self,
        Parameters(request): Parameters<WorkerRequest>,
    ) -> Result<Json<DoneResponse>, String> {
        let mesh = self.mesh()?;
        let stopped = self
            .supervisor
            .stop(mesh, &request.name)
            .await
            .map_err(text)?;
        Ok(Json(DoneResponse {
            done: stopped,
            note: if stopped {
                format!("stopped {}", request.name)
            } else {
                format!("no worker named {} is running here", request.name)
            },
        }))
    }
}

impl Server {
    fn mesh(&self) -> Result<&crate::relay::Mesh, String> {
        self.mesh
            .as_ref()
            .ok_or_else(|| "the Synapse mesh is switched off".to_owned())
    }

    /// The mesh plus the name this session registered under, refusing every tool
    /// but `register` until it has one — otherwise two sessions would answer as
    /// the same nameless agent and drain each other's inboxes.
    async fn identify(&self) -> Result<(&crate::relay::Mesh, String), String> {
        let mesh = self.mesh()?;
        let identity = self.identity.lock().await.clone();
        let name = identity
            .ok_or_else(|| "you are not on the mesh yet — call register first".to_owned())?;
        // Every tool call proves this session is alive and looping, so one that
        // stops calling ages off the roster.
        let _ = mesh.touch(&name).await;
        Ok((mesh, name))
    }

    /// Wrap a drain, acknowledging delivery only after the messages have been
    /// turned into the reply.
    async fn drained(
        &self,
        me: &str,
        messages: Vec<crate::relay::Message>,
    ) -> Result<Json<MessagesResponse>, String> {
        let mesh = self.mesh()?;
        let note = messages.is_empty().then(|| IDLE.to_owned());
        if let Some(last) = messages.last() {
            relay::ack(mesh, me, last.id).await.map_err(text)?;
        }
        Ok(Json(MessagesResponse { messages, note }))
    }
}

/// Emit rising progress for as long as a park runs. Protocol traffic is what
/// keeps a client from aging the call out; transport keepalives are invisible to
/// the layer that applies the timeout.
async fn report(peer: rmcp::Peer<RoleServer>, token: ProgressToken) {
    let mut count = 0_u64;
    loop {
        tokio::time::sleep(Duration::from_secs(relay::PROGRESSSECONDS)).await;
        count += 1;
        let sent = peer
            .notify_progress(
                ProgressNotificationParam::new(token.clone(), count as f64)
                    .with_message("parked on the mesh"),
            )
            .await;
        if sent.is_err() {
            break;
        }
    }
}

fn text(error: anyhow::Error) -> String {
    format!("{error:#}")
}
