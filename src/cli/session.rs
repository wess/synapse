//! What a connected tool shows the user about Synapse.
//!
//! `session` answers a Claude Code SessionStart hook. Its `systemMessage` is
//! displayed in the terminal at startup, beside the welcome box, which is the
//! only way to state the connection before the model has written anything. The
//! same call also tells the model the notice has already been shown, so the
//! line does not appear twice for a tool that has both the hook and the
//! guidance pointer.
//!
//! `statusline` answers Claude Code's `statusLine` command and prints the one
//! line under the prompt for the rest of the session.

use crate::cli::Outcome;
use anyhow::Result;
use serde::Serialize;
use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Report {
    pub connected: bool,
    pub memories: i64,
    pub project: Option<String>,
    pub mesh: bool,
    pub agents: usize,
    pub vault: String,
    /// Set instead of the rest when Synapse could not be read at all.
    pub problem: Option<String>,
}

pub fn session(arguments: &[OsString]) -> Result<Outcome> {
    let root = folder(&stdin());
    let report = collect(root.as_deref());
    if arguments.iter().any(|value| value == "--json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(Outcome::Exit(0));
    }
    let payload = serde_json::json!({
        "systemMessage": notice(&report),
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context(&report),
        }
    });
    println!("{}", serde_json::to_string(&payload)?);
    Ok(Outcome::Exit(0))
}

pub fn statusline(_arguments: &[OsString]) -> Result<Outcome> {
    let input = stdin();
    let report = collect(folder(&input).as_deref());
    println!("{}", line(&input, &report));
    Ok(Outcome::Exit(0))
}

/// The one line shown to the user at startup. Reports what is actually there,
/// including when that is nothing.
pub fn notice(report: &Report) -> String {
    if let Some(problem) = &report.problem {
        return format!("Synapse unavailable · {problem}");
    }
    let mut parts = vec![match report.memories {
        0 => "Synapse connected · no memories yet".to_owned(),
        1 => "Synapse connected · 1 memory".to_owned(),
        count => format!("Synapse connected · {count} memories"),
    }];
    if report.vault != "inactive" {
        parts.push(format!("vault {}", report.vault));
    }
    if report.mesh {
        parts.push(match report.agents {
            0 => "mesh on".to_owned(),
            1 => "mesh · 1 agent".to_owned(),
            count => format!("mesh · {count} agents"),
        });
    }
    parts.join(" · ")
}

/// What the model is told. Deliberately short, and it exists mainly to stop the
/// model repeating a notice the user has already seen in the terminal.
fn context(report: &Report) -> String {
    if let Some(problem) = &report.problem {
        return format!(
            "Synapse could not be reached this session: {problem}. Do not claim a connection that is not there."
        );
    }
    let scope = report
        .project
        .as_deref()
        .map(|project| format!(" for {project}"))
        .unwrap_or_default();
    format!(
        "Synapse is connected and holds {} memories{scope}. The Synapse session hook has already \
         shown the user a connection notice in their terminal, so do not print a `Synapse \
         connected` line yourself. Still call `recall` before you start work.",
        report.memories
    )
}

fn line(input: &serde_json::Value, report: &Report) -> String {
    let mut parts = Vec::new();
    if let Some(model) = input
        .pointer("/model/display_name")
        .and_then(serde_json::Value::as_str)
    {
        parts.push(model.to_owned());
    }
    if let Some(folder) = folder(input)
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
    {
        parts.push(folder.to_owned());
    }
    parts.push(match &report.problem {
        Some(_) => "◆ Synapse unavailable".to_owned(),
        None => match report.mesh {
            true => format!("◆ Synapse {} · mesh {}", report.memories, report.agents),
            false => format!("◆ Synapse {}", report.memories),
        },
    });
    parts.join(" · ")
}

/// Read everything a report needs in one pass, turning any failure into a
/// reportable problem rather than a non-zero exit: a hook that fails is noise in
/// the user's terminal, and a status line that fails leaves a blank bar.
fn collect(root: Option<&Path>) -> Report {
    match gather(root) {
        Ok(report) => report,
        Err(error) => Report {
            connected: false,
            memories: 0,
            project: None,
            mesh: false,
            agents: 0,
            vault: "inactive".to_owned(),
            problem: Some(shortened(&format!("{error:#}"))),
        },
    }
}

