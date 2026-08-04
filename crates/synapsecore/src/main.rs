use std::ffi::OsString;
use synapsecore::cli::{self, Outcome};

fn main() -> anyhow::Result<()> {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    match cli::run(arguments)? {
        // Where the desktop binary opens its window, this one opens the
        // dashboard in the terminal it was run from.
        Outcome::App => dashboard(),
        outcome => finish(outcome),
    }
}

#[cfg(feature = "tui")]
fn dashboard() -> anyhow::Result<()> {
    // A pipe or a log is not a screen. Entering raw mode there would corrupt
    // whatever is reading, so the answer is the text form of the same thing.
    if !synapsecore::tui::available() {
        return finish(cli::run(vec![OsString::from("status")])?);
    }
    synapsecore::tui::run()
}

#[cfg(not(feature = "tui"))]
fn dashboard() -> anyhow::Result<()> {
    finish(cli::run(vec![OsString::from("status")])?)
}

fn finish(outcome: Outcome) -> anyhow::Result<()> {
    match outcome {
        Outcome::App | Outcome::Exit(0) => Ok(()),
        Outcome::Exit(code) => std::process::exit(code),
    }
}
