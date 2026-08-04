//! The one pipeline that turns "launch agent X as role Y" into a command:
//! resolve the role, merge its channels and tool grants, render the harness, and
//! build the argv. Shared by `synapse relay launch` and the `spawn` MCP tool, so
//! the two cannot drift.
//!
//! Unlike a mesh that runs over HTTP, nothing here needs a port, a token, or a
//! generated credential: the agent reaches the mesh through the same `synapse
//! mcp` stdio server it already uses for memory. A tool that Synapse is already
//! connected to is launched as-is; one that is not is handed what it needs for
//! the life of the process — a config pointing at this binary, or for pi the
//! extension that carries the same tools — so a launch works before setup has
//! ever been run, and leaves the machine as it found it.

use crate::agent::{Agent, Kind};
use crate::relay::{harness, role};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Options<'a> {
    /// The name the agent joins the mesh under. `None` launches the tool
    /// wired into Synapse but outside the mesh — no harness prompt, no
    /// registration — which is what `synapse launch` wants and what a plain
    /// interactive session is.
    pub name: Option<&'a str>,
    pub role: &'a str,
    /// Project root the agent works in. The role's project layer resolves here,
    /// and the child is told about it through `SYNAPSE_PROJECT_DIR` so its
    /// memory and mesh registration are scoped to the right checkout.
    pub root: &'a Path,
    /// `claude`, `codex`, or `pi`; falls back to the role's tool, then Claude Code.
    pub tool: Option<&'a str>,
    /// Per-launch focus, distinct from the role's durable brief.
    pub task: Option<&'a str>,
    /// Channels to join, merged with the role's.
    pub channels: &'a [String],
    /// Extra pre-granted tool rules, merged with the role's.
    pub tools: &'a [String],
    pub model: Option<&'a str>,
    /// Launch as the human-driven lead. A `driver` role implies it.
    pub lead: bool,
    pub optimize: bool,
    /// A supervised background worker rather than an interactive session.
    pub headless: bool,
    /// Bypass the tool's own permission prompts. Required for a headless worker,
    /// which has no terminal to prompt in; opt-in otherwise.
    pub skippermissions: bool,
    /// Load only the Synapse MCP server, ignoring project and user servers.
    pub strict: bool,
    /// Custom launch template. Placeholders: {prompt} {config} {name}.
    pub command: Option<&'a str>,
    /// Flags appended verbatim to the tool's own argv.
    pub extra: &'a [String],
}

#[derive(Clone, Debug)]
pub struct Launch {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    /// The tool the options resolved to.
    pub tool: String,
    /// A fixed session id for a resumable headless Claude Code worker, so its
    /// context survives a crash. Absent for an interactive launch, and absent
    /// for pi, which takes one flag for both claiming and resuming a session and
    /// so carries it in the argv itself.
    pub session: Option<String>,
}

