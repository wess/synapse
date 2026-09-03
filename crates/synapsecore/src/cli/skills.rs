//! Managing the skill library and getting it into the tools that read it.
//!
//! Two things here are not obvious from the command names. A skill an agent
//! wrote is *proposed*: it is in the library and in no tool, and a bulk
//! `install` steps over it, because the gate on self-improvement is install and
//! not write. And every command that takes a bare name resolves this project's
//! shelf before the global one, so a repository's own `release` shadows the
//! shared one instead of colliding with it.

use crate::agent::Agent;
use crate::cli::Outcome;
use crate::cli::editor::editor;
use crate::skill::{self, Receipts, Shelf, Skill};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "usage: synapse skill <list|show|create|edit|delete|install|remove|status|adopt|proposed|approve|reject|history|revert>";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = super::relay::text(arguments, 0, USAGE)?;
    let json = arguments.iter().any(|value| value == "--json");
    match action.as_str() {
        "list" => list(arguments, json)?,
        "show" => show(arguments)?,
        "create" => create(arguments)?,
        "edit" => edit(arguments)?,
        "delete" => delete(arguments)?,
        "install" => install(arguments)?,
        "remove" => remove(arguments)?,
        "status" => status(arguments, json)?,
        "adopt" => adopt(arguments)?,
        "proposed" => proposed(json)?,
        "approve" => approve(arguments)?,
        "reject" => reject(arguments)?,
        "history" => history(arguments, json)?,
        "revert" => revert(arguments)?,
        unknown => anyhow::bail!("unknown skill command `{unknown}`\n\n{USAGE}"),
    }
    Ok(Outcome::Exit(0))
}

fn list(arguments: &[OsString], json: bool) -> Result<()> {
    let (skills, problems) = skill::library::all()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let waiting = runtime.block_on(receipts.proposals())?;
    let wanted = shelf(arguments)?;

    let selected: Vec<&Skill> = skills
        .iter()
        .filter(|skill| wanted.as_ref().is_none_or(|shelf| &skill.shelf == shelf))
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&selected)?);
    } else {
        for skill in selected {
            let proposed = waiting
                .iter()
                .any(|item| item.shelf == skill.shelf.key() && item.skill == skill.name);
            println!(
                "{}\t{}\t{}\t{}{}",
                skill.name,
                skill.shelf.label(),
                skill.files.len(),
                if proposed { "proposed · " } else { "" },
                headline(&skill.description)
            );
        }
    }
    report(&problems);
    Ok(())
}

fn show(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill show <name>")?;
    let skill = find(arguments, &name)?;
    let root = skill::library::path(&skill.shelf, &skill.name)?;
    print!("{}", std::fs::read_to_string(root.join(skill::ENTRY))?);
    Ok(())
}

fn create(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill create <name>")?;
    // A skill somebody types out by hand is global unless they say otherwise.
    // The volume of project skills comes from agents, and they pass a scope.
    let shelf = shelf(arguments)?.unwrap_or(Shelf::Global);
    let root = skill::library::create(&shelf, &name)?;
    println!("Created {}", root.join(skill::ENTRY).display());
    println!("Edit it, then run `synapse skill install {name}`.");
    Ok(())
}

