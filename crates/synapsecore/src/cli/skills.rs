//! Managing the skill library and getting it into the tools that read it.

use crate::agent::Agent;
use crate::cli::Outcome;
use crate::cli::editor::editor;
use crate::skill::{self, Receipts};
use anyhow::{Context, Result};
use std::ffi::OsString;

const USAGE: &str =
    "usage: synapse skill <list|show|create|edit|delete|install|remove|status|adopt>";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = super::relay::text(arguments, 0, USAGE)?;
    let json = arguments.iter().any(|value| value == "--json");
    match action.as_str() {
        "list" => list(json)?,
        "show" => show(arguments)?,
        "create" => create(arguments)?,
        "edit" => edit(arguments)?,
        "delete" => delete(arguments)?,
        "install" => install(arguments)?,
        "remove" => remove(arguments)?,
        "status" => status(arguments, json)?,
        "adopt" => adopt(arguments)?,
        unknown => anyhow::bail!("unknown skill command `{unknown}`\n\n{USAGE}"),
    }
    Ok(Outcome::Exit(0))
}

fn list(json: bool) -> Result<()> {
    let (skills, problems) = skill::library::all()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&skills)?);
    } else {
        for skill in skills {
            println!(
                "{}\t{}\t{}",
                skill.name,
                skill.files.len(),
                headline(&skill.description)
            );
        }
    }
    report(&problems);
    Ok(())
}

fn show(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill show <name>")?;
    let root = skill::library::path(&name)?;
    print!(
        "{}",
        std::fs::read_to_string(root.join(skill::ENTRY))
            .with_context(|| format!("no skill named `{name}` in the library"))?
    );
    Ok(())
}

fn create(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill create <name>")?;
    let root = skill::library::create(&name)?;
    println!("Created {}", root.join(skill::ENTRY).display());
    println!("Edit it, then run `synapse skill install {name}`.");
    Ok(())
}

fn edit(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill edit <name>")?;
    let root = skill::library::path(&name)?;
    let entry = root.join(skill::ENTRY);
    let current = std::fs::read_to_string(&entry)
        .with_context(|| format!("no skill named `{name}` in the library"))?;
    let draft = std::env::temp_dir().join(format!("synapse-skill-{name}.md"));
    std::fs::write(&draft, &current)?;

    let status = std::process::Command::new(editor())
        .arg(&draft)
        .status()
        .context("could not open your editor")?;
    anyhow::ensure!(status.success(), "the editor exited without saving");

    let edited = std::fs::read_to_string(&draft)?;
    match skill::library::save(&name, &edited) {
        Ok(_) => {
            let _ = std::fs::remove_file(&draft);
            println!("Saved {}", entry.display());
            println!("Run `synapse skill install {name}` to push it to your tools.");
            Ok(())
        }
        Err(error) => {
            Err(error).with_context(|| format!("your draft is still at {}", draft.display()))
        }
    }
}

fn delete(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill delete <name> --confirm")?;
    anyhow::ensure!(
        arguments.iter().any(|value| value == "--confirm"),
        "add --confirm to delete `{name}` from the library"
    );
    let root = skill::library::delete(&name)?;
    println!("Deleted {}", root.display());
    println!("Copies already installed in your tools are left alone.");
    Ok(())
}

fn install(arguments: &[OsString]) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let agents = chosen(arguments)?;
    let replace = arguments.iter().any(|value| value == "--replace");
    let (skills, problems) = skill::library::all()?;
    let wanted = super::relay::text(arguments, 1, "").ok();
    let wanted = wanted.filter(|value| !value.starts_with("--"));

    let selected: Vec<_> = match &wanted {
        Some(name) => skills.iter().filter(|skill| &skill.name == name).collect(),
        None => skills.iter().collect(),
    };
    anyhow::ensure!(
        !selected.is_empty(),
        "no skill named `{}` in the library",
        wanted.unwrap_or_default()
    );

    let mut failures = Vec::new();
    for agent in &agents {
        for skill in &selected {
            match runtime.block_on(skill::install(&receipts, agent, skill, replace)) {
                Ok(_) => println!("{} → {}", skill.name, agent.name),
                Err(error) => failures.push(format!("{} → {}: {error:#}", skill.name, agent.name)),
            }
        }
    }
    report(&problems);
    for failure in &failures {
        eprintln!("warning: {failure}");
    }
    anyhow::ensure!(
        failures.is_empty(),
        "{} skill install(s) did not happen",
        failures.len()
    );
    Ok(())
}