/// Resolve `options` into a launchable command.
pub fn launch(options: &Options) -> Result<Launch> {
    // The name the agent will join the mesh under is also the name of the
    // config file written for it, and a model chooses it. Refuse it here, where
    // it first becomes a path, rather than after something has been written.
    if let Some(name) = options.name {
        crate::relay::store::validname(name)?;
    }
    let role = role::resolve(Some(options.root), options.role)?;
    let brief = role
        .as_ref()
        .map(|role| role.description.clone())
        .unwrap_or_default();

    let mut channels = role
        .as_ref()
        .map(|role| role.channels.clone())
        .unwrap_or_default();
    for channel in options.channels {
        if !channels.contains(channel) {
            channels.push(channel.clone());
        }
    }
    let mut allowed = role
        .as_ref()
        .map(|role| role.tools.clone())
        .unwrap_or_default();
    for rule in options.tools {
        if !allowed.contains(rule) {
            allowed.push(rule.clone());
        }
    }
    let name = options
        .tool
        .map(str::to_owned)
        .or_else(|| role.as_ref().and_then(|role| role.tool.clone()))
        .unwrap_or_else(|| "claude".to_owned());
    let model = options
        .model
        .map(str::to_owned)
        .or_else(|| role.as_ref().and_then(|role| role.model.clone()));
    let interactive =
        !options.headless && (options.lead || role.as_ref().is_some_and(|role| role.driver));

    // A launch with no mesh name gets no harness: it is a person's own session,
    // and the opening turn belongs to them, not to a protocol.
    let prompt = options.name.map(|name| {
        harness::prompt(
            name,
            options.role,
            &brief,
            &channels,
            options.task,
            interactive,
            options.optimize,
        )
    });

    let environment = vec![(
        "SYNAPSE_PROJECT_DIR".to_owned(),
        options.root.display().to_string(),
    )];

    if let Some(template) = options.command {
        let config = configpath(options.name, options.root)?;
        writeconfig(&config, options.root)?;
        let mut launch = fromtemplate(
            template,
            prompt.as_deref().unwrap_or_default(),
            &config,
            options.name.unwrap_or_default(),
        );
        launch.arguments.extend(options.extra.iter().cloned());
        launch.environment = environment;
        return Ok(launch);
    }

    let kind = match name.as_str() {
        "claude" => Kind::Claude,
        "codex" => Kind::Codex,
        "pi" => Kind::Pi,
        other => anyhow::bail!(
            "unknown tool `{other}`; use claude, codex, or pi, or pass a template with --command"
        ),
    };
    let agent = resolveagent(kind)?;
    let detection = crate::agent::detect(&agent, Some(&binary()?));
    let program = detection
        .executable
        .clone()
        .with_context(|| format!("{} is not installed or is not on PATH", agent.name))?;

    // An already-connected tool loads Synapse from its own user configuration.
    // Adding a second copy would give the agent two of every tool, so what
    // follows is written only for a tool that has no connection of its own yet.
    //
    // For pi that means *registered* rather than *configured*: its connection
    // is a package, and a package installed from anywhere carries the same
    // tools. Elsewhere a registration that is not configured is a stale entry
    // pointing at a binary that may no longer exist, which is not a connection.
    let connected = match kind {
        Kind::Pi => detection.registered,
        _ => detection.configured,
    };

    let mut launch = match kind {
        Kind::Claude | Kind::Codex => {
            let config = if connected {
                None
            } else {
                let path = configpath(options.name, options.root)?;
                writeconfig(&path, options.root)?;
                Some(path)
            };
            match kind {
                Kind::Codex => codex(
                    &program,
                    prompt.as_deref(),
                    config.as_deref(),
                    options,
                    model.as_deref(),
                ),
                _ => claude(
                    &program,
                    prompt.as_deref(),
                    config.as_deref(),
                    options,
                    &allowed,
                    model.as_deref(),
                ),
            }
        }
        Kind::Pi => {
            let extension = if connected {
                None
            } else {
                Some(super::extension::write()?)
            };
            pi(
                &program,
                prompt.as_deref(),
                extension.as_deref(),
                options,
                model.as_deref(),
            )
        }
    };
    launch.arguments.extend(options.extra.iter().cloned());
    launch.environment = environment;
    launch.session = (options.headless && kind == Kind::Claude).then(sessionid);
    Ok(launch)
}

fn claude(
    program: &Path,
    prompt: Option<&str>,
    config: Option<&Path>,
    options: &Options,
    allowed: &[String],
    model: Option<&str>,
) -> Launch {
    let mut arguments = Vec::new();
    match prompt {
        // A headless run needs something to do, so its harness is the argument
        // that starts it.
        Some(prompt) if options.headless => arguments.extend([
            "-p".to_owned(),
            prompt.to_owned(),
            "--output-format".to_owned(),
            "stream-json".to_owned(),
            "--verbose".to_owned(),
        ]),
        Some(prompt) => arguments.push(prompt.to_owned()),
        // No harness: the tool opens on an empty prompt, exactly as it would
        // if the person had typed its name.
        None => {}
    }
    // A headless worker has no terminal to prompt in, and an unattended session
    // has nobody to answer.
    if options.skippermissions || options.headless {
        arguments.push("--dangerously-skip-permissions".to_owned());
    }
    if let Some(config) = config {
        arguments.push("--mcp-config".to_owned());
        arguments.push(config.display().to_string());
    }
    // `--mcp-config` is additive: without this the agent keeps its project and
    // user MCP servers alongside Synapse, which is what you want unless you
    // explicitly asked for a hermetic worker.
    if options.strict {
        arguments.push("--strict-mcp-config".to_owned());
    }
    // `--allowedTools` is variadic; one argv token per rule keeps a rule with
    // spaces such as `Bash(git commit:*)` intact.
    if !allowed.is_empty() {
        arguments.push("--allowedTools".to_owned());
        arguments.extend(allowed.iter().cloned());
    }
    if let Some(model) = model {
        arguments.extend(["--model".to_owned(), model.to_owned()]);
    }
    Launch {
        program: program.to_path_buf(),
        arguments,
        environment: Vec::new(),
        tool: "claude".to_owned(),
        session: None,
    }
}

