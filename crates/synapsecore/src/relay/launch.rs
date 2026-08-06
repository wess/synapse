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

    let mut environment = vec![(
        "SYNAPSE_PROJECT_DIR".to_owned(),
        options.root.display().to_string(),
    )];
    // The tool we resolved may be a version manager's shim, and a shim execs its
    // manager by name. The desktop app inherits the Finder's four-entry PATH, so
    // without this the child starts and immediately cannot find `asdf`.
    if let Some(path) = crate::agent::searchpath().and_then(|path| path.into_string().ok()) {
        environment.push(("PATH".to_owned(), path));
    }

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

    let agent = resolveagent(&name)?;
    let detection = crate::agent::detect(&agent, Some(&binary()?));
    let program = detection
        .executable
        .clone()
        .with_context(|| format!("{} is not installed or is not on PATH", agent.name))?;

    // An already-connected tool loads Synapse from its own user configuration.
    // Adding a second copy would give the agent two of every tool, so what
    // follows is written only for a tool that has no connection of its own yet.
    //
    // For a package-style connection that means *registered* rather than
    // *configured*: a package installed from anywhere carries the same tools.
    // Elsewhere a registration that is not configured is a stale entry pointing
    // at a binary that may no longer exist, which is not a connection.
    let connected = match agent.detect.style {
        crate::agent::tool::Style::Package => detection.registered,
        crate::agent::tool::Style::Server => detection.configured,
    };

    // What the tool is handed so it can reach Synapse for the life of this
    // process: a generated MCP config pointing at this binary, or for pi the
    // extension that carries the same tools. A tool whose descriptor declares no
    // config slot is never handed either, and nothing is written for it.
    let payload = if connected || agent.launch.config.is_empty() {
        None
    } else if agent.kind == Kind::Pi {
        Some(super::extension::write()?)
    } else {
        let path = configpath(options.name, options.root)?;
        writeconfig(&path, options.root)?;
        Some(path)
    };

    let mut launch = fromslots(
        &agent,
        &program,
        prompt.as_deref(),
        payload.as_deref(),
        options,
        &allowed,
        model.as_deref(),
    )?;
    launch.arguments.extend(options.extra.iter().cloned());
    launch.environment = environment;
    // Claude Code takes a session id for claiming and a separate flag for
    // resuming, so a resumable worker's id is carried on the launch rather than
    // in the argv. A tool that takes one flag for both puts `{session}` in its
    // own headless slot instead.
    launch.session = (options.headless && agent.kind == Kind::Claude).then(sessionid);
    Ok(launch)
}

