//! `synapse tokens`: what Synapse costs a session before the first turn.
//!
//! Recall has a budget. The guidance, the tool definitions, and every skill
//! description don't, and they're paid once per session per connected tool
//! whether or not a memory is ever recalled. This counts them so the settings
//! that move them (optimization, mesh, learn) have a number beside them. It
//! changes nothing. The Stats page and `doctor` show the same measurement.

use crate::brain::Brain;
use crate::cli::Outcome;
use crate::usage::{Surface, thousands};
use anyhow::Result;
use std::ffi::OsString;

const PERTOKEN: usize = crate::usage::PERTOKEN as usize;

pub fn tokens(arguments: &[OsString]) -> Result<Outcome> {
    let json = arguments.iter().any(|value| value == "--json");
    let runtime = tokio::runtime::Runtime::new()?;
    let lines = runtime.block_on(measure())?;

    let total: usize = lines.iter().map(|line| line.chars).sum();
    if json {
        let body = serde_json::json!({
            "estimated": true,
            "characterspertoken": PERTOKEN,
            "total": { "chars": total, "tokens": total / PERTOKEN },
            "lines": lines.iter().map(|line| serde_json::json!({
                "label": line.label,
                "detail": line.detail,
                "chars": line.chars,
                "tokens": line.tokens(),
                "warning": line.warning,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(Outcome::Exit(0));
    }

    println!("What Synapse costs a session, before the first turn");
    println!();
    for line in &lines {
        println!(
            "  {:<16} {:>7} chars  ~{:>6} tokens   {}",
            line.label,
            thousands(line.chars),
            thousands(line.tokens()),
            line.detail
        );
    }
    println!("  {}", "─".repeat(62));
    println!(
        "  {:<16} {:>7} chars  ~{:>6} tokens   per session, per connected tool",
        "Total",
        thousands(total),
        thousands(total / PERTOKEN)
    );
    println!();
    println!("  Token figures are characters ÷ {PERTOKEN}, not a tokenizer. Counts are exact.");

    let warnings: Vec<&Surface> = lines.iter().filter(|line| line.warning.is_some()).collect();
    if !warnings.is_empty() {
        println!();
        for line in warnings {
            if let Some(warning) = &line.warning {
                println!("  ! {warning}");
            }
        }
    }
    Ok(Outcome::Exit(0))
}

/// Measured from the current folder, so the startup recall includes the
/// project's memory when run inside one.
pub async fn measure() -> Result<Vec<Surface>> {
    let database = crate::files::database()?;
    let soul = crate::files::soul()?;
    let brain = Brain::glance(&database).await?;
    let root = std::env::current_dir()
        .ok()
        .and_then(|current| crate::brain::projectroot(&current).ok().flatten());
    crate::usage::surfaces(&brain, &soul, root.as_deref()).await
}
