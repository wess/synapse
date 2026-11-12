use crate::backend::{Backend, Decision, EventDraft, EventFilter, RunBundle, RunStatus};
use crate::cli::Outcome;
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;

const USAGE: &str = "usage: synapse backend <summary|events|emit|runs|run|steps|changes|review|comment|artifact|runner|preview|checkpoints|workspace|capability|trigger|route|evaluation|api>";
const INPUTLIMIT: u64 = 1024 * 1024;
const ARTIFACTLIMIT: u64 = 128 * 1024 * 1024;

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = text(arguments, 0, USAGE)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let backend = runtime.block_on(Backend::open(crate::files::database()?))?;
    if action == "api" {
        let input = std::io::BufReader::new(std::io::stdin().lock());
        runtime.block_on(crate::backend::api::serve(
            &backend,
            input,
            std::io::stdout().lock(),
        ))?;
        return Ok(Outcome::Exit(0));
    }
    runtime.block_on(command(&backend, &action, &arguments[1..]))?;
    Ok(Outcome::Exit(0))
}

async fn command(backend: &Backend, action: &str, arguments: &[OsString]) -> Result<()> {
    match action {
        "summary" => printjson(&backend.summary().await?),
        "events" => {
            let filter = EventFilter {
                after: number(arguments, "--after")?.unwrap_or_default(),
                limit: number(arguments, "--limit")?.unwrap_or(100),
                project: value(arguments, "--project"),
                agent: value(arguments, "--agent"),
                source: value(arguments, "--source"),
                level: value(arguments, "--level")
                    .map(|value| value.parse())
                    .transpose()?,
                runid: value(arguments, "--run"),
                query: value(arguments, "--query"),
            };
            printjson(&backend.events(&filter).await?)
        }
        "emit" => {
            let draft: EventDraft = inputjson(INPUTLIMIT)?;
            printjson(&backend.record(draft).await?)
        }
        "runs" => printjson(
            &backend
                .runs(number(arguments, "--limit")?.unwrap_or(100))
                .await?,
        ),
        "run" => runcommand(backend, arguments).await,
        "steps" => printjson(
            &backend
                .steps(&text(arguments, 0, "usage: synapse backend steps <run>")?)
                .await?,
        ),
        "changes" => changes(backend, arguments).await,
        "review" => review(backend, arguments).await,
        "comment" => comment(backend, arguments).await,
        "artifact" => artifact(backend, arguments).await,
        "runner" => runner(backend, arguments).await,
        "preview" => preview(backend, arguments).await,
        "checkpoints" => checkpoints(backend, arguments).await,
        "workspace" => workspace(backend, arguments).await,
        "capability" => capability(backend, arguments).await,
        "trigger" => trigger(backend, arguments).await,
        "route" => route(backend, arguments).await,
        "evaluation" => evaluation(backend, arguments).await,
        unknown => anyhow::bail!("unknown backend command `{unknown}`\n\n{USAGE}"),
    }
}

