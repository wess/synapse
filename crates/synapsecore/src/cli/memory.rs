use crate::brain::{Brain, MemoryScope};
use crate::cli::Outcome;
use crate::imports::{ImportPreview, ImportProvider, ImportStatus};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

const USAGE: &str = "usage: synapse memory <list|grep|show|add|edit|supersede|restore|import|imports|undo|delete|wipe>";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(arguments, 0, USAGE)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let brain = runtime.block_on(Brain::open(crate::files::database()?))?;
    let json = has(before(arguments), "--json");
    match action.as_str() {
        "list" => list(&runtime, &brain, arguments, json)?,
        "grep" => grep(&runtime, &brain, arguments, json)?,
        "show" => show(&runtime, &brain, arguments, json)?,
        "add" => add(&runtime, &brain, arguments)?,
        "edit" => edit(&runtime, &brain, arguments)?,
        "supersede" => supersede(&runtime, &brain, arguments)?,
        "restore" => restore(&runtime, &brain, arguments)?,
        "import" => import(&runtime, &brain, arguments, json)?,
        "imports" => imports(&runtime, &brain, json)?,
        "undo" => undo(&runtime, &brain, arguments)?,
        "delete" => delete(&runtime, &brain, arguments)?,
        "wipe" => wipe(&runtime, &brain, arguments)?,
        _ => anyhow::bail!("unknown memory command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn list(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    arguments: &[OsString],
    json: bool,
) -> Result<()> {
    let query = words(arguments, &["--json", "--explain"]);
    if has(arguments, "--explain") {
        return explain(runtime, brain, &query, json);
    }
    let memories = runtime.block_on(brain.search(&query, 100))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&memories)?);
        return Ok(());
    }
    for memory in memories {
        println!("{}", row(&memory));
    }
    Ok(())
}

fn grep(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    arguments: &[OsString],
    json: bool,
) -> Result<()> {
    let pattern = words(arguments, &["--json"]);
    anyhow::ensure!(
        !pattern.trim().is_empty(),
        "usage: synapse memory grep <text> [--json]"
    );
    let memories = runtime.block_on(brain.grep(&pattern, 100))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&memories)?);
        return Ok(());
    }
    for memory in memories {
        println!("{}", row(&memory));
    }
    Ok(())
}

/// What the search actually did, for the case where the answer is wrong and
/// nothing on screen says why.
fn explain(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    query: &str,
    json: bool,
) -> Result<()> {
    let explanation = runtime.block_on(brain.explain(query, 100))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&explanation)?);
        return Ok(());
    }
    println!("Query:      {}", explanation.query);
    println!("Mode:       {}", explanation.mode());
    match &explanation.expression {
        Some(expression) => println!("Expression: {expression}"),
        None => println!(
            "Expression: none — no term left to search for, so the newest memories answered instead"
        ),
    }
    if !explanation.kept.is_empty() {
        println!("Searched:   {}", explanation.kept.join(", "));
    }
    if !explanation.dropped.is_empty() {
        println!(
            "Dropped:    {} (matches nearly every memory)",
            explanation.dropped.join(", ")
        );
    }
    println!("Matches:    {}", explanation.matches.len());
    println!();
    for hit in &explanation.matches {
        let score = hit
            .rank
            .map(|rank| format!("{rank:.4}"))
            .unwrap_or_else(|| "-".to_owned());
        println!("{score}\t{}", row(&hit.memory));
    }
    Ok(())
}

fn supersede(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    arguments: &[OsString],
) -> Result<()> {
    const USAGE: &str = "usage: synapse memory supersede <old> <new>";
    let old = memoryid(arguments, 1, USAGE)?;
    let new = memoryid(arguments, 2, USAGE)?;
    anyhow::ensure!(
        runtime.block_on(brain.supersede(old, new))?,
        "memory #{old} not found"
    );
    println!("Memory #{old} superseded by #{new}; recall now returns #{new} instead");
    println!("Undo with: synapse memory restore {old}");
    Ok(())
}

fn restore(runtime: &tokio::runtime::Runtime, brain: &Brain, arguments: &[OsString]) -> Result<()> {
    let id = memoryid(arguments, 1, "usage: synapse memory restore <id>")?;
    anyhow::ensure!(
        runtime.block_on(brain.unsupersede(id))?,
        "memory #{id} is not superseded"
    );
    println!("Restored memory #{id}");
    Ok(())
}

