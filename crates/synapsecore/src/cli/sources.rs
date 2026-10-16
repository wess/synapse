//! `synapse source`: where skill and MCP searches look.

use crate::cli::Outcome;
use crate::sources::{self, Kind, Origin};
use anyhow::Result;
use std::ffi::OsString;

const USAGE: &str = "usage: synapse source <list|add|remove|allow|refresh>";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = super::relay::text(arguments, 0, USAGE)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let folder = std::env::current_dir().ok();
    match action.as_str() {
        "list" => {
            let (all, problems) = runtime.block_on(sources::resolved(folder.as_deref()))?;
            if arguments.iter().any(|value| value == "--json") {
                println!("{}", serde_json::to_string_pretty(&all)?);
                return Ok(Outcome::Exit(0));
            }
            for source in &all {
                let holds = match (source.skills(), source.servers()) {
                    (true, true) => "skills, MCP",
                    (true, false) => "skills",
                    _ => "MCP",
                };
                let layer = match &source.origin {
                    Origin::Builtin => "built in".to_owned(),
                    Origin::User => "yours".to_owned(),
                    Origin::Project { trusted: true, .. } => "this project".to_owned(),
                    Origin::Project { trusted: false, .. } => {
                        "this project, not approved — `synapse source allow`".to_owned()
                    }
                };
                println!(
                    "{}\t{}\t{holds}\t{}\t{layer}",
                    source.name,
                    source.kind.label(),
                    source.location()
                );
            }
            for problem in problems {
                eprintln!("warning: {problem}");
            }
        }
        "add" => {
            let usage = "usage: synapse source add <name> <owner/repo | address> [--kind skills-index|mcp-registry|github] [--token <vault.name>]";
            let name = super::relay::text(arguments, 1, usage)?;
            let location = super::relay::text(arguments, 2, usage)?;
            let kind = match super::relay::value(arguments, "--kind").as_deref() {
                None => None,
                Some("skills-index") => Some(Kind::SkillsIndex),
                Some("mcp-registry") => Some(Kind::McpRegistry),
                Some("github") => Some(Kind::Github),
                Some(other) => anyhow::bail!("`{other}` is not a kind of source\n\n{usage}"),
            };
            let mut source = runtime.block_on(sources::infer(&name, &location, kind))?;
            if let Some(token) = super::relay::value(arguments, "--token") {
                source.token = token;
            }
            let path = sources::add(source.clone())?;
            println!(
                "Added {} ({}, {}) to {}",
                source.name,
                source.kind.label(),
                source.location(),
                path.display()
            );
            if source.kind == Kind::Github && source.token.is_empty() {
                println!(
                    "A private repository needs a token: `--token <vault.name>` names one in the vault."
                );
            }
        }
        "remove" => {
            let name = super::relay::text(arguments, 1, "usage: synapse source remove <name>")?;
            let path = sources::remove(&name)?;
            println!("Removed {name} from {}", path.display());
        }
        "allow" => {
            let folder =
                folder.ok_or_else(|| anyhow::anyhow!("could not tell which folder this is"))?;
            let path = runtime.block_on(sources::allow(&folder))?;
            println!("Approved {} as it reads now.", path.display());
            println!("Editing it takes the approval away until it is allowed again.");
        }
        "refresh" => {
            let wanted = super::relay::text(arguments, 1, "").ok();
            let (all, _) = runtime.block_on(sources::resolved(folder.as_deref()))?;
            let mut any = false;
            for source in all.iter().filter(|source| {
                source.kind == Kind::Github
                    && source.usable()
                    && wanted.as_ref().is_none_or(|name| *name == source.name)
            }) {
                any = true;
                match runtime.block_on(sources::github::refresh(source)) {
                    Ok((skills, servers)) => {
                        println!("{}: {skills} skill(s), {servers} server(s)", source.name)
                    }
                    Err(error) => eprintln!("warning: {}: {error:#}", source.name),
                }
            }
            if !any {
                println!(
                    "No repository source to refresh; indexes and registries are asked fresh every time."
                );
            }
        }
        unknown => anyhow::bail!("unknown source command `{unknown}`\n\n{USAGE}"),
    }
    Ok(Outcome::Exit(0))
}
