//! Undoing a connection.
//!
//! Synapse writes into files it did not create — a tool's MCP registry, its
//! global instruction file, Claude Code's settings, its skills folder — and
//! software that edits your configuration owes you a way back out. Setup is
//! several steps in several places, so leaving the user to reverse each by hand
//! is leaving most of it behind.
//!
//! Every step removes only what Synapse put there and reports what it did. A
//! step that finds nothing is not a failure, and a step that fails does not
//! stop the others: half-disconnected is the worst outcome available, so the
//! run continues and says which parts did not come out.

use crate::agent::{Agent, Kind};
use anyhow::{Context, Result};
use std::path::Path;

/// What came out, and what did not.
#[derive(Debug, Default)]
pub struct Removed {
    pub done: Vec<String>,
    pub problems: Vec<String>,
}

impl Removed {
    fn step(&mut self, what: &str, outcome: Result<bool>) {
        match outcome {
            Ok(true) => self.done.push(what.to_owned()),
            Ok(false) => {}
            Err(error) => self.problems.push(format!("{what}: {error:#}")),
        }
    }

    fn absorb(&mut self, other: Removed) {
        self.done.extend(other.done);
        self.problems.extend(other.problems);
    }
}

/// Disconnect one tool: its MCP registration, the managed block in its
/// instruction file, the Claude Code session notice and status line, and any
/// skill Synapse installed for it.
pub async fn disconnect(agent: &Agent, server: &Path) -> Removed {
    let mut removed = Removed::default();
    removed.step(&connection(agent), unregister(agent, server));
    removed.step(
        &format!("{} guidance pointer", agent.name),
        crate::agent::guidance::removepointer(&agent.instructions),
    );
    if agent.kind == Kind::Claude {
        removed.step(
            "Claude Code session notice and status line",
            notice(&agent.settings, server),
        );
    }
    removed.absorb(skills(agent).await);
    removed
}

/// What the connection is called for this tool, so the report says what was
/// actually taken out rather than what it is called elsewhere.
fn connection(agent: &Agent) -> String {
    match agent.detect.style {
        crate::agent::tool::Style::Package => format!("the {} package", agent.name),
        crate::agent::tool::Style::Server => format!("{} MCP registration", agent.name),
    }
}

/// Ask the tool's own CLI to forget the server, the same way setup asked it to
/// remember one. Nothing here edits the tool's config file directly.
fn unregister(agent: &Agent, server: &Path) -> Result<bool> {
    let detection = crate::agent::detect(agent, None);
    if !detection.registered {
        return Ok(false);
    }
    anyhow::ensure!(
        !agent.connect.remove.is_empty(),
        "the `{}` tool does not say how to disconnect it; give its descriptor a `connect.remove`",
        agent.slug
    );
    let executable = detection
        .executable
        .as_deref()
        .context("the tool is not installed or is not on PATH")?;
    let output = crate::agent::command(executable)
        .args(super::setup::argv(&agent.connect.remove, agent, server))
        .output()
        .with_context(|| format!("could not run the {} removal command", agent.name))?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(true)
}

fn notice(settings: &Path, server: &Path) -> Result<bool> {
    let before = crate::agent::hooks::state(settings, server);
    if !before.notice && !before.statusline {
        return Ok(false);
    }
    crate::agent::hooks::remove(settings, server)?;
    Ok(true)
}

/// Take back only the skills Synapse installed and nobody has edited since.
/// One the user wrote, or changed in place, stays and is reported.
async fn skills(agent: &Agent) -> Removed {
    let mut removed = Removed::default();
    let listed = async {
        let receipts = crate::skill::Receipts::open(crate::files::database()?).await?;
        let installed = receipts.installed(&agent.name).await?;
        Ok::<_, anyhow::Error>((receipts, installed))
    }
    .await;
    let (receipts, installed) = match listed {
        Ok(listed) => listed,
        Err(error) => {
            removed
                .problems
                .push(format!("{} skills: {error:#}", agent.name));
            return removed;
        }
    };
    for skill in installed {
        removed.step(
            &format!("{} skill `{skill}`", agent.name),
            crate::skill::remove(&receipts, agent, &skill, false).await,
        );
    }
    removed
}
