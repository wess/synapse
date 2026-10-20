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

use crate::brain::drift::Finding;
use crate::learning::{Disposition, Feedback, Outcome, Verification};
use crate::mcp::model::*;
use crate::mcp::server::{Server, projectpath};
use crate::skill::{self, Shelf};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{Json, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(JsonSchema)]
struct FeedbackRequest {
    feedback: Feedback,
    project: Option<String>,
}

impl<'de> Deserialize<'de> for FeedbackRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            feedback: Feedback,
            project: Option<String>,
        }
        let input = Input::deserialize(deserializer)
            .map_err(|_| serde::de::Error::custom("invalid feedback parameters"))?;
        Ok(Self {
            feedback: input.feedback,
            project: input.project,
        })
    }
}

#[derive(Serialize, JsonSchema)]
struct FeedbackReceipt {
    id: i64,
    outcome: Outcome,
    verification: Verification,
    qualified: bool,
    unstable: bool,
}

#[derive(JsonSchema)]
struct ResolveRequest {
    finding: Finding,
    disposition: Disposition,
    note: String,
    project: Option<String>,
}

impl<'de> Deserialize<'de> for ResolveRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            finding: Finding,
            disposition: Disposition,
            note: String,
            project: Option<String>,
        }
        let input = Input::deserialize(deserializer)
            .map_err(|_| serde::de::Error::custom("invalid drift resolution parameters"))?;
        Ok(Self {
            finding: input.finding,
            disposition: input.disposition,
            note: input.note,
            project: input.project,
        })
    }
}

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
        description = "Propose a corrected skill with its full replacement instructions and a reason. The exact diff waits for approval through synapse learn approve; no installed copy changes before review. Approval preserves revision history and leaves hand-edited copies alone."
    )]
    async fn revise(
        &self,
        Parameters(request): Parameters<ReviseRequest>,
    ) -> Result<Json<ReviseResponse>, String> {
        self.receipts()?;
        let name = request.name.trim().to_lowercase();
        let project = projectpath(request.project.as_deref());
        let project =
            project.ok_or_else(|| "a skill revision needs a project folder".to_owned())?;
        let existing =
            skill::library::locate(&name, Some(&project)).map_err(|error| error.to_string())?;
        let proposal = self
            .brain
            .proposerevision(
                &project,
                &existing,
                request.description.as_deref(),
                &request.instructions,
                request.note.as_deref().unwrap_or_default(),
                &self.learningid,
            )
            .await
            .map_err(|error| format!("{error:#}"))?;

        Ok(Json(ReviseResponse {
            skill: name,
            scope: existing.shelf.label().to_owned(),
            revision: 0,
            updated: Vec::new(),
            proposed: proposal.state == "proposed",
            proposal: Some(proposal.id),
            state: proposal.state,
        }))
    }

    #[tool(
        description = "Record an outcome for a memory or skill, the mistake, repeatable correction, and evidence. Claims stay reported until a check or user confirmation verifies them. Use unresolved after a passing retry when the cause is still unknown. Include query and expected only for a proposed recall regression case. Never send credentials or raw command output."
    )]
    async fn feedback(
        &self,
        Parameters(request): Parameters<FeedbackRequest>,
    ) -> Result<Json<FeedbackReceipt>, String> {
        self.receipts()?;
        let folder = projectpath(request.project.as_deref())
            .ok_or_else(|| "feedback needs a project folder".to_owned())?;
        let record = self
            .brain
            .feedback(request.feedback, &folder, &self.learningid)
            .await
            .map_err(|error| format!("{error:#}"))?;
        Ok(Json(FeedbackReceipt {
            id: record.id,
            outcome: record.outcome,
            verification: record.verification,
            qualified: record.qualified(),
            unstable: record.unstable,
        }))
    }

    #[tool(
        description = "Resolve one exact drift finding after checking its context. Use contextual when both claims apply in different contexts, falsepositive when the warning is mistaken, and corrected only after supersession. Give the evidence in a short note. Resolutions are project scoped, reversible, and stop applying when memory content changes."
    )]
    async fn resolvedrift(
        &self,
        Parameters(request): Parameters<ResolveRequest>,
    ) -> Result<Json<crate::learning::Resolution>, String> {
        self.receipts()?;
        let folder = projectpath(request.project.as_deref())
            .ok_or_else(|| "a drift resolution needs a project folder".to_owned())?;
        self.brain
            .resolvefinding(
                &folder,
                &request.finding,
                request.disposition,
                &request.note,
            )
            .await
            .map(Json)
            .map_err(|error| format!("{error:#}"))
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
