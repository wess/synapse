//! `synapse mcp <verb>`: the MCP server library.
//!
//! `synapse mcp` with nothing after it is still Synapse's own server, which is
//! what every tool's existing registration runs; a word after it is one of
//! these. `run` is the one a person never types — it is what a tool starts.

use crate::agent::Agent;
use crate::cli::Outcome;
use crate::cli::editor::editor;
use crate::servers::{self, Holding, Server};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::io::{IsTerminal, Read};

const USAGE: &str = "usage: synapse mcp <list|show|add|edit|delete|install|uninstall|status|adopt|secret|search|get|run>";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = super::relay::text(arguments, 0, USAGE)?;
    let json = arguments.iter().any(|value| value == "--json");
    match action.as_str() {
        "run" => {
            let name = super::relay::text(arguments, 1, "usage: synapse mcp run <name>")?;
            return Ok(Outcome::Exit(servers::run(&name)?));
        }
        "list" => list(json)?,
        "show" => show(arguments)?,
        "add" => add(arguments)?,
        "edit" => edit(arguments)?,
        "delete" => delete(arguments)?,
        "install" => install(arguments)?,
        "uninstall" => uninstall(arguments)?,
        "status" => status(arguments, json)?,
        "adopt" => adopt(arguments)?,
        "secret" => secret(arguments)?,
        "search" => search(arguments, json)?,
        "get" => get(arguments)?,
        unknown => anyhow::bail!("unknown mcp command `{unknown}`\n\n{USAGE}"),
    }
    Ok(Outcome::Exit(0))
}

fn list(json: bool) -> Result<()> {
    let (library, problems) = servers::all()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&library)?);
    } else if library.is_empty() {
        println!("The MCP library is empty.");
        println!(
            "`synapse mcp search <words>`, `synapse mcp add <name> -- <command>`, or `synapse mcp adopt <name>`."
        );
    } else {
        for server in &library {
            let missing = server.missing();
            println!(
                "{}\t{}{}",
                server.name,
                server.commandline(),
                match missing.is_empty() {
                    true => String::new(),
                    false => format!("\tneeds {}", missing.join(", ")),
                }
            );
        }
    }
    report(&problems);
    Ok(())
}

fn show(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse mcp show <name>")?;
    print!("{}", servers::read(&name)?.text()?);
    Ok(())
}

/// `synapse mcp add <name> [--description <text>] -- <command> [args…]`. The
/// command is everything after the bare `--`, so its own flags are never read
/// as ours.
fn add(arguments: &[OsString]) -> Result<()> {
    let usage = "usage: synapse mcp add <name> [--description <text>] -- <command> [args...]";
    let split = arguments
        .iter()
        .position(|value| value == "--")
        .context(usage)?;
    let (ours, theirs) = arguments.split_at(split);
    let name = super::relay::text(ours, 1, usage)?;
    let mut command = theirs[1..].iter().map(|value| {
        value
            .to_str()
            .map(ToOwned::to_owned)
            .context("the command is not valid UTF-8")
    });
    let program = command.next().context(usage)??;
    let server = Server {
        name: name.clone(),
        description: super::relay::value(ours, "--description").unwrap_or_default(),
        command: program,
        args: command.collect::<Result<_>>()?,
        ..Server::default()
    };
    servers::validname(&name)?;
    anyhow::ensure!(
        servers::read(&name).is_err(),
        "the library already has a server named `{name}`"
    );
    let path = servers::save(&server)?;
    println!("Added {name} at {}", path.display());
    println!("Secrets go in the vault: `synapse mcp secret {name} <VARIABLE>`.");
    println!("Then `synapse mcp install {name}` gives it to every connected tool.");
    Ok(())
}

fn edit(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse mcp edit <name>")?;
    let current = servers::read(&name)?.text()?;
    let draft = std::env::temp_dir().join(format!("synapse-mcp-{name}.toml"));
    std::fs::write(&draft, &current)?;
    let status = std::process::Command::new(editor())
        .arg(&draft)
        .status()
        .context("could not open your editor")?;
    anyhow::ensure!(status.success(), "the editor exited without saving");
    let edited = std::fs::read_to_string(&draft)?;
    match Server::parse(&name, &edited).and_then(|server| servers::save(&server)) {
        Ok(path) => {
            let _ = std::fs::remove_file(&draft);
            println!("Saved {}", path.display());
            // Tools run it through `synapse mcp run`, so there is nothing to
            // push: the next session that starts it reads this.
            println!(
                "Every tool that has it picks the change up the next time it starts the server."
            );
            Ok(())
        }
        Err(error) => {
            Err(error).with_context(|| format!("your draft is still at {}", draft.display()))
        }
    }
}