/// One memory as a line: id, scope, source, and enough body to recognise it.
fn row(memory: &crate::brain::Memory) -> String {
    let scope = if memory.scope == MemoryScope::Global {
        "global".to_owned()
    } else {
        format!("project:{}", memory.project)
    };
    let body = if memory.superseded == 0 {
        bodypreview(&memory.body)
    } else {
        format!(
            "(superseded by #{}) {}",
            memory.superseded,
            bodypreview(&memory.body)
        )
    };
    format!(
        "{}\t{}\t{}\t{}",
        memory.id,
        scope,
        if memory.source.is_empty() {
            "-"
        } else {
            &memory.source
        },
        body
    )
}

/// The positional words of a subcommand, with the named flags taken out.
///
/// A bare `--` ends the flags: everything after it is the text being searched
/// for, which is what lets `memory grep -- --no-verify` mean the flag rather
/// than a flag.
fn words(arguments: &[OsString], flags: &[&str]) -> String {
    let rest = arguments.get(1..).unwrap_or_default();
    if let Some(at) = rest.iter().position(|value| value == "--") {
        return join(rest[at + 1..].iter());
    }
    join(
        rest.iter()
            .filter(|value| !flags.iter().any(|flag| value == flag)),
    )
}

fn join<'a>(values: impl Iterator<Item = &'a OsString>) -> String {
    values
        .map(|value| value.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Arguments up to a bare `--`. A flag written after the separator is part of
/// the text being searched for, not a flag.
fn before(arguments: &[OsString]) -> &[OsString] {
    match arguments.iter().position(|value| value == "--") {
        Some(at) => &arguments[..at],
        None => arguments,
    }
}

fn show(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    arguments: &[OsString],
    json: bool,
) -> Result<()> {
    let id = memoryid(arguments, 1, "usage: synapse memory show <id> [--json]")?;
    let memory = runtime
        .block_on(brain.memory(id))?
        .context("memory not found")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&memory)?);
    } else {
        println!("Memory #{}", memory.id);
        println!("Scope: {}", memory.scope.value());
        if !memory.project.is_empty() {
            println!("Project: {}", memory.project);
        }
        println!("Source: {}", memory.source);
        println!("Created: {}", memory.created);
        if memory.superseded != 0 {
            println!(
                "Superseded by: #{} (not recalled; restore with `synapse memory restore {}`)",
                memory.superseded, memory.id
            );
        }
        println!();
        println!("{}", memory.body);
    }
    Ok(())
}

fn add(runtime: &tokio::runtime::Runtime, brain: &Brain, arguments: &[OsString]) -> Result<()> {
    let (scope, project) = memoryscope(arguments)?;
    let source = positional(arguments, 1, &["--global", "--project"]);
    let body = readmemory()?;
    let id = runtime.block_on(brain.rememberscoped(
        &body,
        source.as_deref(),
        scope,
        project.as_deref(),
    ))?;
    println!("Stored memory #{id}");
    Ok(())
}

fn edit(runtime: &tokio::runtime::Runtime, brain: &Brain, arguments: &[OsString]) -> Result<()> {
    let id = memoryid(arguments, 1, "usage: synapse memory edit <id> [source]")?;
    let source = arguments.get(2).and_then(|value| value.to_str());
    let body = readmemory()?;
    runtime
        .block_on(brain.updatememory(id, &body, source))?
        .context("memory not found")?;
    println!("Updated memory #{id}");
    Ok(())
}

fn import(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    arguments: &[OsString],
    json: bool,
) -> Result<()> {
    let provider: ImportProvider = textarg(
        arguments,
        1,
        "usage: synapse memory import <claude|codex|markdown> [path] [--confirm]",
    )?
    .parse()?;
    let candidates = if provider == ImportProvider::Markdown {
        let path = PathBuf::from(textarg(
            arguments,
            2,
            "usage: synapse memory import markdown <path> [--confirm]",
        )?);
        crate::imports::scanmarkdown(&path)?
    } else {
        let home = crate::files::home()?;
        runtime.block_on(crate::imports::scan(&home, provider))?
    };
    let preview = runtime.block_on(brain.importpreview(provider, candidates))?;
    if !has(arguments, "--confirm") {
        printpreview(&preview, json)?;
        if !json {
            println!("Preview only. Add --confirm to import safe entries.");
        }
        return Ok(());
    }
    let includeflagged = has(arguments, "--include-flagged");
    let report = runtime.block_on(brain.importmemories(preview, includeflagged))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "Import batch #{}: {} stored, {} linked, {} already imported, {} flagged and left untouched",
            report.batch.id, report.stored, report.linked, report.skipped, report.flagged
        );
        println!(
            "Undo with: synapse memory undo {} --confirm",
            report.batch.id
        );
    }
    Ok(())
}