fn remove(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(
        arguments,
        1,
        "usage: synapse skill remove <name> [--tool <tool>]",
    )?;
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let force = arguments.iter().any(|value| value == "--force");
    for agent in chosen(arguments)? {
        match runtime.block_on(skill::remove(&receipts, &agent, &name, force)) {
            Ok(true) => println!("Removed {} from {}", name, agent.name),
            Ok(false) => println!("{} has no {}", agent.name, name),
            Err(error) => eprintln!("warning: {} → {}: {error:#}", name, agent.name),
        }
    }
    Ok(())
}

fn status(arguments: &[OsString], json: bool) -> Result<()> {
    let home = crate::files::home()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let (statuses, problems) = runtime.block_on(skill::survey(&home))?;
    let (skills, _) = skill::library::all()?;
    let known: Vec<String> = skills.iter().map(|skill| skill.name.clone()).collect();

    if json {
        let extra: Vec<_> = crate::agent::agents(&home)
            .into_iter()
            .map(|agent| {
                serde_json::json!({
                    "tool": agent.name,
                    "unknown": skill::unknown(&agent, &known),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "skills": statuses,
                "unmanaged": extra,
            }))?
        );
        report(&problems);
        return Ok(());
    }

    let wanted = super::relay::text(arguments, 1, "").ok();
    let wanted = wanted.filter(|value| !value.starts_with("--"));
    for entry in &statuses {
        if wanted.as_ref().is_some_and(|name| name != &entry.skill) {
            continue;
        }
        println!("{}\t{}\t{}", entry.skill, entry.tool, entry.state.label());
    }
    for agent in crate::agent::agents(&home) {
        for name in skill::unknown(&agent, &known) {
            println!("{name}\t{}\tnot in the library", agent.name);
        }
    }
    report(&problems);
    Ok(())
}

fn adopt(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(
        arguments,
        1,
        "usage: synapse skill adopt <name> --tool <tool>",
    )?;
    let agents = chosen(arguments)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let mut adopted = false;
    for agent in &agents {
        match runtime.block_on(skill::adopt(&receipts, agent, &name)) {
            Ok(path) => {
                println!(
                    "Copied {} from {} into {}",
                    name,
                    agent.name,
                    path.display()
                );
                adopted = true;
                break;
            }
            Err(error) if agents.len() == 1 => return Err(error),
            Err(_) => continue,
        }
    }
    anyhow::ensure!(adopted, "no connected tool has a skill named `{name}`");
    println!("Run `synapse skill install {name}` to put it everywhere.");
    Ok(())
}

/// Which tools a command applies to. Everything by default, or one named with
/// `--tool`.
fn chosen(arguments: &[OsString]) -> Result<Vec<Agent>> {
    let home = crate::files::home()?;
    let agents = crate::agent::agents(&home);
    let Some(wanted) = super::relay::value(arguments, "--tool") else {
        return Ok(agents);
    };
    let wanted = wanted.to_lowercase();
    let matched: Vec<Agent> = agents
        .into_iter()
        .filter(|agent| {
            agent.command.eq_ignore_ascii_case(&wanted)
                || agent.name.to_lowercase() == wanted
                || agent.name.to_lowercase().replace(' ', "") == wanted.replace(' ', "")
        })
        .collect();
    anyhow::ensure!(
        !matched.is_empty(),
        "unknown tool `{wanted}`; use claude, codex, or pi"
    );
    Ok(matched)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headline_is_one_trimmed_line() {
        assert_eq!(
            headline("Does a thing.\nAnd more."),
            "Does a thing. And more."
        );
        let long = headline(&"word ".repeat(50));
        assert!(long.ends_with('…'));
        assert!(long.chars().count() <= 91);
    }

    #[test]
    fn a_tool_can_be_named_by_command_or_label() {
        let arguments: Vec<OsString> = ["install", "--tool", "claude"]
            .iter()
            .map(OsString::from)
            .collect();
        let selected = chosen(&arguments).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].command, "claude");

        let labelled: Vec<OsString> = ["install", "--tool", "Claude Code"]
            .iter()
            .map(OsString::from)
            .collect();
        assert_eq!(chosen(&labelled).unwrap().len(), 1);
    }

    #[test]
    fn every_tool_is_the_default_and_an_unknown_one_is_refused() {
        let bare: Vec<OsString> = vec![OsString::from("install")];
        assert_eq!(chosen(&bare).unwrap().len(), 3);

        let unknown: Vec<OsString> = ["install", "--tool", "emacs"]
            .iter()
            .map(OsString::from)
            .collect();
        assert!(chosen(&unknown).is_err());
    }
}