async fn runcommand(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend run <create|update|fork|replay|export|import|claim|renew|cancel|retry>",
    )?;
    match action.as_str() {
        "create" => {
            let project = required(arguments, "--project")?;
            let harness = required(arguments, "--harness")?;
            let input = optionaljson(INPUTLIMIT)?;
            printjson(&backend.createrun(&project, &harness, input).await?)
        }
        "update" => {
            let id = text(
                arguments,
                1,
                "usage: synapse backend run update <id> <status>",
            )?;
            let status: RunStatus = text(
                arguments,
                2,
                "usage: synapse backend run update <id> <status>",
            )?
            .parse()?;
            let body: RunResult = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .updaterun(&id, status, &body.output, &body.error)
                    .await?,
            )
        }
        "replay" => printjson(
            &backend
                .replay(&text(
                    arguments,
                    1,
                    "usage: synapse backend run replay <id>",
                )?)
                .await?,
        ),
        "fork" => {
            let id = text(
                arguments,
                1,
                "usage: synapse backend run fork <id> [--title <title>]",
            )?;
            printjson(
                &backend
                    .forkrun(&id, &value(arguments, "--title").unwrap_or_default())
                    .await?,
            )
        }
        "export" => printjson(
            &backend
                .exportbundle(&text(
                    arguments,
                    1,
                    "usage: synapse backend run export <id>",
                )?)
                .await?,
        ),
        "import" => {
            let bundle: RunBundle = inputjson(ARTIFACTLIMIT)?;
            printjson(
                &backend
                    .importbundle(bundle, value(arguments, "--project").as_deref())
                    .await?,
            )
        }
        "cancel" => printjson(
            &backend
                .cancel(&text(
                    arguments,
                    1,
                    "usage: synapse backend run cancel <id>",
                )?)
                .await?,
        ),
        "retry" => printjson(
            &backend
                .retry(&text(
                    arguments,
                    1,
                    "usage: synapse backend run retry <id>",
                )?)
                .await?,
        ),
        "claim" => {
            let worker = text(
                arguments,
                1,
                "usage: synapse backend run claim <worker> [--lease <seconds>]",
            )?;
            printjson(
                &backend
                    .claim(&worker, number(arguments, "--lease")?.unwrap_or(60))
                    .await?,
            )
        }
        "renew" => {
            let id = text(
                arguments,
                1,
                "usage: synapse backend run renew <id> <worker> [--lease <seconds>]",
            )?;
            let worker = text(
                arguments,
                2,
                "usage: synapse backend run renew <id> <worker> [--lease <seconds>]",
            )?;
            let renewed = backend
                .renew(&id, &worker, number(arguments, "--lease")?.unwrap_or(60))
                .await?;
            printjson(&json!({ "renewed": renewed }))
        }
        unknown => anyhow::bail!("unknown run command `{unknown}`"),
    }
}

async fn changes(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend changes <list|record> <run>",
    )?;
    let run = text(
        arguments,
        1,
        "usage: synapse backend changes <list|record> <run>",
    )?;
    match action.as_str() {
        "list" => printjson(&backend.changes(&run).await?),
        "record" => {
            let input: ChangeInput = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .recordchange(
                        &run,
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
        unknown => anyhow::bail!("unknown changes command `{unknown}`"),
    }
}

#[derive(serde::Deserialize)]
struct ChangeInput {
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

async fn review(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend review <list|submit> <run>",
    )?;
    let run = text(
        arguments,
        1,
        "usage: synapse backend review <list|submit> <run>",
    )?;
    match action.as_str() {
        "list" => printjson(&backend.reviews(&run).await?),
        "submit" => {
            let input: ReviewInput = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .submitreview(
                        &run,
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
        unknown => anyhow::bail!("unknown review command `{unknown}`"),
    }
}

#[derive(serde::Deserialize)]
struct ReviewInput {
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

async fn comment(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend comment <list|add|resolve> ...",
    )?;
    match action.as_str() {
        "list" => printjson(
            &backend
                .comments(&text(
                    arguments,
                    1,
                    "usage: synapse backend comment list <run>",
                )?)
                .await?,
        ),
        "add" => {
            let run = text(arguments, 1, "usage: synapse backend comment add <run>")?;
            let input: CommentInput = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .comment(
                        &run,
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
        "resolve" | "reopen" => {
            let id = text(
                arguments,
                1,
                "usage: synapse backend comment <resolve|reopen> <id>",
            )?
            .parse()?;
            printjson(&backend.resolvecomment(id, action == "resolve").await?)
        }
        unknown => anyhow::bail!("unknown comment command `{unknown}`"),
    }
}

#[derive(serde::Deserialize)]
struct CommentInput {
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

async fn runner(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend runner <list|put|heartbeat>",
    )?;
    match action.as_str() {
        "list" => printjson(&backend.runners().await?),
        "put" => {
            let id = text(arguments, 1, "usage: synapse backend runner put <id>")?;
            let input: RunnerInput = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .putrunner(
                        &id,
                        &input.name,
                        &input.endpoint,
                        input.capabilities,
                        input.local,
                    )
                    .await?,
            )
        }
        "heartbeat" => {
            let id = text(
                arguments,
                1,
                "usage: synapse backend runner heartbeat <id> <status>",
            )?;
            let status = text(
                arguments,
                2,
                "usage: synapse backend runner heartbeat <id> <status>",
            )?;
            printjson(&backend.heartbeat(&id, &status).await?)
        }
        unknown => anyhow::bail!("unknown runner command `{unknown}`"),
    }
}

#[derive(serde::Deserialize)]
struct RunnerInput {
    name: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    capabilities: Value,
    #[serde(default = "yes")]
    local: bool,
}

async fn preview(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend preview <list|put> <run>",
    )?;
    let run = text(
        arguments,
        1,
        "usage: synapse backend preview <list|put> <run>",
    )?;
    match action.as_str() {
        "list" => printjson(&backend.previews(&run).await?),
        "put" => {
            let input: PreviewInput = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .putpreview(
                        &input.id,
                        &run,
                        &input.runnerid,
                        &input.url,
                        &input.status,
                        input.expires,
                    )
                    .await?,
            )
        }
        unknown => anyhow::bail!("unknown preview command `{unknown}`"),
    }
}

async fn checkpoints(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend checkpoints <list|create|restore> ...",
    )?;
    match action.as_str() {
        "list" => printjson(
            &backend
                .checkpoints(&text(
                    arguments,
                    1,
                    "usage: synapse backend checkpoints list <run>",
                )?)
                .await?,
        ),
        "create" => {
            let run = text(
                arguments,
                1,
                "usage: synapse backend checkpoints create <run> --root <path>",
            )?;
            let root = PathBuf::from(required(arguments, "--root")?);
            let input: CheckpointInput = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .checkpoint(&run, &root, &input.paths, &input.label, input.state)
                    .await?,
            )
        }
        "restore" => printjson(
            &backend
                .restorecheckpoint(&text(
                    arguments,
                    1,
                    "usage: synapse backend checkpoints restore <id>",
                )?)
                .await?,
        ),
        unknown => anyhow::bail!("unknown checkpoints command `{unknown}`"),
    }
}

