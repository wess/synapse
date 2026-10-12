//! `synapse memory eval`: scoring recall against a file of labeled questions.
//!
//! The command behind the claim that a change to search made it better. It
//! exits non-zero under `--fail-under`, so a store and a cases file kept beside
//! a project can hold the line in CI the way a test suite does.

use crate::brain::{Brain, Evaluation};
use crate::cli::Outcome;
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "usage: synapse memory eval <cases.toml> [--limit <n>] [--budget <full|balanced|lean>] [--fail-under <0-1>] [--json]";

/// How many memories each question may bring back when `--limit` is not
/// given: what a focused `recall` asks for.
const LIMIT: u32 = 5;

pub fn run(
    runtime: &tokio::runtime::Runtime,
    brain: &Brain,
    arguments: &[OsString],
    json: bool,
) -> Result<Outcome> {
    let path = file(arguments).context(USAGE)?;
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let cases = crate::brain::cases(&raw)?;
    let limit = match value(arguments, "--limit") {
        Some(limit) => limit.parse().context(USAGE)?,
        None => LIMIT,
    };
    let budget = value(arguments, "--budget")
        .map(|budget| budget.parse().context(USAGE))
        .transpose()?;
    let floor: Option<f64> = value(arguments, "--fail-under")
        .map(|floor| floor.parse().context(USAGE))
        .transpose()?;

    let evaluation = runtime.block_on(brain.evaluate(&cases, limit, budget))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&evaluation)?);
    } else {
        print!("{}", report(&evaluation));
    }
    match floor {
        Some(floor) if evaluation.found < floor => {
            eprintln!(
                "found {:.0}% of answers, under the floor of {:.0}%",
                evaluation.found * 100.0,
                floor * 100.0
            );
            Ok(Outcome::Exit(1))
        }
        _ => Ok(Outcome::Exit(0)),
    }
}

fn report(evaluation: &Evaluation) -> String {
    let answerable = evaluation
        .cases
        .iter()
        .filter(|case| case.answerable)
        .count();
    let unanswerable = evaluation.cases.len() - answerable;
    let mut text = format!(
        "Cases:   {} ({answerable} with an answer, {unanswerable} without)\n\
         Limit:   {}\n",
        evaluation.cases.len(),
        evaluation.limit
    );
    if answerable > 0 {
        text.push_str(&format!(
            "First:   {:.0}%  answer ranked first\n\
             Found:   {:.0}%  answer returned at all\n\
             MRR:     {:.2}\n\
             Shown:   {:.0}%  answer text inside what recall returned\n",
            evaluation.top * 100.0,
            evaluation.found * 100.0,
            evaluation.mrr,
            evaluation.shown * 100.0
        ));
    }
    if unanswerable > 0 {
        text.push_str(&format!(
            "Noise:   {:.1}  memories returned where none answer\n",
            evaluation.noise
        ));
    }
    text.push_str("\nrank\treturned\tquery\n");
    for case in &evaluation.cases {
        let rank = match (case.answerable, case.rank) {
            (false, _) => "-".to_owned(),
            (true, Some(rank)) => rank.to_string(),
            (true, None) => "miss".to_owned(),
        };
        text.push_str(&format!("{rank}\t{}\t{}\n", case.returned, case.query));
    }
    text
}

/// The first argument after `eval` that is neither a flag nor a flag's value.
fn file(arguments: &[OsString]) -> Option<PathBuf> {
    let mut skip = false;
    for argument in arguments.iter().skip(1) {
        if skip {
            skip = false;
            continue;
        }
        let text = argument.to_str()?;
        if text == "--limit" || text == "--fail-under" || text == "--budget" {
            skip = true;
            continue;
        }
        if text.starts_with("--") {
            continue;
        }
        return Some(PathBuf::from(argument));
    }
    None
}

fn value(arguments: &[OsString], flag: &str) -> Option<String> {
    arguments
        .iter()
        .position(|argument| argument == flag)
        .and_then(|index| arguments.get(index + 1))
        .and_then(|argument| argument.to_str())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::Scored;

    #[test]
    fn the_cases_file_is_found_around_the_flags() {
        let arguments: Vec<OsString> = ["eval", "--limit", "3", "cases.toml", "--json"]
            .iter()
            .map(OsString::from)
            .collect();
        assert_eq!(file(&arguments), Some(PathBuf::from("cases.toml")));
        assert_eq!(value(&arguments, "--limit").as_deref(), Some("3"));
    }

    #[test]
    fn a_miss_and_a_case_without_an_answer_read_differently() {
        let evaluation = Evaluation {
            limit: 5,
            cases: vec![
                Scored {
                    query: "deploy branch".into(),
                    answerable: true,
                    rank: None,
                    shown: false,
                    returned: 2,
                },
                Scored {
                    query: "wifi password".into(),
                    answerable: false,
                    rank: None,
                    shown: false,
                    returned: 1,
                },
            ],
            top: 0.0,
            found: 0.0,
            mrr: 0.0,
            shown: 0.0,
            noise: 1.0,
        };
        let text = report(&evaluation);
        assert!(text.contains("miss\t2\tdeploy branch"));
        assert!(text.contains("-\t1\twifi password"));
        assert!(text.contains("Noise:   1.0"));
    }
}
