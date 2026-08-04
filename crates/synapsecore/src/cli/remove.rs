//! `synapse disconnect` and `synapse uninstall`.
//!
//! What Synapse writes outside its own folder it must be able to take back:
//! two tools' MCP registries, two instruction files, Claude Code's settings,
//! two skills folders, a shell rc file, and a binary on PATH. Reversing that by
//! hand is not something to ask of somebody who has decided to stop using it.
//!
//! Memory is never removed by either command. It is the thing the user spent
//! their time on, it is the one part that is theirs alone, and deleting it is a
//! separate decision with its own confirmation.

use crate::cli::Outcome;
use anyhow::Result;
use std::ffi::OsString;
use std::path::Path;

pub fn disconnect(arguments: &[OsString]) -> Result<Outcome> {
    let wanted = arguments
        .iter()
        .find(|value| !value.to_string_lossy().starts_with("--"))
        .map(|value| value.to_string_lossy().to_lowercase());
    let home = crate::files::home()?;
    let server = crate::cli::destination()?;
    let agents = crate::agent::agents(&home);
    let chosen: Vec<_> = match &wanted {
        Some(name) => agents
            .into_iter()
            .filter(|agent| agent.name.to_lowercase().contains(name) || agent.command == name)
            .collect(),
        None => agents,
    };
    anyhow::ensure!(
        !chosen.is_empty(),
        "no connected tool matches `{}`; use claude, codex, or pi",
        wanted.unwrap_or_default()
    );

    let runtime = tokio::runtime::Runtime::new()?;
    let mut removed = crate::agent::Removed::default();
    for agent in &chosen {
        let outcome = runtime.block_on(crate::agent::disconnect(agent, &server));
        removed.done.extend(outcome.done);
        removed.problems.extend(outcome.problems);
    }
    report(&removed, "Nothing to disconnect.");
    Ok(Outcome::Exit(i32::from(!removed.problems.is_empty())))
}

pub fn uninstall(arguments: &[OsString]) -> Result<Outcome> {
    let confirmed = arguments.iter().any(|value| value == "--confirm");
    let alsodata = arguments.iter().any(|value| value == "--data");
    if !confirmed {
        return preview(alsodata);
    }

    let home = crate::files::home()?;
    let server = crate::cli::destination()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut removed = crate::agent::Removed::default();

    for agent in crate::agent::agents(&home) {
        let outcome = runtime.block_on(crate::agent::disconnect(&agent, &server));
        removed.done.extend(outcome.done);
        removed.problems.extend(outcome.problems);
    }

    // Ask what is there before taking it out, so a hook that was never
    // installed is not reported as one that was just removed.
    match crate::shellsetup::status(&server) {
        Ok(before) if before.state == crate::shellsetup::IntegrationState::Missing => {}
        Ok(_) => match crate::shellsetup::remove(&server) {
            Ok(after) if after.state == crate::shellsetup::IntegrationState::Missing => {
                removed.done.push(format!("the {} hook", after.shell));
            }
            Ok(after) => removed.problems.push(format!(
                "the {} hook was edited after Synapse wrote it, so it was left in place",
                after.shell
            )),
            Err(error) => removed.problems.push(format!("shell hook: {error:#}")),
        },
        Err(error) => removed.problems.push(format!("shell hook: {error:#}")),
    }

    // The data folder holds the memory, and losing that by running a command
    // about *installation* would be the worst thing this could do.
    if alsodata {
        match crate::files::data()
            .and_then(|data| Ok(std::fs::remove_dir_all(&data).map(|_| data)?))
        {
            Ok(data) => removed
                .done
                .push(format!("everything in {}", data.display())),
            Err(error) => removed.problems.push(format!("data folder: {error:#}")),
        }
    }

    // The binary goes last: it is the one running this.
    match removebinary() {
        Ok(true) => removed
            .done
            .push(format!("the CLI at {}", server.display())),
        Ok(false) => {}
        Err(error) => removed
            .problems
            .push(format!("command line tool: {error:#}")),
    }

    report(&removed, "Synapse was not installed anywhere.");
    if !alsodata && let Ok(data) = crate::files::data() {
        println!();
        println!("Your memory is untouched, in {}.", data.display());
        println!("Remove it with `synapse uninstall --data --confirm`, or delete the folder.");
    }
    Ok(Outcome::Exit(i32::from(!removed.problems.is_empty())))
}

/// Say what would go before anything does. Uninstalling is the one operation
/// where a surprise is unrecoverable.
fn preview(alsodata: bool) -> Result<Outcome> {
    let home = crate::files::home()?;
    let soul = crate::files::soul()?;
    let server = crate::cli::destination()?;
    println!("`synapse uninstall --confirm` would remove:");
    for agent in crate::agent::agents(&home) {
        let detection = crate::agent::detect(&agent, Some(&server));
        if detection.configured {
            println!("  · the Synapse MCP server from {}", agent.name);
        }
        if crate::agent::pointermatches(&agent.instructions, &soul) {
            println!("  · the Synapse block in {}", agent.instructions.display());
        }
        if detection.hooks.notice || detection.hooks.statusline {
            println!("  · the Synapse session notice and status line from Claude Code");
        }
    }
    if let Ok(integration) = crate::shellsetup::status(&server)
        && integration.state != crate::shellsetup::IntegrationState::Missing
    {
        println!("  · the shell hook in {}", integration.path.display());
    }
    if matches!(
        crate::cli::status()?,
        crate::cli::InstallStatus::Installed(_)
    ) {
        println!("  · the command line tool at {}", server.display());
    }
    println!("  · every skill Synapse installed, leaving any you wrote");
    let data = crate::files::data()?;
    println!();
    if alsodata {
        println!(
            "And, because --data was given, everything in {}",
            data.display()
        );
        println!("including all of your memory. That cannot be undone.");
    } else {
        println!("Your memory in {} would be left alone.", data.display());
    }
    println!();
    println!("Add --confirm to go ahead.");
    Ok(Outcome::Exit(0))
}

/// Remove the installed CLI and its receipt, and only when it is the copy
/// Synapse put there.
fn removebinary() -> Result<bool> {
    match crate::cli::status()? {
        crate::cli::InstallStatus::Missing => Ok(false),
        crate::cli::InstallStatus::Conflict(path) => {
            anyhow::bail!(
                "{} is not the Synapse executable, so it was left alone",
                path.display()
            )
        }
        crate::cli::InstallStatus::Installed(path) => {
            std::fs::remove_file(&path)?;
            let mut receipt = path.clone().into_os_string();
            receipt.push(".synapsereceipt");
            let _ = std::fs::remove_file(Path::new(&receipt));
            Ok(true)
        }
    }
}

fn report(removed: &crate::agent::Removed, empty: &str) {
    if removed.done.is_empty() && removed.problems.is_empty() {
        println!("{empty}");
        return;
    }
    for done in &removed.done {
        println!("Removed {done}");
    }
    for problem in &removed.problems {
        eprintln!("warning: {problem}");
    }
}