fn imports(runtime: &tokio::runtime::Runtime, brain: &Brain, json: bool) -> Result<()> {
    let batches = runtime.block_on(brain.importbatches())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&batches)?);
        return Ok(());
    }
    for batch in batches {
        println!(
            "{}\t{}\t{} stored\t{} linked\t{}",
            batch.id,
            batch.provider,
            batch.imported,
            batch.linked,
            if batch.undone { "undone" } else { "active" }
        );
    }
    Ok(())
}

fn undo(runtime: &tokio::runtime::Runtime, brain: &Brain, arguments: &[OsString]) -> Result<()> {
    let id = memoryid(arguments, 1, "usage: synapse memory undo <batch> --confirm")?;
    anyhow::ensure!(
        has(arguments, "--confirm"),
        "add --confirm to undo import batch {id}"
    );
    let deleted = runtime.block_on(brain.undoimport(id))?;
    println!("Undid import batch #{id}; removed {deleted} imported memories");
    Ok(())
}

fn delete(runtime: &tokio::runtime::Runtime, brain: &Brain, arguments: &[OsString]) -> Result<()> {
    let id = memoryid(arguments, 1, "usage: synapse memory delete <id> --confirm")?;
    anyhow::ensure!(
        has(arguments, "--confirm"),
        "add --confirm to delete memory #{id}"
    );
    runtime
        .block_on(brain.deletememory(id))?
        .context("memory not found")?;
    println!("Deleted memory #{id}");
    Ok(())
}

fn wipe(runtime: &tokio::runtime::Runtime, brain: &Brain, arguments: &[OsString]) -> Result<()> {
    anyhow::ensure!(
        has(arguments, "--confirm"),
        "add --confirm to delete every memory"
    );
    let count = runtime.block_on(brain.wipememories())?;
    println!("Deleted {count} memories");
    Ok(())
}

fn memoryscope(arguments: &[OsString]) -> Result<(MemoryScope, Option<PathBuf>)> {
    anyhow::ensure!(
        !(has(arguments, "--global") && has(arguments, "--project")),
        "choose either --global or --project"
    );
    if has(arguments, "--global") {
        return Ok((MemoryScope::Global, None));
    }
    let project = optionvalue(arguments, "--project")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    Ok((MemoryScope::Project, Some(project)))
}

fn printpreview(preview: &ImportPreview, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(preview)?);
        return Ok(());
    }
    println!(
        "{}: {} found · {} ready · {} existing · {} flagged",
        preview.provider.name(),
        preview.found,
        preview.ready,
        preview.existing,
        preview.flagged
    );
    for item in &preview.items {
        let marker = match item.status {
            ImportStatus::Ready => "ready",
            ImportStatus::Existing => "existing",
            ImportStatus::Flagged => "review",
        };
        println!("{marker}\t{}\t{}", item.path, item.preview);
        if let Some(warning) = &item.warning {
            println!("\twarning: {warning}");
        }
    }
    Ok(())
}

fn readmemory() -> Result<String> {
    anyhow::ensure!(
        !std::io::stdin().is_terminal(),
        "pipe the replacement memory content to stdin"
    );
    let mut value = String::new();
    std::io::stdin().read_to_string(&mut value)?;
    anyhow::ensure!(!value.trim().is_empty(), "memory content cannot be empty");
    Ok(value)
}

fn positional(arguments: &[OsString], start: usize, options: &[&str]) -> Option<String> {
    let mut skip = false;
    for value in arguments.iter().skip(start) {
        if skip {
            skip = false;
            continue;
        }
        let value = value.to_str()?;
        if value == "--project" {
            skip = true;
            continue;
        }
        if options.contains(&value) || value.starts_with("--") {
            continue;
        }
        return Some(value.to_owned());
    }
    None
}

fn optionvalue(arguments: &[OsString], option: &str) -> Option<String> {
    arguments
        .iter()
        .position(|value| value == option)
        .and_then(|index| arguments.get(index + 1))
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
}

fn memoryid(arguments: &[OsString], index: usize, usage: &str) -> Result<i64> {
    textarg(arguments, index, usage)?
        .parse()
        .with_context(|| usage.to_owned())
}

fn bodypreview(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = compact.chars();
    let preview = characters.by_ref().take(80).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn has(arguments: &[OsString], flag: &str) -> bool {
    arguments.iter().any(|value| value == flag)
}

fn textarg(arguments: &[OsString], index: usize, usage: &str) -> Result<String> {
    arguments
        .get(index)
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .with_context(|| usage.to_owned())
}