fn edit(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill edit <name>")?;
    let skill = find(arguments, &name)?;
    let entry = skill::library::path(&skill.shelf, &skill.name)?.join(skill::ENTRY);
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
    match skill::library::save(&skill.shelf, &name, &edited) {
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
    let skill = find(arguments, &name)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let root = skill::library::delete(&skill.shelf, &name)?;
    runtime.block_on(receipts.forgethistory(skill.shelf.key(), &name))?;
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
    let waiting = runtime.block_on(receipts.proposals())?;
    let wanted = named(arguments);

    let selected: Vec<&Skill> = match &wanted {
        Some(name) => {
            let skill = find(arguments, name)?;
            anyhow::ensure!(
                !waiting
                    .iter()
                    .any(|item| item.shelf == skill.shelf.key() && item.skill == skill.name),
                "`{name}` is waiting for review; approve it with `synapse skill approve {name}`"
            );
            skills
                .iter()
                .filter(|item| item.name == skill.name && item.shelf == skill.shelf)
                .collect()
        }
        // Everything means everything approved. A proposal reaching a tool
        // because somebody ran a bulk install is the one way this gate leaks.
        None => skills
            .iter()
            .filter(|skill| {
                !waiting
                    .iter()
                    .any(|item| item.shelf == skill.shelf.key() && item.skill == skill.name)
            })
            .collect(),
    };
    anyhow::ensure!(
        !selected.is_empty(),
        "no skill named `{}` in the library",
        wanted.unwrap_or_default()
    );

    let mut failures = Vec::new();
    for agent in &agents {
        for skill in &selected {
            if skill::target(agent, &skill.shelf, &skill.name).is_none() {
                continue;
            }
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
    // Resolved from the library when it is still there, and from the global
    // shelf when it is not: a skill deleted from the library still has copies.
    let key = find(arguments, &name)
        .map(|skill| skill.shelf.key().to_owned())
        .unwrap_or_default();
    for agent in chosen(arguments)? {
        match runtime.block_on(skill::remove(&receipts, &agent, &key, &name, force)) {
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

    let wanted = named(arguments);
    for entry in &statuses {
        if wanted.as_ref().is_some_and(|name| name != &entry.skill) {
            continue;
        }
        println!(
            "{}\t{}\t{}\t{}",
            entry.skill,
            entry.scope,
            entry.tool,
            match entry.proposed {
                true => "waiting for review",
                false => entry.state.label(),
            }
        );
    }
    for agent in crate::agent::agents(&home) {
        for name in skill::unknown(&agent, &known) {
            println!("{name}\tglobal\t{}\tnot in the library", agent.name);
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

/// What agents have written and nobody has looked at. The queue is the whole
/// user-facing half of self-improvement: Synapse accumulates and waits.
fn proposed(json: bool) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let waiting = runtime.block_on(receipts.proposals())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&waiting)?);
        return Ok(());
    }
    if waiting.is_empty() {
        println!("Nothing is waiting for review.");
        return Ok(());
    }
    for item in &waiting {
        println!(
            "{}\t{}\t{}\t{}",
            item.skill,
            item.scope,
            item.tool,
            headline(&item.note)
        );
        if !item.project.is_empty() {
            println!("\t{}", item.project);
        }
    }
    println!();
    println!("`synapse skill show <name>` to read one, then `approve` or `reject`.");
    Ok(())
}

fn approve(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill approve <name>")?;
    let skill = find(arguments, &name)?;
    let agents = chosen(arguments)?;
    let replace = arguments.iter().any(|value| value == "--replace");
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let results = runtime.block_on(skill::approve(&receipts, &agents, &skill, replace))?;
    anyhow::ensure!(
        !results.is_empty(),
        "no connected tool can hold a {} skill",
        skill.shelf.label()
    );
    for (tool, outcome) in results {
        match outcome {
            Ok(_) => println!("{name} → {tool}"),
            Err(error) => eprintln!("warning: {name} → {tool}: {error:#}"),
        }
    }
    Ok(())
}

fn reject(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill reject <name> --confirm")?;
    anyhow::ensure!(
        arguments.iter().any(|value| value == "--confirm"),
        "add --confirm to turn down `{name}`"
    );
    let skill = find(arguments, &name)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let path = runtime.block_on(skill::reject(&receipts, &skill))?;
    println!("Removed {}", path.display());
    Ok(())
}

fn history(arguments: &[OsString], json: bool) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill history <name>")?;
    let skill = find(arguments, &name)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let history = runtime.block_on(receipts.revisions(skill.shelf.key(), &skill.name))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&history)?);
        return Ok(());
    }
    if history.is_empty() {
        println!("`{name}` has never been revised.");
        return Ok(());
    }
    for revision in &history {
        println!(
            "{}\t{}\t{}",
            revision.id,
            match revision.tool.is_empty() {
                true => "you",
                false => &revision.tool,
            },
            headline(&revision.note)
        );
    }
    println!();
    println!("`synapse skill revert {name} [<id>]` puts one of these back.");
    Ok(())
}

