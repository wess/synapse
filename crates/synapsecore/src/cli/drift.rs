//! `synapse memory drift`: the drift check over everything a folder can see,
//! with the command that fixes each finding. Read-only. `--strict` exits 1 when
//! anything is found, for scripts and CI.

use crate::brain::Brain;
use crate::brain::drift::{Concern, Drift, Finding, SCANCEILING};
use crate::cli::Outcome;
use crate::usage::thousands;
use anyhow::Result;
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "usage: synapse memory drift [folder] [--json] [--strict]";

pub fn run(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    arguments: &[OsString],
    json: bool,
) -> Result<Outcome> {
    let (folder, strict) = options(arguments)?;
    let drift = runtime.block_on(brain.drift(Some(&folder)))?;
    match json {
        true => println!("{}", serde_json::to_string_pretty(&drift)?),
        false => print!("{}", report(&drift)),
    }
    Ok(Outcome::Exit(match strict && !drift.findings.is_empty() {
        true => 1,
        false => 0,
    }))
}

fn options(arguments: &[OsString]) -> Result<(PathBuf, bool)> {
    let mut folder = None;
    let mut strict = false;
    for argument in arguments.iter().skip(1) {
        match argument.to_string_lossy().as_ref() {
            "--json" => {}
            "--strict" => strict = true,
            flag if flag.starts_with('-') => anyhow::bail!("unexpected `{flag}`\n\n{USAGE}"),
            _ if folder.is_some() => anyhow::bail!("one folder at a time\n\n{USAGE}"),
            _ => folder = Some(PathBuf::from(argument)),
        }
    }
    let folder = match folder {
        Some(folder) => folder,
        None => std::env::current_dir()?,
    };
    Ok((folder, strict))
}

fn report(drift: &Drift) -> String {
    let scope = match (&drift.project, &drift.root) {
        (Some(project), Some(root)) => format!(" visible from {project} ({})", root.display()),
        (Some(project), None) => format!(" visible from {project}"),
        _ => " stored globally".to_owned(),
    };
    let counted = match drift.scanned {
        1 => "1 memory".to_owned(),
        count => format!("{} memories", thousands(count)),
    };
    let mut text = String::new();
    if drift.findings.is_empty() {
        text.push_str(&format!("No drift in {counted}{scope}.\n"));
    } else {
        text.push_str(&format!("Drift in {counted}{scope}\n\n"));
        for finding in &drift.findings {
            text.push_str(&entry(finding));
        }
        text.push_str(&format!(
            "\n{}. Nothing was changed.\n",
            match drift.findings.len() {
                1 => "1 finding".to_owned(),
                count => format!("{count} findings"),
            }
        ));
    }
    if drift.capped {
        text.push_str(&format!(
            "Only the newest {} were read.\n",
            thousands(SCANCEILING as usize)
        ));
    }
    if drift.partial {
        text.push_str(
            "Similar-memory comparisons hit a resource limit; findings may be incomplete.\n",
        );
    }
    if drift.root.is_none() {
        text.push_str(
            "No checkout here, so paths relative to a project were not checked. \
             Run this from inside one to check them.\n",
        );
    }
    text
}

fn entry(finding: &Finding) -> String {
    let (label, settle) = match (finding.concern, finding.ids.as_slice()) {
        (Concern::Stale, [id, ..]) => (
            "stale",
            format!(
                "update or remove it: synapse memory edit {id} · synapse memory delete {id} --confirm"
            ),
        ),
        (Concern::Conflict, [older, newer, ..]) => (
            "conflict",
            format!("if #{newer} holds: synapse memory supersede {older} {newer}"),
        ),
        (Concern::Duplicate, [older, newer, ..]) => (
            "duplicate",
            format!("keep one: synapse memory supersede {older} {newer}"),
        ),
        _ => ("", String::new()),
    };
    format!("  {label:<10} {}\n  {:<10} {settle}\n", finding.note, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(concern: Concern, ids: Vec<i64>, note: &str) -> Finding {
        Finding {
            concern,
            ids,
            note: note.to_owned(),
            missing: Vec::new(),
        }
    }

    #[test]
    fn every_finding_comes_with_the_command_that_settles_it() {
        let drift = Drift {
            root: Some(PathBuf::from("/work/app")),
            project: Some("github.com/acme/app".into()),
            scanned: 12,
            capped: false,
            partial: false,
            findings: vec![
                finding(
                    Concern::Stale,
                    vec![7],
                    "#7 names `a.rs`, which is no longer there.",
                ),
                finding(
                    Concern::Conflict,
                    vec![3, 4],
                    "#3 says `always` where #4 says `never`.",
                ),
                finding(
                    Concern::Duplicate,
                    vec![1, 2],
                    "#1 and #2 say nearly the same thing.",
                ),
            ],
        };
        let text = report(&drift);
        assert!(
            text.starts_with("Drift in 12 memories visible from github.com/acme/app (/work/app)")
        );
        assert!(text.contains("synapse memory edit 7 · synapse memory delete 7 --confirm"));
        assert!(text.contains("if #4 holds: synapse memory supersede 3 4"));
        assert!(text.contains("keep one: synapse memory supersede 1 2"));
        assert!(text.contains("3 findings. Nothing was changed."));
        assert!(!text.contains("No checkout here"));
    }

    #[test]
    fn a_clean_store_and_a_partial_read_both_say_so() {
        let drift = Drift {
            root: None,
            project: None,
            scanned: 1,
            capped: true,
            partial: false,
            findings: Vec::new(),
        };
        let text = report(&drift);
        assert!(
            text.starts_with("No drift in 1 memory stored globally."),
            "{text}"
        );
        assert!(text.contains("Only the newest 5,000 were read."));
        assert!(text.contains("No checkout here"));
    }

    #[test]
    fn a_folder_and_strict_are_read_and_anything_else_is_refused() {
        let arguments = |words: &[&str]| -> Vec<OsString> {
            std::iter::once("drift")
                .chain(words.iter().copied())
                .map(OsString::from)
                .collect()
        };
        assert_eq!(
            options(&arguments(&["/work/app", "--strict", "--json"])).unwrap(),
            (PathBuf::from("/work/app"), true)
        );
        assert!(!options(&arguments(&[])).unwrap().1);
        assert!(options(&arguments(&["--fix"])).is_err());
        assert!(options(&arguments(&["/a", "/b"])).is_err());
    }
}
