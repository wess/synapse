mod options;

use crate::brain::{Brain, drift::Finding};
use crate::cli::Outcome;
use crate::learning::{Disposition, Feedback};
use anyhow::{Context, Result};
use options::Options;
use serde::Deserialize;
use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::time::Duration;

const USAGE: &str = "usage: synapse learn <report|list|show|check|confirm|reject|proposals|proposal|approve|dismiss|cases|review|eval|resolve|resolutions|restore> [--project <folder>]";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resolve {
    finding: Finding,
    disposition: Disposition,
    note: String,
}

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let options = Options::parse(arguments)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let brain = Brain::glance(crate::files::database()?).await?;
        let folder = &options.project;
        match options.action.as_str() {
            "report" => {
                let input: Feedback = input()?;
                let session = options
                    .value("--session")
                    .context("report needs --session <id>")?;
                printjson(&brain.feedback(input, folder, session).await?)?;
            }
            "list" => printjson(&brain.feedbackrecords(folder, 100).await?)?,
            "show" => printjson(&brain.feedbackrecord(options.id()?, folder).await?)?,
            "check" => {
                let timeout = options.value("--timeout").unwrap_or("120").parse::<u64>()?;
                let check = brain
                    .checkfeedback(
                        options.id()?,
                        folder,
                        &options.command,
                        Duration::from_secs(timeout),
                    )
                    .await?;
                let ok = !check.timedout && check.exit == Some(0);
                printjson(&check)?;
                return Ok(Outcome::Exit(if ok { 0 } else { 1 }));
            }
            "confirm" | "reject" => {
                options.confirm()?;
                printjson(
                    &brain
                        .confirmfeedback(options.id()?, folder, options.action == "confirm")
                        .await?,
                )?;
            }
            "proposals" => {
                let proposals = brain.learningproposals(folder).await?;
                let summaries: Vec<_> = proposals.iter().map(|proposal| serde_json::json!({
                    "id":proposal.id,"skill":proposal.skill,"origin":proposal.origin,
                    "feedback":proposal.feedback,"state":proposal.state,"created":proposal.created,
                })).collect();
                printjson(&summaries)?;
            }
            "proposal" => printjson(&brain.reviewlearning(options.id()?, folder).await?)?,
            "approve" => {
                options.confirm()?;
                let approval = brain
                    .approvelearning(
                        options.id()?,
                        folder,
                        &crate::files::home()?,
                        options.has("--global"),
                    )
                    .await?;
                printjson(&approval)?;
            }
            "dismiss" => {
                options.confirm()?;
                brain.rejectlearning(options.id()?, folder).await?;
                println!("Suggestion dismissed.");
            }
            "cases" => printjson(&brain.learningcases(folder).await?)?,
            "review" => {
                options.confirm()?;
                anyhow::ensure!(
                    options.has("--accept") != options.has("--reject"),
                    "review needs exactly one of --accept or --reject"
                );
                brain
                    .reviewlearningcase(options.id()?, folder, options.has("--accept"))
                    .await?;
                println!("Regression case reviewed.");
            }
            "eval" => {
                let limit = options.value("--limit").unwrap_or("4").parse()?;
                let budget = options.value("--budget").map(str::parse).transpose()?;
                let floor = options
                    .value("--fail-under")
                    .map(str::parse::<f64>)
                    .transpose()?;
                anyhow::ensure!(
                    floor.is_none_or(|n| n.is_finite() && (0.0..=1.0).contains(&n)),
                    "--fail-under needs a number between 0 and 1"
                );
                let evaluation = brain.evaluatelearning(folder, limit, budget).await?;
                if options.has("--json") {
                    printjson(&evaluation)?;
                } else {
                    print!("{}", super::eval::report(&evaluation));
                }
                return Ok(Outcome::Exit(
                    if floor.is_some_and(|n| evaluation.found < n) {
                        1
                    } else {
                        0
                    },
                ));
            }
            "resolve" => {
                options.confirm()?;
                let input: Resolve = input()?;
                printjson(
                    &brain
                        .resolvefinding(folder, &input.finding, input.disposition, &input.note)
                        .await?,
                )?;
            }
            "resolutions" => printjson(&brain.driftresolutions(folder).await?)?,
            "restore" => {
                options.confirm()?;
                anyhow::ensure!(
                    brain.restoreresolution(options.id()?, folder).await?,
                    "resolution does not exist in this project"
                );
                println!("Drift warning restored.");
            }
            _ => anyhow::bail!(USAGE),
        }
        Ok(Outcome::Exit(0))
    })
}

fn input<T: serde::de::DeserializeOwned>() -> Result<T> {
    anyhow::ensure!(!std::io::stdin().is_terminal(), "read JSON from stdin");
    let mut raw = Vec::new();
    std::io::stdin().take(65_537).read_to_end(&mut raw)?;
    anyhow::ensure!(raw.len() <= 65_536, "learning input exceeds 64 KiB");
    serde_json::from_slice(&raw).map_err(|error| {
        anyhow::anyhow!(
            "invalid learning JSON at line {} column {}",
            error.line(),
            error.column(),
        )
    })
}

fn printjson(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
