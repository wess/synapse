//! Versioned JSONL interface for local harnesses.

use super::{Backend, Decision, EventDraft, EventFilter, RunBundle, RunStatus, StepStatus};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;

pub const VERSION: u32 = 1;
const LINELIMIT: usize = 1024 * 1024;
const ARTIFACTREADLIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct Request {
    pub version: u32,
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct Response {
    pub version: u32,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    fn ok(id: Value, result: impl Serialize) -> Result<Self> {
        Ok(Self {
            version: VERSION,
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    fn error(id: Value, error: impl std::fmt::Display) -> Self {
        Self {
            version: VERSION,
            id,
            result: None,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Deserialize)]
struct RunCreate {
    project: String,
    harness: String,
    #[serde(default)]
    input: Value,
}

#[derive(Deserialize)]
struct RunUpdate {
    id: String,
    status: RunStatus,
    #[serde(default)]
    output: String,
    #[serde(default)]
    error: String,
}

#[derive(Deserialize)]
struct Named {
    id: String,
}

#[derive(Deserialize)]
struct Limited {
    #[serde(default = "default_limit")]
    limit: i64,
}

#[derive(Deserialize)]
struct StepCreate {
    runid: String,
    name: String,
    #[serde(default)]
    input: Value,
}

#[derive(Deserialize)]
struct StepUpdate {
    id: i64,
    status: StepStatus,
    #[serde(default)]
    output: String,
    #[serde(default)]
    error: String,
}

#[derive(Deserialize)]
struct RunNamed {
    runid: String,
}

#[derive(Deserialize)]
struct RunClaim {
    worker: String,
    #[serde(default = "default_lease")]
    lease: i64,
}

#[derive(Deserialize)]
struct RunRenew {
    id: String,
    worker: String,
    #[serde(default = "default_lease")]
    lease: i64,
}

#[derive(Deserialize)]
struct RunFork {
    id: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct ChangeRecord {
    runid: String,
    stepid: Option<i64>,
    path: String,
    operation: String,
    #[serde(default)]
    previouspath: String,
    #[serde(default)]
    beforedigest: String,
    #[serde(default)]
    afterdigest: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
struct ReviewSubmit {
    runid: String,
    #[serde(default)]
    reviewer: String,
    #[serde(default = "pending")]
    verdict: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    checks: Value,
    #[serde(default)]
    reasoning: String,
    #[serde(default)]
    concerns: Value,
}

#[derive(Deserialize)]
struct CommentCreate {
    runid: String,
    reviewid: Option<i64>,
    #[serde(default)]
    author: String,
    #[serde(default = "run_target")]
    targetkind: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    line: i64,
    body: String,
    parent: Option<i64>,
}

#[derive(Deserialize)]
struct CommentResolve {
    id: i64,
    #[serde(default = "yes")]
    resolved: bool,
}

#[derive(Deserialize)]
struct RunnerPut {
    id: String,
    name: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    capabilities: Value,
    #[serde(default = "yes")]
    local: bool,
}

#[derive(Deserialize)]
struct RunnerHeartbeat {
    id: String,
    #[serde(default = "online")]
    status: String,
}

#[derive(Deserialize)]
struct PreviewPut {
    id: String,
    runid: String,
    runnerid: String,
    url: String,
    #[serde(default = "starting")]
    status: String,
    #[serde(default)]
    expires: i64,
}

#[derive(Deserialize)]
struct CheckpointCreate {
    runid: String,
    root: PathBuf,
    paths: Vec<PathBuf>,
    #[serde(default)]
    label: String,
    #[serde(default)]
    state: Value,
}

#[derive(Deserialize)]
struct WorkspaceCreate {
    runid: String,
    root: PathBuf,
    base: Option<String>,
}

#[derive(Deserialize)]
struct BundleImport {
    bundle: RunBundle,
    project: Option<String>,
}

#[derive(Deserialize)]
struct ArtifactPut {
    name: String,
    kind: String,
    media: String,
    content: String,
    #[serde(default)]
    encoding: String,
}

#[derive(Deserialize)]
struct Digest {
    digest: String,
}

#[derive(Deserialize)]
struct ArtifactAttach {
    runid: String,
    digest: String,
    role: String,
}

#[derive(Deserialize)]
struct CapabilityDecision {
    subject: String,
    kind: String,
    resource: String,
    decision: Decision,
    #[serde(default)]
    digest: String,
}

#[derive(Deserialize)]
struct CapabilityCheck {
    subject: String,
    kind: String,
    resource: String,
    #[serde(default)]
    digest: String,
}

#[derive(Deserialize)]
struct TriggerPut {
    name: String,
    kind: String,
    #[serde(default)]
    spec: Value,
    #[serde(default)]
    next: i64,
}

#[derive(Deserialize)]
struct TriggerDue {
    time: i64,
}

#[derive(Deserialize)]
struct TriggerMark {
    name: String,
    next: i64,
}

#[derive(Deserialize)]
struct RoutePut {
    name: String,
    endpoint: String,
    model: String,
    #[serde(default)]
    priority: i64,
    #[serde(default)]
    local: bool,
}

#[derive(Deserialize)]
struct RouteSelect {
    local: Option<bool>,
}

#[derive(Deserialize)]
struct RouteCall {
    local: Option<bool>,
    #[serde(default)]
    body: Value,
}

#[derive(Deserialize)]
struct EvaluationAdd {
    runid: String,
    expected: String,
    actual: String,
    score: f64,
    #[serde(default)]
    note: String,
}

fn default_limit() -> i64 {
    100
}

fn default_lease() -> i64 {
    60
}

fn pending() -> String {
    "pending".to_owned()
}

fn run_target() -> String {
    "run".to_owned()
}

fn online() -> String {
    "online".to_owned()
}

fn starting() -> String {
    "starting".to_owned()
}

fn yes() -> bool {
    true
}

fn params<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).context("invalid request parameters")
}

pub async fn handle(backend: &Backend, request: Request) -> Response {
    let id = request.id;
    if request.version != VERSION {
        return Response::error(id, format!("unsupported API version {}", request.version));
    }
    let result: Result<Response> = async {
        match request.method.as_str() {
            "summary" => Response::ok(id.clone(), backend.summary().await?),
            "events.list" => {
                let filter = params::<EventFilter>(request.params)?;
                Response::ok(id.clone(), backend.events(&filter).await?)
            }
            "events.emit" => {
                let draft = params::<EventDraft>(request.params)?;
                Response::ok(id.clone(), backend.record(draft).await?)
            }
            "runs.list" => {
                let input = params::<Limited>(request.params)?;
                Response::ok(id.clone(), backend.runs(input.limit).await?)
            }
            "runs.create" => {
                let input = params::<RunCreate>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .createrun(&input.project, &input.harness, input.input)
                        .await?,
                )
            }
            "runs.update" => {
                let input = params::<RunUpdate>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .updaterun(&input.id, input.status, &input.output, &input.error)
                        .await?,
                )
            }
            "runs.replay" => {
                let input = params::<Named>(request.params)?;
                Response::ok(id.clone(), backend.replay(&input.id).await?)
            }
            "runs.fork" => {
                let input = params::<RunFork>(request.params)?;
                Response::ok(id.clone(), backend.forkrun(&input.id, &input.title).await?)
            }
            "runs.export" => {
                let input = params::<Named>(request.params)?;
                Response::ok(id.clone(), backend.exportbundle(&input.id).await?)
            }
            "runs.import" => {
                let input = params::<BundleImport>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .importbundle(input.bundle, input.project.as_deref())
                        .await?,
                )
            }
            "runs.claim" => {
                let input = params::<RunClaim>(request.params)?;
                Response::ok(id.clone(), backend.claim(&input.worker, input.lease).await?)
            }
            "runs.renew" => {
                let input = params::<RunRenew>(request.params)?;
                let renewed = backend.renew(&input.id, &input.worker, input.lease).await?;
                Response::ok(id.clone(), json!({ "renewed": renewed }))
            }
            "runs.cancel" => {
                let input = params::<Named>(request.params)?;
                Response::ok(id.clone(), backend.cancel(&input.id).await?)
            }
            "runs.retry" => {
                let input = params::<Named>(request.params)?;
                Response::ok(id.clone(), backend.retry(&input.id).await?)
            }
            "steps.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.steps(&input.runid).await?)
            }
            "steps.create" => {
                let input = params::<StepCreate>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .addstep(&input.runid, &input.name, input.input)
                        .await?,
                )
            }
            "steps.update" => {
                let input = params::<StepUpdate>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .updatestep(input.id, input.status, &input.output, &input.error)
                        .await?,
                )
            }
            "changes.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.changes(&input.runid).await?)
            }
            "changes.record" => {
                let input = params::<ChangeRecord>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .recordchange(
                            &input.runid,
                            input.stepid,
                            &input.path,
                            &input.operation,
                            &input.previouspath,
                            &input.beforedigest,
                            &input.afterdigest,
                            &input.summary,
                        )
                        .await?,
                )
            }
            "reviews.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.reviews(&input.runid).await?)
            }
            "reviews.submit" => {
                let input = params::<ReviewSubmit>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .submitreview(
                            &input.runid,
                            &input.reviewer,
                            &input.verdict,
                            &input.summary,
                            input.checks,
                            &input.reasoning,
                            input.concerns,
                        )
                        .await?,
                )
            }
            "comments.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.comments(&input.runid).await?)
            }
            "comments.create" => {
                let input = params::<CommentCreate>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .comment(
                            &input.runid,
                            input.reviewid,
                            &input.author,
                            &input.targetkind,
                            &input.target,
                            input.line,
                            &input.body,
                            input.parent,
                        )
                        .await?,
                )
            }
            "comments.resolve" => {
                let input = params::<CommentResolve>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend.resolvecomment(input.id, input.resolved).await?,
                )
            }
            "artifacts.put" => {
                let input = params::<ArtifactPut>(request.params)?;
                let content = match input.encoding.as_str() {
                    "" | "utf8" => input.content.into_bytes(),
                    "hex" => decodehex(&input.content)?,
                    _ => anyhow::bail!("artifact encoding must be utf8 or hex"),
                };
                Response::ok(
                    id.clone(),
                    backend
                        .putartifact(&input.name, &input.kind, &input.media, &content)
                        .await?,
                )
            }
            "artifacts.attach" => {
                let input = params::<ArtifactAttach>(request.params)?;
                backend
                    .attach(&input.runid, &input.digest, &input.role)
                    .await?;
                Response::ok(id.clone(), json!({ "attached": true }))
            }
            "artifacts.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.artifacts(&input.runid).await?)
            }
            "artifacts.read" => {
                let input = params::<Digest>(request.params)?;
                let content = backend.readartifact(&input.digest).await?;
                anyhow::ensure!(
                    content.len() <= ARTIFACTREADLIMIT,
                    "artifact is too large for JSONL; use the artifact read command"
                );
                Response::ok(
                    id.clone(),
                    json!({
                        "digest": input.digest,
                        "encoding": "hex",
                        "content": hex(&content),
                    }),
                )
            }
            "capabilities.list" => Response::ok(id.clone(), backend.capabilities().await?),
            "capabilities.decide" => {
                let input = params::<CapabilityDecision>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .decide(
                            &input.subject,
                            &input.kind,
                            &input.resource,
                            input.decision,
                            &input.digest,
                        )
                        .await?,
                )
            }
            "capabilities.check" => {
                let input = params::<CapabilityCheck>(request.params)?;
                let allowed = backend
                    .allowed(&input.subject, &input.kind, &input.resource, &input.digest)
                    .await?;
                Response::ok(id.clone(), json!({ "allowed": allowed }))
            }
            "triggers.list" => Response::ok(id.clone(), backend.triggers().await?),
            "triggers.put" => {
                let input = params::<TriggerPut>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .puttrigger(&input.name, &input.kind, input.spec, input.next)
                        .await?,
                )
            }
            "triggers.due" => {
                let input = params::<TriggerDue>(request.params)?;
                Response::ok(id.clone(), backend.due(input.time).await?)
            }
            "triggers.mark" => {
                let input = params::<TriggerMark>(request.params)?;
                backend.marktrigger(&input.name, input.next).await?;
                Response::ok(id.clone(), json!({ "marked": true }))
            }
            "triggers.tick" => {
                let input = params::<TriggerDue>(request.params)?;
                Response::ok(id.clone(), backend.tick(input.time).await?)
            }
            "routes.list" => Response::ok(id.clone(), backend.routes().await?),
            "routes.put" => {
                let input = params::<RoutePut>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .putroute(
                            &input.name,
                            &input.endpoint,
                            &input.model,
                            input.priority,
                            input.local,
                        )
                        .await?,
                )
            }
            "routes.select" => {
                let input = params::<RouteSelect>(request.params)?;
                Response::ok(id.clone(), backend.selectroute(input.local).await?)
            }
            "routes.call" => {
                let input = params::<RouteCall>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend.callroute(input.local, input.body).await?,
                )
            }
            "evaluations.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.evaluations(&input.runid).await?)
            }
            "evaluations.add" => {
                let input = params::<EvaluationAdd>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .evaluate(
                            &input.runid,
                            &input.expected,
                            &input.actual,
                            input.score,
                            &input.note,
                        )
                        .await?,
                )
            }
            "runners.list" => Response::ok(id.clone(), backend.runners().await?),
            "runners.put" => {
                let input = params::<RunnerPut>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .putrunner(
                            &input.id,
                            &input.name,
                            &input.endpoint,
                            input.capabilities,
                            input.local,
                        )
                        .await?,
                )
            }
            "runners.heartbeat" => {
                let input = params::<RunnerHeartbeat>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend.heartbeat(&input.id, &input.status).await?,
                )
            }
            "previews.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.previews(&input.runid).await?)
            }
            "previews.put" => {
                let input = params::<PreviewPut>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .putpreview(
                            &input.id,
                            &input.runid,
                            &input.runnerid,
                            &input.url,
                            &input.status,
                            input.expires,
                        )
                        .await?,
                )
            }
            "checkpoints.list" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.checkpoints(&input.runid).await?)
            }
            "checkpoints.create" => {
                let input = params::<CheckpointCreate>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .checkpoint(
                            &input.runid,
                            &input.root,
                            &input.paths,
                            &input.label,
                            input.state,
                        )
                        .await?,
                )
            }
            "checkpoints.restore" => {
                let input = params::<Named>(request.params)?;
                Response::ok(id.clone(), backend.restorecheckpoint(&input.id).await?)
            }
            "workspaces.get" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.workspace(&input.runid).await?)
            }
            "workspaces.create" => {
                let input = params::<WorkspaceCreate>(request.params)?;
                Response::ok(
                    id.clone(),
                    backend
                        .createworkspace(&input.runid, &input.root, input.base.as_deref())
                        .await?,
                )
            }
            "workspaces.merge" => {
                let input = params::<RunNamed>(request.params)?;
                Response::ok(id.clone(), backend.mergeworkspace(&input.runid).await?)
            }
            unknown => anyhow::bail!("unknown backend method `{unknown}`"),
        }
    }
    .await;
    match result {
        Ok(response) => response,
        Err(error) => Response::error(id, error),
    }
}

