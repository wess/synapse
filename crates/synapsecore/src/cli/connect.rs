//! `synapse connect` — the inverse of `synapse disconnect`, and the reason a
//! machine with no window can still wire a tool in.
//!
//! Connecting used to live only in the desktop, which made the window the one
//! place a tool could be set up. That was survivable while the original built-in
//! tools were the only ones there were; it stopped being survivable when
//! a person could describe their own, because they could write a descriptor from
//! the terminal and then have nowhere to act on it.

use crate::agent::Agent;
use crate::cli::Outcome;
use anyhow::Result;
use std::ffi::OsString;

const USAGE: &str = "usage: synapse connect [tool] [--refresh|--reset]";

/// What connecting should do about a tool that is already connected.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Leave a working registration alone and bring the rest up to date.
    Connect,
    /// Write the registration again, for a descriptor that moved in a release.
    Refresh,
    /// Take the connection out and make it again.
    Reset,
}

pub fn connect(arguments: &[OsString]) -> Result<Outcome> {
    let flag = |name: &str| {
        arguments
            .iter()
            .any(|value| value.to_string_lossy() == format!("--{name}"))
    };
    let mode = match (flag("refresh"), flag("reset")) {
        (true, true) => anyhow::bail!("--refresh and --reset ask for different things\n\n{USAGE}"),
        (true, false) => Mode::Refresh,
        (false, true) => Mode::Reset,
        (false, false) => Mode::Connect,
    };
    let wanted = arguments
        .iter()
        .find(|value| !value.to_string_lossy().starts_with("--"))
        .map(|value| value.to_string_lossy().to_lowercase());
    let home = crate::files::home()?;
    let server = crate::cli::destination()?;
    let soul = crate::files::soul()?;
    let database = crate::files::database()?;
    let agents = crate::agent::agents(&home);
    let known: Vec<String> = agents.iter().map(|agent| agent.slug.clone()).collect();

    let chosen: Vec<Agent> = match &wanted {
        Some(name) => agents
            .into_iter()
            .filter(|agent| matches(agent, name))
            .collect(),
        // No name connects everything this machine actually has, which is the
        // one-command setup somebody arriving from the website expects.
        None => agents
            .into_iter()
            .filter(|agent| {
                crate::agent::detect(agent, Some(&server))
                    .executable
                    .is_some()
            })
            .collect(),
    };
    anyhow::ensure!(
        !chosen.is_empty(),
        "no tool matches `{}`; this machine has {}\n\n{USAGE}",
        wanted.unwrap_or_default(),
        known.join(", ")
    );

    let runtime = tokio::runtime::Runtime::new()?;
    let mut failed = false;
    for agent in &chosen {
        let detection = crate::agent::detect(agent, Some(&server));
        if detection.executable.is_none() {
            println!("· {} is not installed or is not on PATH", agent.name);
            failed = true;
            continue;
        }
        // Refreshing or resetting something that was never connected is just
        // connecting it, and saying so is friendlier than refusing over a flag.
        let wired = detection.registered && detection.configured;
        let outcome = runtime.block_on(async {
            match mode {
                Mode::Refresh if wired => {
                    crate::agent::refresh(agent, &detection, &server, &soul, &database).await
                }
                Mode::Reset if wired => crate::agent::reset(agent, &server, &soul, &database)
                    .await
                    .map(|removed| {
                        for problem in &removed.problems {
                            println!("· {problem}");
                        }
                    }),
                // Already wired in is not a failure and not a no-op worth
                // hiding: the guidance pointer and the notice are still brought
                // up to date.
                _ => crate::agent::connect(agent, &detection, &server, &soul, &database).await,
            }
        });
        match outcome {
            Ok(()) => println!("{}", done(mode, wired, &agent.name)),
            Err(error) => {
                println!("· could not connect {}: {error:#}", agent.name);
                failed = true;
            }
        }
    }
    Ok(Outcome::Exit(i32::from(failed)))
}

/// What to say afterwards. A refresh and a reset are things a person asked for
/// specifically, so reporting either as "is connected" would leave them
/// wondering whether the flag did anything.
fn done(mode: Mode, wired: bool, name: &str) -> String {
    match mode {
        Mode::Refresh if wired => format!("✓ {name} is connected, from this release's descriptor."),
        Mode::Reset if wired => format!("✓ {name} was disconnected and connected again."),
        _ => format!("✓ {name} is connected to Synapse."),
    }
}

/// The slug first, then the binary, then the display name — so `hermes`,
/// `claude`, and `claude code` all land where somebody would expect.
fn matches(agent: &Agent, name: &str) -> bool {
    agent.slug == name
        || agent.command == name
        || agent.name.to_lowercase() == name
        || agent.name.to_lowercase().replace(' ', "") == name.replace(' ', "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_tool_is_found_by_slug_binary_or_name() {
        let agent = crate::agent::tool::resolve(Path::new("/users/test"), None, "claude")
            .unwrap()
            .unwrap();

        assert!(matches(&agent, "claude"));
        assert!(matches(&agent, "claude code"));
        assert!(matches(&agent, "claudecode"));
        assert!(!matches(&agent, "codex"));
    }
}