fn delete(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse mcp delete <name> --confirm")?;
    anyhow::ensure!(
        arguments.iter().any(|value| value == "--confirm"),
        "add --confirm to delete `{name}`, take it out of every tool Synapse gave it to, and forget its secrets"
    );
    servers::read(&name)?;
    let binary = crate::cli::destination()?;
    for agent in agents()? {
        match servers::uninstall(&agent, &name, &binary) {
            Ok(true) => println!("{name} ✕ {}", agent.name),
            Ok(false) => {}
            // Somebody's own entry by that name is theirs and stays.
            Err(error) => eprintln!("warning: {error:#}"),
        }
    }
    let forgotten = tokio::runtime::Runtime::new()?.block_on(servers::forgetsecret(&name))?;
    let path = servers::delete(&name)?;
    println!("Deleted {}", path.display());
    if forgotten > 0 {
        println!("Forgot {forgotten} secret(s) it held.");
    }
    Ok(())
}

fn install(arguments: &[OsString]) -> Result<()> {
    let replace = arguments.iter().any(|value| value == "--replace");
    let binary = crate::cli::destination()?;
    let (library, problems) = servers::all()?;
    report(&problems);
    let wanted = named(arguments);
    let selected: Vec<&Server> = library
        .iter()
        .filter(|server| wanted.as_ref().is_none_or(|name| *name == server.name))
        .collect();
    if let Some(name) = &wanted {
        anyhow::ensure!(
            !selected.is_empty(),
            "the library has no MCP server named `{name}`"
        );
    }
    let agents: Vec<Agent> = chosen(arguments)?
        .into_iter()
        .filter(|agent| agent.servers.supported())
        .collect();
    anyhow::ensure!(
        !agents.is_empty(),
        "no connected tool here can hold an MCP server"
    );
    let mut failed = false;
    for server in selected {
        let missing = server.missing();
        if !missing.is_empty() {
            // Installing it anyway would put a server in every tool that fails
            // every time it starts.
            eprintln!(
                "warning: skipped {}; it still needs {} (`synapse mcp secret {} <VARIABLE>` or `synapse mcp edit {}`)",
                server.name,
                missing.join(", "),
                server.name,
                server.name
            );
            failed = true;
            continue;
        }
        for agent in &agents {
            match servers::install(agent, server, &binary, replace) {
                Ok(_) => println!("{} → {}", server.name, agent.name),
                Err(error) => {
                    eprintln!("warning: {} → {}: {error:#}", server.name, agent.name);
                    failed = true;
                }
            }
        }
    }
    if failed && !replace {
        eprintln!(
            "A tool's own server by the same name is left alone; add --replace to swap it for the library's."
        );
    }
    Ok(())
}

fn uninstall(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(
        arguments,
        1,
        "usage: synapse mcp uninstall <name> [--tool <tool>]",
    )?;
    let binary = crate::cli::destination()?;
    let mut any = false;
    for agent in chosen(arguments)? {
        match servers::uninstall(&agent, &name, &binary) {
            Ok(true) => {
                any = true;
                println!("{name} ✕ {}", agent.name);
            }
            Ok(false) => {}
            Err(error) => eprintln!("warning: {error:#}"),
        }
    }
    if !any {
        println!("No tool had `{name}` from Synapse.");
    }
    Ok(())
}

fn status(arguments: &[OsString], json: bool) -> Result<()> {
    let binary = crate::cli::destination()?;
    let (library, problems) = servers::all()?;
    let agents = agents()?;
    let wanted = named(arguments);
    let placements: Vec<_> = servers::survey(&agents, &library, &binary)
        .into_iter()
        .filter(|placement| wanted.as_ref().is_none_or(|name| *name == placement.server))
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&placements)?);
        return Ok(());
    }
    for placement in &placements {
        println!(
            "{}\t{}\t{}",
            placement.server,
            placement.tool,
            placement.holding.label()
        );
    }
    let known: Vec<String> = library.iter().map(|server| server.name.clone()).collect();
    let foreign = servers::foreign(&agents, &known);
    if !foreign.is_empty() && wanted.is_none() {
        println!();
        println!("In your tools but not the library (`synapse mcp adopt <name> --tool <tool>`):");
        for (tool, name) in foreign {
            println!("\t{name}\t{tool}");
        }
    }
    if placements
        .iter()
        .any(|placement| placement.holding == Holding::Stale)
    {
        println!();
        println!("`synapse mcp install` brings the stale ones up to date.");
    }
    report(&problems);
    Ok(())
}