#[derive(serde::Deserialize)]
struct CheckpointInput {
    paths: Vec<PathBuf>,
    #[serde(default)]
    label: String,
    #[serde(default)]
    state: Value,
}

async fn workspace(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend workspace <get|create|merge> <run>",
    )?;
    let run = text(
        arguments,
        1,
        "usage: synapse backend workspace <get|create|merge> <run>",
    )?;
    match action.as_str() {
        "get" => printjson(&backend.workspace(&run).await?),
        "create" => printjson(
            &backend
                .createworkspace(
                    &run,
                    &PathBuf::from(required(arguments, "--root")?),
                    value(arguments, "--base").as_deref(),
                )
                .await?,
        ),
        "merge" => printjson(&backend.mergeworkspace(&run).await?),
        unknown => anyhow::bail!("unknown workspace command `{unknown}`"),
    }
}

#[derive(serde::Deserialize)]
struct PreviewInput {
    id: String,
    runnerid: String,
    url: String,
    #[serde(default = "starting")]
    status: String,
    #[serde(default)]
    expires: i64,
}

fn pending() -> String {
    "pending".to_owned()
}

fn run_target() -> String {
    "run".to_owned()
}

fn starting() -> String {
    "starting".to_owned()
}

fn yes() -> bool {
    true
}

#[derive(serde::Deserialize)]
struct RunResult {
    #[serde(default)]
    output: String,
    #[serde(default)]
    error: String,
}

async fn artifact(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend artifact <put|attach|list|read>",
    )?;
    match action.as_str() {
        "put" => {
            let name = required(arguments, "--name")?;
            let kind = required(arguments, "--kind")?;
            let media = required(arguments, "--media")?;
            let content = match value(arguments, "--file") {
                Some(path) => readfile(PathBuf::from(path), ARTIFACTLIMIT)?,
                None => readstdin(ARTIFACTLIMIT)?,
            };
            printjson(&backend.putartifact(&name, &kind, &media, &content).await?)
        }
        "attach" => {
            let run = text(
                arguments,
                1,
                "usage: synapse backend artifact attach <run> <digest> <role>",
            )?;
            let digest = text(
                arguments,
                2,
                "usage: synapse backend artifact attach <run> <digest> <role>",
            )?;
            let role = text(
                arguments,
                3,
                "usage: synapse backend artifact attach <run> <digest> <role>",
            )?;
            backend.attach(&run, &digest, &role).await?;
            printjson(&json!({ "attached": true }))
        }
        "list" => printjson(
            &backend
                .artifacts(&text(
                    arguments,
                    1,
                    "usage: synapse backend artifact list <run>",
                )?)
                .await?,
        ),
        "read" => {
            let content = backend
                .readartifact(&text(
                    arguments,
                    1,
                    "usage: synapse backend artifact read <digest>",
                )?)
                .await?;
            std::io::stdout()
                .lock()
                .write_all(&content)
                .context("could not write artifact")
        }
        unknown => anyhow::bail!("unknown artifact command `{unknown}`"),
    }
}