/// Fill the descriptor's argv slots.
///
/// Every slot is optional: one a tool does not declare is a flag it does not
/// have, and is simply never passed. That is what lets a tool nobody has heard
/// of launch through the same path as the three that ship.
fn fromslots(
    agent: &Agent,
    program: &Path,
    prompt: Option<&str>,
    payload: Option<&Path>,
    options: &Options,
    allowed: &[String],
    model: Option<&str>,
) -> Result<Launch> {
    let slots = &agent.launch;
    let server = binary()?.display().to_string();
    let payload = payload.map(|path| path.display().to_string());
    let session = sessionid();
    let fill = |template: &[String]| -> Vec<String> {
        template
            .iter()
            .filter(|token| prompt.is_some() || *token != "{prompt}")
            .flat_map(|token| {
                // The variadic slot: one argv token per rule, so a rule with
                // spaces such as `Bash(git commit:*)` stays one argument.
                if token == "{tools...}" {
                    return allowed.to_vec();
                }
                vec![
                    token
                        .replace("{prompt}", prompt.unwrap_or_default())
                        .replace("{config}", payload.as_deref().unwrap_or_default())
                        .replace("{server}", &server)
                        .replace("{model}", model.unwrap_or_default())
                        .replace("{name}", options.name.unwrap_or_default())
                        .replace("{session}", &session),
                ]
            })
            .collect()
    };

    let mut arguments = Vec::new();
    // A headless run needs something to do, so its harness is the argument that
    // starts it. No harness at all and the tool opens on an empty prompt,
    // exactly as it would if the person had typed its name.
    if options.headless && !slots.headless.is_empty() {
        arguments.extend(fill(&slots.headless));
    } else if prompt.is_some() {
        arguments.extend(fill(&slots.prompt));
    }
    // A headless worker has no terminal to prompt in, and an unattended session
    // has nobody to answer.
    if options.skippermissions || options.headless {
        arguments.extend(fill(&slots.skippermissions));
    }
    if payload.is_some() {
        arguments.extend(fill(&slots.config));
    }
    if options.strict {
        arguments.extend(fill(&slots.strict));
    }
    if !allowed.is_empty() {
        arguments.extend(fill(&slots.allowedtools));
    }
    if model.is_some() {
        arguments.extend(fill(&slots.model));
    }
    Ok(Launch {
        program: program.to_path_buf(),
        arguments,
        environment: Vec::new(),
        tool: agent.slug.clone(),
        session: None,
    })
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

/// The tool a `--tool` value or a role's `tool` names. Matched on the descriptor
/// slug first and then the binary, so `hermes` finds a tool nobody shipped.
fn resolveagent(name: &str) -> Result<Agent> {
    let home = crate::files::home()?;
    let agents = crate::agent::agents(&home);
    agents
        .iter()
        .find(|agent| agent.slug == name)
        .or_else(|| agents.iter().find(|agent| agent.command == name))
        .cloned()
        .with_context(|| {
            let known: Vec<_> = agents.iter().map(|agent| agent.slug.as_str()).collect();
            format!(
                "unknown tool `{name}`; this machine has {}. Describe another with `synapse tool create`",
                known.join(", ")
            )
        })
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

    /// Build a tool's argv the way a launch does, through its descriptor. Every
    /// argv assertion below goes through this, so a descriptor that stops
    /// producing the flags a tool actually takes fails here.
    fn argv(
        slug: &str,
        prompt: Option<&str>,
        payload: Option<&Path>,
        options: &Options,
        allowed: &[String],
        model: Option<&str>,
    ) -> Launch {
        let home = Path::new("/users/test");
        let agent = crate::agent::tool::resolve(home, None, slug)
            .unwrap()
            .unwrap();
        fromslots(
            &agent,
            Path::new("/usr/bin").join(slug).as_path(),
            prompt,
            payload,
            options,
            allowed,
            model,
        )
        .unwrap()
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
        assert!(error.contains("claude, codex, pi"), "got {error}");
        // A tool nobody shipped is a descriptor away, and the error says so
        // rather than reading like a closed list.
        assert!(error.contains("synapse tool create"), "got {error}");
    }

    #[test]
    fn a_headless_claude_can_never_stop_to_ask() {
        let launch = argv(
            "claude",
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
        let launch = argv(
            "claude",
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
        let launch = argv(
            "claude",
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
        let launch = argv(
            "claude",
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
        let launch = argv(
            "claude",
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
        let launch = argv(
            "codex",
            Some("hi"),
            None,
            &Options {
                headless: true,
                ..options(Path::new("/tmp"), "codex")
            },
            &[],
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
        let launch = argv(
            "pi",
            Some("hi"),
            None,
            &Options {
                headless: true,
                ..options(Path::new("/tmp"), "pi")
            },
            &[],
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
        let launch = argv(
            "pi",
            Some("hi"),
            None,
            &options(Path::new("/tmp"), "pi"),
            &[],
            None,
        );
        assert!(!launch.arguments.iter().any(|item| item == "--approve"));
        assert!(!launch.arguments.iter().any(|item| item == "--mode"));
        assert!(!launch.arguments.iter().any(|item| item == "--session-id"));
    }

    #[test]
    fn an_unconnected_pi_is_handed_the_extension_for_this_run_only() {
        let launch = argv(
            "pi",
            Some("hi"),
            Some(Path::new("/data/relay/pi/synapse/index.ts")),
            &options(Path::new("/tmp"), "pi"),
            &[],
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