fn adopt(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(
        arguments,
        1,
        "usage: synapse mcp adopt <name> [--tool <tool>]",
    )?;
    let candidates: Vec<Agent> = chosen(arguments)?
        .into_iter()
        .filter(|agent| {
            crate::agent::config::servers(agent, &agent.servers.at)
                .iter()
                .any(|(entry, _)| *entry == name)
        })
        .collect();
    let agent = match candidates.as_slice() {
        [] => anyhow::bail!("no connected tool has an MCP server named `{name}`"),
        [agent] => agent,
        _ => anyhow::bail!(
            "more than one tool has `{name}` ({}); say which with --tool",
            candidates
                .iter()
                .map(|agent| agent.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let adopted = tokio::runtime::Runtime::new()?.block_on(servers::adopt(agent, &name))?;
    let server = servers::read(&adopted)?;
    println!(
        "Copied {}'s `{name}` into the library as {adopted}.",
        agent.name
    );
    if !server.secrets.is_empty() {
        println!(
            "Its {} went into the vault.",
            server
                .secrets
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!(
        "{}'s own entry is unchanged and still holds them. To have it read from the library instead:",
        agent.name
    );
    println!(
        "  synapse mcp install {adopted} --tool {} --replace",
        agent.slug
    );
    Ok(())
}

fn secret(arguments: &[OsString]) -> Result<()> {
    let usage =
        "usage: synapse mcp secret <name> <VARIABLE>   (the value is read from a prompt or stdin)";
    let name = super::relay::text(arguments, 1, usage)?;
    let variable = super::relay::text(arguments, 2, usage)?;
    // Shown before the prompt, so a person can see which of several they are
    // being asked for.
    let value = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(format!("{variable} for {name}: "))?
    } else {
        let mut value = String::new();
        std::io::stdin().read_to_string(&mut value)?;
        value
    };
    let reference =
        tokio::runtime::Runtime::new()?.block_on(servers::setsecret(&name, &variable, &value))?;
    println!("{name} now reads {variable} from {reference}.");
    Ok(())
}

fn search(arguments: &[OsString], json: bool) -> Result<()> {
    let query = arguments
        .iter()
        .skip(1)
        .filter_map(|value| value.to_str())
        .filter(|value| !value.starts_with("--"))
        .collect::<Vec<_>>()
        .join(" ");
    anyhow::ensure!(
        !query.is_empty(),
        "usage: synapse mcp search <words> [--json]"
    );
    eprintln!("Asking every MCP source; a registry can take half a minute.");
    let folder = std::env::current_dir().ok();
    let searched = tokio::runtime::Runtime::new()?
        .block_on(servers::registry::search(&query, folder.as_deref()))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&searched)?);
        return Ok(());
    }
    for problem in &searched.problems {
        eprintln!("warning: {problem}");
    }
    if searched.found.is_empty() {
        println!("No MCP source has a server matching `{query}`.");
        return Ok(());
    }
    for listing in &searched.found {
        println!(
            "{}\t{}",
            listing.qualified(),
            headline(&listing.description)
        );
        match &listing.draft {
            Some(server) => println!("\t{}", server.commandline()),
            None => println!("\t{}", listing.note),
        }
    }
    println!();
    println!(
        "`synapse mcp get <source:id>` adds one to the library. Nothing reaches a tool until `synapse mcp install`."
    );
    Ok(())
}

fn get(arguments: &[OsString]) -> Result<()> {
    let id = super::relay::text(
        arguments,
        1,
        "usage: synapse mcp get <source:id> [--name <name>]",
    )?;
    eprintln!("Asking the source; a registry can take half a minute.");
    let folder = std::env::current_dir().ok();
    let listing = tokio::runtime::Runtime::new()?
        .block_on(servers::registry::find(&id, folder.as_deref()))?;
    let mut server = listing
        .draft
        .with_context(|| format!("`{id}` cannot be added: {}", listing.note))?;
    if let Some(name) = super::relay::value(arguments, "--name") {
        server.name = name;
    }
    servers::validname(&server.name)?;
    anyhow::ensure!(
        servers::read(&server.name).is_err(),
        "the library already has a server named `{}`; choose another with --name",
        server.name
    );
    let path = servers::save(&server)?;
    println!("Added {} at {}", server.name, path.display());
    println!("It runs: {}", server.commandline());
    for variable in server.secrets.keys() {
        println!(
            "  needs {variable}: synapse mcp secret {} {variable}",
            server.name
        );
    }
    for (variable, value) in &server.env {
        if value.is_empty() {
            println!("  needs {variable}: synapse mcp edit {}", server.name);
        }
    }
    if server
        .args
        .iter()
        .any(|arg| arg.starts_with('<') && arg.ends_with('>'))
    {
        println!(
            "  fill in the <placeholders>: synapse mcp edit {}",
            server.name
        );
    }
    println!("Then `synapse mcp install {}`.", server.name);
    Ok(())
}

fn agents() -> Result<Vec<Agent>> {
    Ok(crate::agent::agents(&crate::files::home()?))
}

fn named(arguments: &[OsString]) -> Option<String> {
    super::relay::text(arguments, 1, "")
        .ok()
        .filter(|value| !value.starts_with("--"))
}

fn chosen(arguments: &[OsString]) -> Result<Vec<Agent>> {
    super::skills::chosen(arguments)
}

fn headline(description: &str) -> String {
    let line = description.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(90) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line,
    }
}

fn report(problems: &[String]) {
    for problem in problems {
        eprintln!("warning: skipped {problem}");
    }
}