async fn capability(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend capability <list|allow|deny|check>",
    )?;
    match action.as_str() {
        "list" => printjson(&backend.capabilities().await?),
        "allow" | "deny" => {
            let subject = text(
                arguments,
                1,
                "usage: synapse backend capability <allow|deny> <subject> <kind> <resource> [--digest <digest>]",
            )?;
            let kind = text(
                arguments,
                2,
                "usage: synapse backend capability <allow|deny> <subject> <kind> <resource> [--digest <digest>]",
            )?;
            let resource = text(
                arguments,
                3,
                "usage: synapse backend capability <allow|deny> <subject> <kind> <resource> [--digest <digest>]",
            )?;
            let decision = if action == "allow" {
                Decision::Allow
            } else {
                Decision::Deny
            };
            printjson(
                &backend
                    .decide(
                        &subject,
                        &kind,
                        &resource,
                        decision,
                        &value(arguments, "--digest").unwrap_or_default(),
                    )
                    .await?,
            )
        }
        "check" => {
            let subject = text(
                arguments,
                1,
                "usage: synapse backend capability check <subject> <kind> <resource> [--digest <digest>]",
            )?;
            let kind = text(
                arguments,
                2,
                "usage: synapse backend capability check <subject> <kind> <resource> [--digest <digest>]",
            )?;
            let resource = text(
                arguments,
                3,
                "usage: synapse backend capability check <subject> <kind> <resource> [--digest <digest>]",
            )?;
            let allowed = backend
                .allowed(
                    &subject,
                    &kind,
                    &resource,
                    &value(arguments, "--digest").unwrap_or_default(),
                )
                .await?;
            printjson(&json!({ "allowed": allowed }))
        }
        unknown => anyhow::bail!("unknown capability command `{unknown}`"),
    }
}

async fn trigger(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend trigger <list|put|due|mark|tick>",
    )?;
    match action.as_str() {
        "list" => printjson(&backend.triggers().await?),
        "put" => {
            let name = text(
                arguments,
                1,
                "usage: synapse backend trigger put <name> <schedule|event|dependency> [--next <time>]",
            )?;
            let kind = text(
                arguments,
                2,
                "usage: synapse backend trigger put <name> <schedule|event|dependency> [--next <time>]",
            )?;
            printjson(
                &backend
                    .puttrigger(
                        &name,
                        &kind,
                        optionaljson(INPUTLIMIT)?,
                        number(arguments, "--next")?.unwrap_or_default(),
                    )
                    .await?,
            )
        }
        "due" => printjson(
            &backend
                .due(number(arguments, "--time")?.unwrap_or_else(now))
                .await?,
        ),
        "mark" => {
            let name = text(
                arguments,
                1,
                "usage: synapse backend trigger mark <name> <next>",
            )?;
            let next = text(
                arguments,
                2,
                "usage: synapse backend trigger mark <name> <next>",
            )?
            .parse()?;
            backend.marktrigger(&name, next).await?;
            printjson(&json!({ "marked": true }))
        }
        "tick" => printjson(
            &backend
                .tick(number(arguments, "--time")?.unwrap_or_else(now))
                .await?,
        ),
        unknown => anyhow::bail!("unknown trigger command `{unknown}`"),
    }
}