fn revert(arguments: &[OsString]) -> Result<()> {
    let name = super::relay::text(arguments, 1, "usage: synapse skill revert <name> [<id>]")?;
    let skill = find(arguments, &name)?;
    let id = super::relay::text(arguments, 2, "")
        .ok()
        .filter(|value| !value.starts_with("--"))
        .map(|value| value.parse::<i64>())
        .transpose()
        .context("a revision id is a number from `synapse skill history`")?;
    let home = crate::files::home()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let receipts = runtime.block_on(Receipts::open(crate::files::database()?))?;
    let (kept, reached) = runtime.block_on(skill::revert(&receipts, &home, &skill, id))?;
    println!("Reverted {name}; what it said is kept as revision {kept}.");
    match reached.is_empty() {
        true => println!("No tool was holding a copy Synapse could update."),
        false => println!("Updated in {}.", reached.join(", ")),
    }
    Ok(())
}

/// The shelf a command was told to use, or `None` when it should work it out
/// from the name.
fn shelf(arguments: &[OsString]) -> Result<Option<Shelf>> {
    if arguments.iter().any(|value| value == "--global") {
        return Ok(Some(Shelf::Global));
    }
    let mut found = arguments.iter().skip_while(|value| *value != "--project");
    let Some(_) = found.next() else {
        return Ok(None);
    };
    let root = found
        .next()
        .map(PathBuf::from)
        .filter(|value| !value.to_string_lossy().starts_with("--"))
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)
        .context("could not determine the project folder")?;
    Ok(Some(Shelf::project(&root)))
}

/// The skill a bare name means: the shelf the caller named, or this project's
/// copy before the global one.
fn find(arguments: &[OsString], name: &str) -> Result<Skill> {
    match shelf(arguments)? {
        Some(shelf) => skill::library::read(&shelf, name),
        None => skill::library::locate(name, std::env::current_dir().ok().as_deref()),
    }
}

/// The positional name a command was given, if it was given one.
fn named(arguments: &[OsString]) -> Option<String> {
    super::relay::text(arguments, 1, "")
        .ok()
        .filter(|value| !value.starts_with("--"))
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
    let known: Vec<String> = agents.iter().map(|agent| agent.slug.clone()).collect();
    let matched: Vec<Agent> = agents
        .into_iter()
        .filter(|agent| {
            agent.slug.eq_ignore_ascii_case(&wanted)
                || agent.command.eq_ignore_ascii_case(&wanted)
                || agent.name.to_lowercase() == wanted
                || agent.name.to_lowercase().replace(' ', "") == wanted.replace(' ', "")
        })
        .collect();
    anyhow::ensure!(
        !matched.is_empty(),
        "unknown tool `{wanted}`; this machine has {}",
        known.join(", ")
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

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

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
        let selected = chosen(&arguments(&["install", "--tool", "claude"])).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].command, "claude");

        assert_eq!(
            chosen(&arguments(&["install", "--tool", "Claude Code"]))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn every_tool_is_the_default_and_an_unknown_one_is_refused() {
        assert_eq!(chosen(&arguments(&["install"])).unwrap().len(), 4);
        assert!(chosen(&arguments(&["install", "--tool", "emacs"])).is_err());
    }

    #[test]
    fn a_shelf_is_named_explicitly_or_worked_out_from_the_name() {
        assert!(shelf(&arguments(&["list"])).unwrap().is_none());
        assert_eq!(
            shelf(&arguments(&["list", "--global"])).unwrap(),
            Some(Shelf::Global)
        );

        let directory = tempfile::tempdir().unwrap();
        let named = shelf(&arguments(&[
            "create",
            "mine",
            "--project",
            &directory.path().display().to_string(),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(named.label(), "project");

        // `--project` with nothing after it means the folder you are in, not
        // the flag that follows it.
        let bare = shelf(&arguments(&["create", "mine", "--project", "--json"]))
            .unwrap()
            .unwrap();
        assert_eq!(
            bare.root().unwrap(),
            std::fs::canonicalize(std::env::current_dir().unwrap())
                .unwrap()
                .display()
                .to_string()
        );
    }

    #[test]
    fn a_positional_name_is_never_a_flag() {
        assert_eq!(named(&arguments(&["install", "mine"])), Some("mine".into()));
        assert_eq!(named(&arguments(&["install", "--json"])), None);
        assert_eq!(named(&arguments(&["install"])), None);
    }
}
