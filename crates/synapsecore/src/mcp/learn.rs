//! The self-improvement half of the tool surface: how a session writes down a
//! procedure it worked out, and corrects one that turned out wrong.
//!
//! These are in the router only when the learn setting is on, so a user who does
//! not want agents editing a skill library never pays for their definitions in
//! context — the same bargain the mesh tools make.
//!
//! Synapse has no model and runs no loop, so it never reflects on a session.
//! The agent reflects; this is where the result lands. What Synapse decides is
//! narrower and more useful: that a taught skill goes to the library and to no
//! tool until a person says so, and that a revised one keeps what it used to
//! say.

use crate::mcp::model::*;
use crate::mcp::server::{Server, projectpath};
use crate::skill::{self, Shelf};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{Json, tool, tool_router};

#[tool_router(router = learntools, vis = "pub(super)")]
impl Server {
    #[tool(
        description = "Write down a procedure this session worked out so a later session can follow it, as an Agent Skill. Use it for something you had to figure out — a sequence of steps, a checklist, a way around something undocumented — not for what the repository already explains, and not for what this session happened to do. Give the instructions as you would to somebody doing it for the first time. Use project scope by default and pass the absolute project root. The skill goes into the library and into no tool until the user approves it, so writing one never changes their sessions behind their back."
    )]
    async fn teach(
        &self,
        Parameters(request): Parameters<TeachRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<TeachResponse>, String> {
        let receipts = self.receipts()?;
        let shelf = shelf(request.scope, request.project.as_deref());
        let name = request.name.trim().to_lowercase().replace([' ', '_'], "-");
        let path = skill::teach(
            receipts,
            &shelf,
            &name,
            &request.description,
            &request.instructions,
            &tool(&context),
            request.note.as_deref().unwrap_or_default(),
        )
        .await
        .map_err(|error| format!("{error:#}"))?;

        Ok(Json(TeachResponse {
            skill: name,
            scope: shelf.label().to_owned(),
            path: path.display().to_string(),
            proposed: true,
            note: "Stored in the Synapse library and waiting for the user to approve it. It is not installed in any tool yet, so do not rely on it this session."
                .to_owned(),
        }))
    }

    #[tool(
        description = "Correct a skill that turned out wrong, incomplete, or out of date, replacing its instructions and saying in one line what was wrong with it. Unlike a new skill this reaches the copies Synapse installed, because the user already agreed to this skill being loaded and a correction that never arrives leaves every session running the version that was wrong. What it used to say is kept and can be restored. A copy the user edited by hand is theirs and is left alone."
    )]
    async fn revise(
        &self,
        Parameters(request): Parameters<ReviseRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<ReviseResponse>, String> {
        let receipts = self.receipts()?;
        let home = crate::files::home().map_err(|error| error.to_string())?;
        let name = request.name.trim().to_lowercase();
        let project = projectpath(request.project.as_deref());
        let existing =
            skill::library::locate(&name, project.as_deref()).map_err(|error| error.to_string())?;
        let (revision, reached) = skill::revise(
            receipts,
            &home,
            &existing,
            request.description.as_deref(),
            &request.instructions,
            &tool(&context),
            request.note.as_deref().unwrap_or_default(),
        )
        .await
        .map_err(|error| format!("{error:#}"))?;

        Ok(Json(ReviseResponse {
            skill: name,
            scope: existing.shelf.label().to_owned(),
            revision,
            updated: reached,
        }))
    }
}

/// The shelf a request means. Project by default and with no root given, the
/// call still lands somewhere sensible: `projectpath` falls back to the folder
/// the session was started in.
fn shelf(scope: Option<SkillScope>, project: Option<&str>) -> Shelf {
    match scope.unwrap_or(SkillScope::Project) {
        SkillScope::Global => Shelf::Global,
        SkillScope::Project => projectpath(project)
            .map(|path| Shelf::project(&path))
            .unwrap_or(Shelf::Global),
    }
}

/// Which tool's session this came from, so the queue can say who wrote what.
fn tool(context: &RequestContext<RoleServer>) -> String {
    context
        .peer
        .peer_info()
        .map(|info| info.client_info.name.clone())
        .unwrap_or_default()
}