async fn route(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(
        arguments,
        0,
        "usage: synapse backend route <list|put|select|call>",
    )?;
    match action.as_str() {
        "list" => printjson(&backend.routes().await?),
        "put" => {
            let name = text(
                arguments,
                1,
                "usage: synapse backend route put <name> <endpoint> <model> [--priority <n>] <--local|--remote>",
            )?;
            let endpoint = text(
                arguments,
                2,
                "usage: synapse backend route put <name> <endpoint> <model> [--priority <n>] <--local|--remote>",
            )?;
            let model = text(
                arguments,
                3,
                "usage: synapse backend route put <name> <endpoint> <model> [--priority <n>] <--local|--remote>",
            )?;
            let local = arguments.iter().any(|value| value == "--local");
            anyhow::ensure!(
                local || arguments.iter().any(|value| value == "--remote"),
                "route requires --local or --remote"
            );
            printjson(
                &backend
                    .putroute(
                        &name,
                        &endpoint,
                        &model,
                        number(arguments, "--priority")?.unwrap_or_default(),
                        local,
                    )
                    .await?,
            )
        }
        "select" => printjson(&backend.selectroute(localfilter(arguments)?).await?),
        "call" => printjson(
            &backend
                .callroute(localfilter(arguments)?, inputjson(INPUTLIMIT)?)
                .await?,
        ),
        unknown => anyhow::bail!("unknown route command `{unknown}`"),
    }
}

async fn evaluation(backend: &Backend, arguments: &[OsString]) -> Result<()> {
    let action = text(arguments, 0, "usage: synapse backend evaluation <list|add>")?;
    match action.as_str() {
        "list" => printjson(
            &backend
                .evaluations(&text(
                    arguments,
                    1,
                    "usage: synapse backend evaluation list <run>",
                )?)
                .await?,
        ),
        "add" => {
            let run = text(
                arguments,
                1,
                "usage: synapse backend evaluation add <run> <score>",
            )?;
            let score = text(
                arguments,
                2,
                "usage: synapse backend evaluation add <run> <score>",
            )?
            .parse()?;
            let input: EvaluationInput = inputjson(INPUTLIMIT)?;
            printjson(
                &backend
                    .evaluate(&run, &input.expected, &input.actual, score, &input.note)
                    .await?,
            )
        }
        unknown => anyhow::bail!("unknown evaluation command `{unknown}`"),
    }
}

#[derive(serde::Deserialize)]
struct EvaluationInput {
    expected: String,
    actual: String,
    #[serde(default)]
    note: String,
}

fn printjson(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn inputjson<T: DeserializeOwned>(limit: u64) -> Result<T> {
    serde_json::from_slice(&readstdin(limit)?).context("stdin is not valid JSON")
}

fn optionaljson(limit: u64) -> Result<Value> {
    let input = readstdin(limit)?;
    if input.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&input).context("stdin is not valid JSON")
}

fn readstdin(limit: u64) -> Result<Vec<u8>> {
    let mut content = Vec::new();
    std::io::stdin()
        .lock()
        .take(limit + 1)
        .read_to_end(&mut content)?;
    anyhow::ensure!(content.len() as u64 <= limit, "stdin is too large");
    Ok(content)
}

fn readfile(path: PathBuf, limit: u64) -> Result<Vec<u8>> {
    let metadata =
        std::fs::metadata(&path).with_context(|| format!("could not read {}", path.display()))?;
    anyhow::ensure!(metadata.len() <= limit, "file is too large");
    std::fs::read(&path).with_context(|| format!("could not read {}", path.display()))
}

fn text(arguments: &[OsString], at: usize, usage: &str) -> Result<String> {
    arguments
        .get(at)
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .context(usage.to_owned())
}

fn value(arguments: &[OsString], flag: &str) -> Option<String> {
    arguments
        .iter()
        .position(|value| value == flag)
        .and_then(|at| arguments.get(at + 1))
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
}

fn required(arguments: &[OsString], flag: &str) -> Result<String> {
    value(arguments, flag).with_context(|| format!("{flag} requires a value"))
}

fn number(arguments: &[OsString], flag: &str) -> Result<Option<i64>> {
    value(arguments, flag)
        .map(|value| {
            value
                .parse()
                .with_context(|| format!("{flag} requires an integer"))
        })
        .transpose()
}

fn localfilter(arguments: &[OsString]) -> Result<Option<bool>> {
    let local = arguments.iter().any(|value| value == "--local");
    let remote = arguments.iter().any(|value| value == "--remote");
    anyhow::ensure!(!(local && remote), "choose either --local or --remote");
    Ok(if local {
        Some(true)
    } else if remote {
        Some(false)
    } else {
        None
    })
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