pub async fn serve(
    backend: &Backend,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<()> {
    loop {
        let mut line = Vec::new();
        let read = (&mut input)
            .take(LINELIMIT as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        if line.len() > LINELIMIT {
            if line.last() != Some(&b'\n') {
                discardline(&mut input)?;
            }
            writeresponse(
                &mut output,
                &Response::error(Value::Null, "request line is too large"),
            )?;
            continue;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let response = match serde_json::from_slice::<Request>(&line) {
            Ok(request) => handle(backend, request).await,
            Err(error) => Response::error(Value::Null, format!("invalid request: {error}")),
        };
        writeresponse(&mut output, &response)?;
    }
    Ok(())
}

fn discardline(input: &mut impl BufRead) -> std::io::Result<()> {
    loop {
        let buffer = input.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }
        let end = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|at| at + 1);
        let consumed = end.unwrap_or(buffer.len());
        input.consume(consumed);
        if end.is_some() {
            return Ok(());
        }
    }
}

fn writeresponse(output: &mut impl Write, response: &Response) -> Result<()> {
    serde_json::to_writer(&mut *output, response)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

fn hex(content: &[u8]) -> String {
    content.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decodehex(content: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(
        content.len().is_multiple_of(2),
        "artifact content is not valid hex"
    );
    (0..content.len())
        .step_by(2)
        .map(|at| {
            u8::from_str_radix(&content[at..at + 2], 16)
                .context("artifact content is not valid hex")
        })
        .collect()
}