fn gather(root: Option<&Path>) -> Result<Report> {
    let database = crate::files::database()?;
    let runtime = tokio::runtime::Runtime::new()?;
    // Reporting only, so none of these opens reads the whole store to find out
    // whether it is sound: a status line redraws on every turn and has to stay
    // far cheaper than the work it sits beside.
    runtime.block_on(async {
        let brain = crate::brain::Brain::glance(&database).await?;
        let memories = brain.reach(root).await?;
        let mesh = brain.mesh().await?;
        let agents = if mesh {
            crate::relay::Mesh::glance(&database)
                .await?
                .agents()
                .await?
                .into_iter()
                .filter(|agent| agent.online)
                .count()
        } else {
            0
        };
        let vault = match root {
            Some(root) => {
                let vaults = crate::vault::VaultStore::glance(&database).await?;
                let resolved = crate::vault::resolve(&vaults, root).await?;
                if resolved.scopes.is_empty() {
                    "inactive"
                } else if resolved.warnings.is_empty() {
                    "ready"
                } else {
                    "blocked"
                }
            }
            None => "inactive",
        };
        let project = root
            .map(crate::brain::projectroot)
            .transpose()?
            .flatten()
            .map(|path| path.display().to_string());
        Ok(Report {
            connected: true,
            memories,
            project,
            mesh,
            agents,
            vault: vault.to_owned(),
            problem: None,
        })
    })
}

/// The JSON the calling tool writes on stdin, or nothing when it wrote none.
fn stdin() -> serde_json::Value {
    if std::io::stdin().is_terminal() {
        return serde_json::Value::Null;
    }
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return serde_json::Value::Null;
    }
    serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null)
}

/// Where the calling session is working. Claude Code sends both `cwd` and
/// `workspace.current_dir`; either is fine, and this process's own folder is the
/// fallback.
fn folder(input: &serde_json::Value) -> Option<PathBuf> {
    ["/workspace/current_dir", "/cwd", "/workspace/project_dir"]
        .iter()
        .find_map(|pointer| input.pointer(pointer).and_then(serde_json::Value::as_str))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("SYNAPSE_PROJECT_DIR").map(Into::into))
        .or_else(|| std::env::current_dir().ok())
}

/// Keep a failure to one readable clause: this ends up on a single line in
/// someone's terminal.
fn shortened(message: &str) -> String {
    let first = message.lines().next().unwrap_or(message).trim();
    match first.char_indices().nth(80) {
        Some((at, _)) => format!("{}…", &first[..at]),
        None => first.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            connected: true,
            memories: 128,
            project: Some("/work/api".to_owned()),
            mesh: false,
            agents: 0,
            vault: "inactive".to_owned(),
            problem: None,
        }
    }

    #[test]
    fn the_notice_counts_memories_and_stays_one_line() {
        let notice = notice(&report());
        assert_eq!(notice, "Synapse connected · 128 memories");
        assert!(!notice.contains('\n'));
    }

    #[test]
    fn an_empty_store_says_so_rather_than_reporting_zero() {
        let notice = notice(&Report {
            memories: 0,
            ..report()
        });
        assert_eq!(notice, "Synapse connected · no memories yet");
    }

    #[test]
    fn one_memory_is_not_pluralized() {
        assert!(
            notice(&Report {
                memories: 1,
                ..report()
            })
            .ends_with("1 memory")
        );
    }

    #[test]
    fn the_mesh_and_vault_appear_only_when_they_are_doing_something() {
        let quiet = notice(&report());
        assert!(!quiet.contains("mesh"));
        assert!(!quiet.contains("vault"));

        let busy = notice(&Report {
            mesh: true,
            agents: 3,
            vault: "ready".to_owned(),
            ..report()
        });
        assert_eq!(
            busy,
            "Synapse connected · 128 memories · vault ready · mesh · 3 agents"
        );
    }

    #[test]
    fn a_failure_is_reported_rather_than_claimed_as_a_connection() {
        let broken = Report {
            problem: Some("could not open brain.db".to_owned()),
            connected: false,
            ..report()
        };
        assert_eq!(
            notice(&broken),
            "Synapse unavailable · could not open brain.db"
        );
        assert!(context(&broken).contains("Do not claim a connection"));
    }

    #[test]
    fn the_model_is_told_the_user_has_already_seen_the_notice() {
        let context = context(&report());
        assert!(context.contains("do not print a `Synapse connected` line yourself"));
        assert!(context.contains("128 memories"));
        assert!(context.contains("/work/api"));
        assert!(context.contains("call `recall`"));
    }

    #[test]
    fn the_status_line_leads_with_the_model_and_folder() {
        let input = serde_json::json!({
            "model": {"display_name": "Opus 5"},
            "workspace": {"current_dir": "/work/api"}
        });
        assert_eq!(line(&input, &report()), "Opus 5 · api · ◆ Synapse 128");
    }

    #[test]
    fn the_status_line_still_prints_without_any_input() {
        let line = line(&serde_json::Value::Null, &report());
        assert!(line.contains("◆ Synapse 128"));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn a_long_failure_is_trimmed_to_one_clause() {
        let long = format!("{}\nsecond line", "x".repeat(200));
        let short = shortened(&long);
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= 81);
        assert!(!short.contains('\n'));
    }

    #[test]
    fn the_working_folder_comes_from_the_calling_tool_when_it_sends_one() {
        let input = serde_json::json!({"cwd": "/work/api"});
        assert_eq!(folder(&input), Some(PathBuf::from("/work/api")));
        assert!(folder(&serde_json::Value::Null).is_some());
    }
}