fn codex(
    program: &Path,
    prompt: Option<&str>,
    config: Option<&Path>,
    options: &Options,
    model: Option<&str>,
) -> Launch {
    let mut arguments = Vec::new();
    if options.headless {
        arguments.push("exec".to_owned());
    }
    if let Some(prompt) = prompt {
        arguments.push(prompt.to_owned());
    }
    if options.skippermissions || options.headless {
        arguments.extend(["-c".to_owned(), "approval_policy=\"never\"".to_owned()]);
    }
    // Codex reads no MCP config file, so an unconnected Codex is pointed at this
    // binary through its own settings syntax instead.
    if config.is_some()
        && let Ok(binary) = binary()
    {
        arguments.extend([
            "-c".to_owned(),
            format!("mcp_servers.synapse.command=\"{}\"", binary.display()),
            "-c".to_owned(),
            "mcp_servers.synapse.args=[\"mcp\"]".to_owned(),
        ]);
    }
    if let Some(model) = model {
        arguments.extend(["-c".to_owned(), format!("model=\"{model}\"")]);
    }
    Launch {
        program: program.to_path_buf(),
        arguments,
        environment: Vec::new(),
        tool: "codex".to_owned(),
        session: None,
    }
}

/// pi's argv.
///
/// Two things it does differently. Its tools arrive in an extension rather than
/// from an MCP client, so `--extension` is what `--mcp-config` is elsewhere.
/// And it has no permission prompts to skip — the question it does stop to ask
/// is whether a project's local settings are trusted, which an unattended
/// worker has nobody to answer.
fn pi(
    program: &Path,
    prompt: Option<&str>,
    extension: Option<&Path>,
    options: &Options,
    model: Option<&str>,
) -> Launch {
    let mut arguments = Vec::new();
    if let Some(prompt) = prompt {
        arguments.push(prompt.to_owned());
    }
    if options.headless {
        // The event stream, so a worker's log is readable without a terminal.
        arguments.extend(["--mode".to_owned(), "json".to_owned()]);
        // pi opens the session with this id when one exists and creates it with
        // that id when it does not, so a respawned worker comes back to its own
        // context without a second flag anywhere to remember it by.
        arguments.extend(["--session-id".to_owned(), sessionid()]);
    }
    if options.skippermissions || options.headless {
        arguments.push("--approve".to_owned());
    }
    if let Some(extension) = extension {
        arguments.extend(["--extension".to_owned(), extension.display().to_string()]);
    }
    if let Some(model) = model {
        arguments.extend(["--model".to_owned(), model.to_owned()]);
    }
    Launch {
        program: program.to_path_buf(),
        arguments,
        environment: Vec::new(),
        tool: "pi".to_owned(),
        session: None,
    }
}

/// Split a template into argv, substituting per token so `{prompt}` stays one
/// argument even though it contains spaces.
fn fromtemplate(template: &str, prompt: &str, config: &Path, name: &str) -> Launch {
    let config = config.display().to_string();
    let mut tokens = template.split_whitespace().map(|token| {
        token
            .replace("{prompt}", prompt)
            .replace("{config}", &config)
            .replace("{name}", name)
    });
    let program = PathBuf::from(tokens.next().unwrap_or_default());
    Launch {
        program,
        arguments: tokens.collect(),
        environment: Vec::new(),
        tool: "custom".to_owned(),
        session: None,
    }
}

fn resolveagent(kind: Kind) -> Result<Agent> {
    let home = crate::files::home()?;
    crate::agent::agents(&home)
        .into_iter()
        .find(|agent| agent.kind == kind)
        .context("that tool is not in the Synapse registry")
}

fn binary() -> Result<PathBuf> {
    std::env::current_exe().context("could not locate the running Synapse executable")
}

