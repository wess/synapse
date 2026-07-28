use crate::cli::Outcome;
use anyhow::{Context, Result};
use std::ffi::OsString;

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = arguments
        .first()
        .and_then(|value| value.to_str())
        .context("usage: synapse guidance <show|sync|adopt>")?;
    let home = crate::files::home()?;
    let soul = crate::files::soul()?;
    match action {
        "show" => {
            let state = crate::agent::guidancestate(&home, &soul);
            if arguments.iter().any(|value| value == "--json") {
                println!("{}", serde_json::to_string_pretty(&state)?);
            } else {
                println!("SOUL.md\t{}", state.path.display());
                println!("exists\t{}", state.exists);
                println!("pointers\t{}/{}", state.synced, state.total);
                println!("consolidated\t{}", state.consolidated);
            }
        }
        "sync" => {
            let report = crate::agent::sync(&home, &soul)?;
            println!(
                "Shared guidance ready at {}; refreshed {} pointers",
                report.path.display(),
                report.files.len()
            );
        }
        "adopt" => {
            anyhow::ensure!(
                arguments.iter().any(|value| value == "--confirm"),
                "add --confirm to move existing global guidance into SOUL.md"
            );
            let report = crate::agent::adopt(&home, &soul)?;
            println!(
                "Consolidated {} guidance source(s) into {}; replaced {} global files with managed pointers",
                report.moved,
                report.path.display(),
                report.files.len()
            );
        }
        _ => anyhow::bail!("unknown guidance command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}
