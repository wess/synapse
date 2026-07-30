use crate::cli::Outcome;
use crate::relay::Mesh;
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

const USAGE: &str = "usage: synapse relay <status|agents|channels|feed|launch|team|role|ps|kill>";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = text(arguments, 0, USAGE)?;
    match action.as_str() {
        "launch" => return super::launch::launch(&arguments[1..]),
        "team" if arguments.get(1).is_some_and(|value| value == "open") => {
            return super::launch::open(&arguments[2..]);
        }
        "role" => return super::roles::role(&arguments[1..]),
        "team" => return super::roles::team(&arguments[1..]),
        _ => {}
    }

    let runtime = tokio::runtime::Runtime::new()?;
    let mesh = runtime.block_on(Mesh::open(crate::files::database()?))?;
    let json = arguments.iter().any(|value| value == "--json");
    match action.as_str() {
        "status" => runtime.block_on(status(&mesh, json))?,
        "agents" => runtime.block_on(agents(&mesh, json))?,
        "channels" => runtime.block_on(channels(&mesh, json))?,
        "feed" => runtime.block_on(feed(&mesh, arguments, json))?,
        "ps" => runtime.block_on(workers(&mesh, json))?,
        "kill" => runtime.block_on(kill(&mesh, arguments))?,
        unknown => anyhow::bail!("unknown relay command `{unknown}`\n\n{USAGE}"),
    }
    Ok(Outcome::Exit(0))
}

async fn status(mesh: &Mesh, json: bool) -> Result<()> {
    let brain = crate::brain::Brain::open(crate::files::database()?).await?;
    let enabled = brain.mesh().await?;
    let agents = mesh.agents().await?;
    let workers = mesh.workers().await?;
    let online = agents.iter().filter(|agent| agent.online).count();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "enabled": enabled,
                "agents": agents,
                "workers": workers,
            }))?
        );
        return Ok(());
    }
    println!("Mesh: {}", if enabled { "on" } else { "off" });
    println!("Agents: {online} online of {}", agents.len());
    println!("Workers: {}", workers.len());
    if !enabled {
        println!("Turn it on with `synapse settings mesh on`.");
    }
    Ok(())
}

async fn agents(mesh: &Mesh, json: bool) -> Result<()> {
    let agents = mesh.agents().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&agents)?);
        return Ok(());
    }
    for agent in agents {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            agent.name,
            if agent.role.is_empty() {
                "-"
            } else {
                &agent.role
            },
            if agent.online { "online" } else { "offline" },
            if agent.status.is_empty() {
                "-"
            } else {
                &agent.status
            },
            if agent.project.is_empty() {
                "-"
            } else {
                &agent.project
            }
        );
    }
    Ok(())
}

async fn channels(mesh: &Mesh, json: bool) -> Result<()> {
    let channels = mesh.channels().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&channels)?);
        return Ok(());
    }
    for channel in channels {
        println!("#{}\t{}", channel.channel, channel.subscribers);
    }
    Ok(())
}

async fn feed(mesh: &Mesh, arguments: &[OsString], json: bool) -> Result<()> {
    let follow = arguments.iter().any(|value| value == "--follow");
    let mut cursor = value(arguments, "--since")
        .map(|value| value.parse::<i64>())
        .transpose()
        .context("--since takes a message id")?
        .unwrap_or(0);
    loop {
        let messages = mesh.feed(cursor, 200).await?;
        if let Some(last) = messages.last() {
            cursor = last.id;
        }
        if json {
            for message in &messages {
                println!("{}", serde_json::to_string(message)?);
            }
        } else {
            for message in &messages {
                println!("{}", line(message));
            }
        }
        if !follow {
            return Ok(());
        }
        // Nothing here holds a wake signal, so the feed is polled at a cadence a
        // person reading along cannot tell from live.
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn line(message: &crate::relay::Message) -> String {
    let to = match (&message.kind, &message.target) {
        (crate::relay::MessageKind::Direct, Some(target)) => format!("→ {target}"),
        (crate::relay::MessageKind::Channel, Some(target)) => format!("→ #{target}"),
        _ => "→ everyone".to_owned(),
    };
    format!(
        "{}\t{}\t{}\t{}",
        message.id, message.sender, to, message.body
    )
}

async fn workers(mesh: &Mesh, json: bool) -> Result<()> {
    let workers = mesh.workers().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&workers)?);
        return Ok(());
    }
    for worker in workers {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            worker.name,
            if worker.role.is_empty() {
                "-"
            } else {
                &worker.role
            },
            worker.status,
            worker.process,
            worker.log
        );
    }
    Ok(())
}

async fn kill(mesh: &Mesh, arguments: &[OsString]) -> Result<()> {
    let name = text(arguments, 1, "usage: synapse relay kill <name>")?;
    // A worker belongs to the session that spawned it. From here the only ones
    // that can be stopped are those the mesh still lists but nobody owns, so
    // say which of those happened rather than reporting a stop either way.
    let known = mesh.worker(&name).await?.is_some();
    anyhow::ensure!(known, "no worker named `{name}`");
    crate::relay::Supervisor::new().stop(mesh, &name).await?;
    println!("Stopped {name}");
    Ok(())
}

/// The folder a relay command resolves project-layer roles and teams against.
pub fn directory(arguments: &[OsString]) -> Result<PathBuf> {
    match value(arguments, "--directory") {
        Some(value) => Ok(PathBuf::from(value)),
        None => std::env::current_dir().context("could not determine the current folder"),
    }
}

/// The value that follows a `--flag`, when it is present.
pub fn value(arguments: &[OsString], flag: &str) -> Option<String> {
    arguments
        .iter()
        .position(|item| item == flag)
        .and_then(|at| arguments.get(at + 1))
        .and_then(|item| item.to_str())
        .map(ToOwned::to_owned)
}

/// Every value of a repeatable `--flag`.
pub fn values(arguments: &[OsString], flag: &str) -> Vec<String> {
    arguments
        .iter()
        .enumerate()
        .filter(|(_, item)| *item == flag)
        .filter_map(|(at, _)| arguments.get(at + 1))
        .filter_map(|item| item.to_str())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn text(arguments: &[OsString], index: usize, usage: &str) -> Result<String> {
    arguments
        .get(index)
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .with_context(|| usage.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn a_repeatable_flag_collects_every_value() {
        let given = arguments(&["launch", "a", "--channel", "one", "--channel", "two"]);
        assert_eq!(values(&given, "--channel"), ["one", "two"]);
        assert!(values(&given, "--model").is_empty());
    }

    #[test]
    fn a_flag_with_no_value_after_it_reads_as_absent() {
        let given = arguments(&["launch", "a", "--model"]);
        assert_eq!(value(&given, "--model"), None);
        assert_eq!(value(&given, "--missing"), None);
    }

    #[test]
    fn feed_lines_name_the_recipient() {
        let message = crate::relay::Message {
            id: 4,
            sender: "lead".to_owned(),
            kind: crate::relay::MessageKind::Channel,
            target: Some("devops".to_owned()),
            body: "deploy".to_owned(),
            created: 0,
        };
        assert_eq!(line(&message), "4\tlead\t→ #devops\tdeploy");

        let broadcast = crate::relay::Message {
            kind: crate::relay::MessageKind::Broadcast,
            target: None,
            ..message
        };
        assert!(line(&broadcast).contains("→ everyone"));
    }
}