/// Where the generated MCP config for this launch is written.
///
/// A mesh agent keys it on its own name, which is already unique and already
/// checked. A launch outside the mesh has no name, so it keys on a digest of the
/// project root: two `synapse launch` runs in one folder reuse one file, and two
/// in different folders do not race to overwrite each other's.
fn configpath(name: Option<&str>, root: &Path) -> Result<PathBuf> {
    let key = match name {
        Some(name) => name.to_owned(),
        None => {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(root.display().to_string().as_bytes());
            let hex: String = digest[..6]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            format!("launch.{hex}")
        }
    };
    Ok(crate::relay::directory()?.join(format!("{key}.mcp.json")))
}

/// The MCP config handed to a tool that Synapse is not connected to yet: this
/// binary, speaking the same stdio server every other connection uses.
fn writeconfig(path: &Path, root: &Path) -> Result<()> {
    let value = serde_json::json!({
        "mcpServers": {
            "synapse": {
                "command": binary()?,
                "args": ["mcp"],
                "env": { "SYNAPSE_PROJECT_DIR": root.display().to_string() }
            }
        }
    });
    crate::files::write(
        path,
        &format!("{}\n", serde_json::to_string_pretty(&value)?),
    )
}

/// A version-4 shaped identifier for a resumable worker session. Derived from
/// the clock, this process, and a per-process counter rather than a random
/// source, which is enough for a local session label that only has to be unique.
fn sessionid() -> String {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(nanos.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.map(|byte| format!("{byte:02x}")).join("");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options<'a>(root: &'a Path, tool: &'a str) -> Options<'a> {
        Options {
            name: Some("backend"),
            role: "worker",
            root,
            tool: Some(tool),
            task: None,
            channels: &[],
            tools: &[],
            model: None,
            lead: false,
            optimize: false,
            headless: false,
            skippermissions: false,
            strict: false,
            command: None,
            extra: &[],
        }
    }

    #[test]
    fn a_template_keeps_the_prompt_as_one_argument() {
        let launch = fromtemplate(
            "mytool --config {config} --prompt {prompt}",
            "two words here",
            Path::new("/tmp/a.json"),
            "backend",
        );
        assert_eq!(launch.program, PathBuf::from("mytool"));
        assert_eq!(launch.arguments.last().unwrap(), "two words here");
        assert!(launch.arguments.contains(&"/tmp/a.json".to_owned()));
    }

    #[test]
    fn an_unknown_tool_is_refused_with_the_alternatives() {
        let directory = tempfile::tempdir().unwrap();
        let mut options = options(directory.path(), "gemini");
        options.tool = Some("gemini");
        let error = launch(&options).unwrap_err().to_string();
        assert!(error.contains("claude, codex, or pi"), "got {error}");
    }

    #[test]
    fn a_headless_claude_can_never_stop_to_ask() {
        let launch = claude(
            Path::new("/usr/bin/claude"),
            Some("hi"),
            None,
            &Options {
                headless: true,
                ..options(Path::new("/tmp"), "claude")
            },
            &[],
            None,
        );
        assert_eq!(launch.arguments[0], "-p");
        assert!(
            launch
                .arguments
                .contains(&"--dangerously-skip-permissions".to_owned())
        );
    }

    #[test]
    fn an_attended_session_keeps_its_permission_prompts() {
        let launch = claude(
            Path::new("/usr/bin/claude"),
            Some("hi"),
            None,
            &options(Path::new("/tmp"), "claude"),
            &[],
            None,
        );
        assert!(!launch.arguments.iter().any(|item| item == "-p"));
        assert!(
            !launch
                .arguments
                .iter()
                .any(|item| item == "--dangerously-skip-permissions")
        );
    }

    #[test]
    fn tool_rules_stay_one_argument_each() {
        let allowed = ["Read".to_owned(), "Bash(git commit:*)".to_owned()];
        let launch = claude(
            Path::new("/usr/bin/claude"),
            Some("hi"),
            None,
            &options(Path::new("/tmp"), "claude"),
            &allowed,
            Some("claude-opus-5"),
        );
        let at = launch
            .arguments
            .iter()
            .position(|item| item == "--allowedTools")
            .unwrap();
        assert_eq!(launch.arguments[at + 1], "Read");
        assert_eq!(launch.arguments[at + 2], "Bash(git commit:*)");
        assert_eq!(launch.arguments[at + 3], "--model");
    }

    #[test]
    fn a_connected_tool_is_not_handed_a_second_server_of_its_own() {
        let launch = claude(
            Path::new("/usr/bin/claude"),
            Some("hi"),
            None,
            &options(Path::new("/tmp"), "claude"),
            &[],
            None,
        );
        assert!(
            !launch.arguments.iter().any(|item| item == "--mcp-config"),
            "a configured tool must not be given a duplicate synapse server"
        );
    }

    #[test]
    fn an_unconnected_tool_is_pointed_at_this_binary() {
        let launch = claude(
            Path::new("/usr/bin/claude"),
            Some("hi"),
            Some(Path::new("/tmp/backend.mcp.json")),
            &options(Path::new("/tmp"), "claude"),
            &[],
            None,
        );
        let at = launch
            .arguments
            .iter()
            .position(|item| item == "--mcp-config")
            .unwrap();
        assert_eq!(launch.arguments[at + 1], "/tmp/backend.mcp.json");
        assert!(
            !launch
                .arguments
                .iter()
                .any(|item| item == "--strict-mcp-config"),
            "the generated config is additive unless strict was asked for"
        );
    }

    #[test]
    fn a_headless_codex_never_approves_its_own_actions() {
        let launch = codex(
            Path::new("/usr/bin/codex"),
            Some("hi"),
            None,
            &Options {
                headless: true,
                ..options(Path::new("/tmp"), "codex")
            },
            None,
        );
        assert_eq!(launch.arguments[0], "exec");
        assert!(
            launch
                .arguments
                .iter()
                .any(|item| item == "approval_policy=\"never\"")
        );
    }

    #[test]
    fn a_headless_pi_streams_events_and_comes_back_to_its_own_session() {
        let launch = pi(
            Path::new("/usr/bin/pi"),
            Some("hi"),
            None,
            &Options {
                headless: true,
                ..options(Path::new("/tmp"), "pi")
            },
            None,
        );
        assert_eq!(launch.arguments[0], "hi");
        let at = launch
            .arguments
            .iter()
            .position(|item| item == "--mode")
            .unwrap();
        assert_eq!(launch.arguments[at + 1], "json");
        let at = launch
            .arguments
            .iter()
            .position(|item| item == "--session-id")
            .unwrap();
        assert_eq!(launch.arguments[at + 1].len(), 36);
        // One flag does both jobs, so nothing outside this argv has to remember
        // the id in order to bring the worker back.
        assert!(launch.session.is_none());
        assert!(launch.arguments.iter().any(|item| item == "--approve"));
    }

    #[test]
    fn an_attended_pi_is_not_approved_on_its_behalf() {
        let launch = pi(
            Path::new("/usr/bin/pi"),
            Some("hi"),
            None,
            &options(Path::new("/tmp"), "pi"),
            None,
        );
        assert!(!launch.arguments.iter().any(|item| item == "--approve"));
        assert!(!launch.arguments.iter().any(|item| item == "--mode"));
        assert!(!launch.arguments.iter().any(|item| item == "--session-id"));
    }

    #[test]
    fn an_unconnected_pi_is_handed_the_extension_for_this_run_only() {
        let launch = pi(
            Path::new("/usr/bin/pi"),
            Some("hi"),
            Some(Path::new("/data/relay/pi/synapse/index.ts")),
            &options(Path::new("/tmp"), "pi"),
            Some("sonnet"),
        );
        let at = launch
            .arguments
            .iter()
            .position(|item| item == "--extension")
            .unwrap();
        assert_eq!(launch.arguments[at + 1], "/data/relay/pi/synapse/index.ts");
        let at = launch
            .arguments
            .iter()
            .position(|item| item == "--model")
            .unwrap();
        assert_eq!(launch.arguments[at + 1], "sonnet");
    }

    #[test]
    fn session_identifiers_are_unique_and_correctly_shaped() {
        let first = sessionid();
        let second = sessionid();
        assert_ne!(first, second);
        assert_eq!(first.len(), 36);
        let parts: Vec<&str> = first.split('-').collect();
        assert_eq!(
            parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'), "expected a version 4 shape");
        assert!(
            first
                .chars()
                .all(|item| item.is_ascii_hexdigit() || item == '-')
        );
    }
}
