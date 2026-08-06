//! `synapse launch <tool> [-- <tool flags>]`.
//!
//! The front door for starting a coding tool with everything Synapse can give
//! it already in place: memory and the vault reachable over MCP, this project's
//! credentials in the environment, and the project root the tool should treat as
//! home. Without it, "set up correctly" is three commands and a shell hook.
//!
//! Nothing here is permanent. A tool Synapse has never been connected to is
//! handed a generated config for the life of the process, so this works on a
//! machine where `synapse connect` was never run and leaves that machine exactly
//! as it found it.

use crate::cli::Outcome;
use crate::cli::relay::{value, values};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "usage: synapse launch <tool> [--directory <folder>] [--model <model>] \
     [--allow-tool <rule>]... [--strict] [--no-vault] [--as <name> [--role <role>] \
     [--task <text>] [--channel <name>]...] [--print] [-- <flags passed to the tool>]";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    // Split before parsing. Everything after the first bare `--` belongs to the
    // tool, and a `--model` on that side is the tool's to interpret — reading
    // flags out of the whole list would quietly steal it.
    let (mine, theirs) = split(arguments);
    let tool = mine
        .iter()
        .find(|value| !value.to_string_lossy().starts_with("--"))
        .and_then(|value| value.to_str())
        .map(str::to_lowercase)
        .with_context(|| USAGE.to_owned())?;

    let root = match value(&mine, "--directory") {
        Some(value) => PathBuf::from(value),
        None => std::env::current_dir().context("could not determine the current folder")?,
    };
    let name = value(&mine, "--as");
    let role = value(&mine, "--role").unwrap_or_else(|| "worker".to_owned());
    let task = value(&mine, "--task");
    let model = value(&mine, "--model");
    let channels = values(&mine, "--channel");
    let allowed = values(&mine, "--allow-tool");
    let novault = mine.iter().any(|value| value == "--no-vault");
    let preview = mine.iter().any(|value| value == "--print");

    let runtime = tokio::runtime::Runtime::new()?;
    if name.is_some() {
        crate::cli::launch::requiremesh()?;
    }

    let built = crate::relay::launch(&crate::relay::Options {
        name: name.as_deref(),
        role: &role,
        root: &root,
        tool: Some(&tool),
        task: task.as_deref(),
        channels: &channels,
        tools: &allowed,
        model: model.as_deref(),
        // A named launch from a person's own terminal is the lead: it should
        // hand the session back rather than park on `wait`.
        lead: name.is_some(),
        optimize: false,
        headless: false,
        skippermissions: mine.iter().any(|value| value == "--skip-permissions"),
        strict: mine.iter().any(|value| value == "--strict"),
        command: None,
        extra: &theirs,
    })?;

    if preview {
        runtime.block_on(print(&built, &root, novault))?;
        return Ok(Outcome::Exit(0));
    }

    // Resolved last, so a scope problem is reported before anything is spent
    // resolving a tool, and the values live for as little time as possible.
    let secrets = if novault {
        Vec::new()
    } else {
        runtime.block_on(crate::vault::environment(&root))?
    };
    notice(&tool, &root, name.as_deref(), secrets.len());

    let status = std::process::Command::new(&built.program)
        .args(&built.arguments)
        .envs(built.environment.clone())
        .envs(secrets)
        .current_dir(&root)
        .status()
        .with_context(|| format!("could not run {}", built.program.display()))?;
    Ok(Outcome::Exit(status.code().unwrap_or(1)))
}

/// Everything before the first bare `--`, and everything after it.
fn split(arguments: &[OsString]) -> (Vec<OsString>, Vec<String>) {
    match arguments.iter().position(|value| value == "--") {
        Some(at) => (
            arguments[..at].to_vec(),
            arguments[at + 1..]
                .iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect(),
        ),
        None => (arguments.to_vec(), Vec::new()),
    }
}

/// What this launch would run, for a bug report or a `--print` before trusting
/// it with a shell.
///
/// Prints variable *names* and never a value. A preview that leaked a secret
/// into a terminal would defeat the reason the values are in Keychain at all.
async fn print(built: &crate::relay::Launch, root: &std::path::Path, novault: bool) -> Result<()> {
    let mut line = vec![built.program.display().to_string()];
    line.extend(built.arguments.iter().map(|value| {
        if value.contains(char::is_whitespace) {
            format!("{value:?}")
        } else {
            value.clone()
        }
    }));
    println!("{}", line.join(" "));
    for (key, value) in &built.environment {
        println!("env  {key}={value}");
    }
    if !novault {
        for name in crate::vault::names(root).await? {
            println!("env  {name}=<from keychain>");
        }
    }
    Ok(())
}

/// One line about what the tool is getting, before it takes the terminal.
///
/// Reports rather than fixes. A stale pointer is worth knowing about and is not
/// worth refusing a launch over, and repairing somebody's instruction file as a
/// side effect of starting a tool is exactly the surprise Synapse promises not
/// to be.
fn notice(tool: &str, root: &std::path::Path, name: Option<&str>, secrets: usize) {
    let mut parts = Vec::new();
    let connected = crate::files::home()
        .map(|home| {
            crate::agent::agents(&home)
                .into_iter()
                .find(|agent| agent.command == tool)
                .map(|agent| {
                    crate::agent::detect(&agent, crate::cli::destination().ok().as_deref())
                })
        })
        .ok()
        .flatten();
    match connected {
        Some(detection) if detection.configured => parts.push("memory".to_owned()),
        // Nothing was written to the tool's own configuration; the wiring lives
        // for the life of this process.
        _ => parts.push("memory (wired for this run)".to_owned()),
    }
    if secrets > 0 {
        parts.push(format!(
            "{secrets} vault variable{}",
            if secrets == 1 { "" } else { "s" }
        ));
    }
    if let Some(name) = name {
        parts.push(format!("mesh as `{name}`"));
    }
    println!(
        "Launching {tool} in {} · {}",
        root.display(),
        parts.join(" · ")
    );

    if let (Ok(home), Ok(soul)) = (crate::files::home(), crate::files::soul()) {
        let stale = crate::agent::agents(&home).into_iter().any(|agent| {
            agent.command == tool && !crate::agent::pointermatches(&agent.instructions, &soul)
        });
        if stale {
            eprintln!(
                "note: {tool}'s guidance pointer is missing or from an older release; \
                 `synapse guidance sync` refreshes it"
            );
        }
    }
}
