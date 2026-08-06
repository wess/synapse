//! Starting agents from the command line.
//!
//! `launch` runs one agent in this terminal. `team open` runs a whole roster:
//! every member but the first becomes a supervised background worker, and the
//! lead runs in the foreground so there is someone to steer. This process is the
//! supervisor, so closing the lead takes its team down with it rather than
//! leaving headless agents running behind you.

use crate::cli::Outcome;
use crate::cli::relay::{directory, text, value, values};
use crate::relay::{self, Mesh, Supervisor};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::Path;

const USAGE: &str = "usage: synapse relay launch <name> [--role <role>] [--tool <tool>] \
     [--task <text>] [--channel <name>]... [--allow-tool <rule>]... [--model <model>] \
     [--directory <folder>] [--lead] [--optimize] [--strict] [--command <template>] [--print]";

pub fn launch(arguments: &[OsString]) -> Result<Outcome> {
    let name = text(arguments, 0, USAGE)?;
    let root = directory(arguments)?;
    let role = value(arguments, "--role").unwrap_or_else(|| "worker".to_owned());
    let channels = values(arguments, "--channel");
    let tools = values(arguments, "--allow-tool");
    let task = value(arguments, "--task");
    let model = value(arguments, "--model");
    let tool = value(arguments, "--tool");
    let command = value(arguments, "--command");

    let built = relay::launch(&relay::Options {
        name: Some(&name),
        role: &role,
        root: &root,
        tool: tool.as_deref(),
        task: task.as_deref(),
        channels: &channels,
        tools: &tools,
        model: model.as_deref(),
        lead: arguments.iter().any(|value| value == "--lead"),
        optimize: arguments.iter().any(|value| value == "--optimize"),
        headless: false,
        skippermissions: arguments.iter().any(|value| value == "--skip-permissions"),
        strict: arguments.iter().any(|value| value == "--strict"),
        command: command.as_deref(),
        extra: &[],
    })?;

    if arguments.iter().any(|value| value == "--print") {
        println!("{}", printable(&built));
        return Ok(Outcome::Exit(0));
    }

    requiremesh()?;
    let status = std::process::Command::new(&built.program)
        .args(&built.arguments)
        .envs(built.environment.clone())
        .current_dir(&root)
        .status()
        .with_context(|| format!("could not run {}", built.program.display()))?;
    Ok(Outcome::Exit(status.code().unwrap_or(1)))
}

pub fn open(arguments: &[OsString]) -> Result<Outcome> {
    let name = text(
        arguments,
        0,
        "usage: synapse relay team open <name> [--directory <folder>]",
    )?;
    let root = directory(arguments)?;
    let team = relay::team::resolve(Some(&root), &name)?
        .with_context(|| format!("no team named `{name}`"))?;
    requiremesh()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(runteam(&team, &root))
}

async fn runteam(team: &relay::team::Team, root: &Path) -> Result<Outcome> {
    let mesh = Mesh::open(crate::files::database()?).await?;
    let supervisor = Supervisor::new();
    let Some((lead, rest)) = team.members.split_first() else {
        anyhow::bail!("the team `{}` has no members", team.name);
    };

    for member in rest {
        let role = member.role.clone().unwrap_or_else(|| "worker".to_owned());
        let built = relay::launch(&relay::Options {
            name: Some(&member.name),
            role: &role,
            root,
            tool: member.tool.as_deref(),
            task: None,
            channels: &[],
            tools: &[],
            model: None,
            lead: false,
            optimize: false,
            headless: true,
            skippermissions: true,
            strict: false,
            command: None,
            extra: &[],
        })?;
        let tool = built.tool.clone();
        let log = supervisor
            .launch(
                &mesh,
                relay::Spec {
                    name: member.name.clone(),
                    role,
                    program: built.program,
                    arguments: built.arguments,
                    environment: built.environment,
                    directory: root.to_path_buf(),
                    keepalive: true,
                    session: built.session,
                },
            )
            .await?;
        println!("Started {} ({tool}) · {}", member.name, log.display());
    }

    let role = lead.role.clone().unwrap_or_else(|| "supervisor".to_owned());
    let built = relay::launch(&relay::Options {
        name: Some(&lead.name),
        role: &role,
        root,
        tool: lead.tool.as_deref(),
        task: None,
        channels: &[],
        tools: &[],
        model: None,
        // The first member is the one you talk to, so it stays interactive.
        lead: true,
        optimize: false,
        headless: false,
        skippermissions: false,
        strict: false,
        command: None,
        extra: &[],
    })?;
    println!("Opening {} as {}", lead.name, role);

    let status = tokio::process::Command::new(&built.program)
        .args(&built.arguments)
        .envs(built.environment.clone())
        .current_dir(root)
        .status()
        .await
        .with_context(|| format!("could not run {}", built.program.display()))?;

    // The lead is gone, so nobody is left to steer the team.
    supervisor.stopall(&mesh).await;
    Ok(Outcome::Exit(status.code().unwrap_or(1)))
}

/// Launching wires an agent into a mesh it can only reach when the mesh is on.
pub fn requiremesh() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let brain = runtime.block_on(crate::brain::Brain::open(crate::files::database()?))?;
    anyhow::ensure!(
        runtime.block_on(brain.mesh())?,
        "the Synapse mesh is off; turn it on with `synapse settings mesh on`"
    );
    Ok(())
}

/// The resolved command as a single shell-ish line, for `--print`.
fn printable(built: &relay::Launch) -> String {
    let mut parts = vec![built.program.display().to_string()];
    parts.extend(built.arguments.iter().map(|value| quote(value)));
    parts.join(" ")
}

fn quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || "-_./:=".contains(item))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printing_keeps_a_multiword_prompt_in_one_quoted_argument() {
        let built = relay::Launch {
            program: std::path::PathBuf::from("/usr/bin/claude"),
            arguments: vec![
                "You are \"backend\".".to_owned(),
                "--model".to_owned(),
                "claude-opus-5".to_owned(),
            ],
            environment: Vec::new(),
            tool: "claude".to_owned(),
            session: None,
        };

        let printed = printable(&built);

        assert!(printed.starts_with("/usr/bin/claude '"));
        assert!(printed.ends_with("--model claude-opus-5"));
        assert_eq!(printed.matches('\'').count(), 2);
    }

    #[test]
    fn quoting_escapes_an_embedded_quote() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote(""), "''");
    }
}
