use std::ffi::OsString;
use synapsecore::cli::{self, Outcome};

fn main() -> anyhow::Result<()> {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    match cli::run(arguments)? {
        Outcome::App => finish(cli::run(vec![OsString::from("status")])?),
        outcome => finish(outcome),
    }
}

fn finish(outcome: Outcome) -> anyhow::Result<()> {
    match outcome {
        Outcome::App | Outcome::Exit(0) => Ok(()),
        Outcome::Exit(code) => std::process::exit(code),
    }
}
