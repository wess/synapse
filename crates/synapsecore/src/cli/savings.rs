//! `synapse savings`: tokens and dollars Synapse kept out of context. The
//! counterpart to `synapse tokens`, which reports what it costs.

use crate::brain::Brain;
use crate::cli::Outcome;
use crate::usage::{Report, approximately, money, plural, thousands, tokens};
use anyhow::{Context, Result};
use std::ffi::OsString;

const USAGE: &str = "usage: synapse savings [--days <n>|--all] [--json]";

const DEFAULTDAYS: u32 = 30;

pub fn savings(arguments: &[OsString]) -> Result<Outcome> {
    let (days, json) = options(arguments)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let report = runtime.block_on(async {
        let brain = Brain::glance(crate::files::database()?).await?;
        crate::usage::report(&brain, days).await
    })?;
    match json {
        true => println!("{}", serde_json::to_string_pretty(&document(&report))?),
        false => print(&report),
    }
    Ok(Outcome::Exit(0))
}

fn options(arguments: &[OsString]) -> Result<(Option<u32>, bool)> {
    let mut days = Some(DEFAULTDAYS);
    let mut json = false;
    let mut words = arguments.iter().map(|value| value.to_string_lossy());
    while let Some(word) = words.next() {
        match word.as_ref() {
            "--json" => json = true,
            "--all" => days = None,
            "--days" => {
                let value = words.next().context(USAGE)?;
                let count: u32 = value
                    .parse()
                    .with_context(|| format!("`{value}` is not a number of days\n\n{USAGE}"))?;
                anyhow::ensure!(count > 0, "a report covers at least one day\n\n{USAGE}");
                days = Some(count);
            }
            other => anyhow::bail!("unexpected argument `{other}`\n\n{USAGE}"),
        }
    }
    Ok((days, json))
}

fn window(days: Option<u32>) -> String {
    match days {
        None => "since counting began".to_owned(),
        Some(1) => "today".to_owned(),
        Some(count) => format!("last {count} days"),
    }
}

fn print(report: &Report) {
    let totals = &report.totals;
    println!("What Synapse saved · {}", window(report.days));
    println!();
    if report.since.is_none() {
        println!("  Nothing counted yet. Open a session in a connected tool and check back.");
        return;
    }
    let net = tokens(totals.net());
    println!(
        "  {:<18} {:>10} tokens   ≈ {} at ${:.2} per million input tokens",
        "Net saved",
        approximately(net),
        money(crate::usage::dollars(net, report.price)),
        report.price
    );
    println!();
    for (label, chars, detail) in [
        (
            "Memory in reach",
            totals.reach,
            "cost of loading it all from an instruction file, per session".to_owned(),
        ),
        (
            "Handed over",
            totals.sent,
            format!(
                "recalled into context, ~{} of it read back",
                thousands(tokens(totals.readback) as usize)
            ),
        ),
        (
            "Synapse's own",
            totals.overhead,
            "tool definitions, its own guidance, response framing".to_owned(),
        ),
    ] {
        println!(
            "  {:<18} {:>10} tokens   {detail}",
            label,
            approximately(tokens(chars))
        );
    }
    println!();
    println!(
        "  {} · {} · {} · {} handed over",
        plural(totals.sessions as usize, "session", "sessions"),
        plural(totals.recalls as usize, "recall", "recalls"),
        plural(totals.reads as usize, "page read back", "pages read back"),
        plural(totals.memories as usize, "memory", "memories"),
    );
    if totals.trimmed > 0 {
        println!(
            "  Recall's budget kept ~{} tokens of matched memory out of context.",
            thousands(tokens(totals.trimmed) as usize)
        );
    }
    println!();
    if let Some(since) = &report.since {
        println!(
            "  Counting since {since} on this machine. Usage older than {} days is pruned.",
            crate::usage::RETENTIONDAYS
        );
    }
    println!(
        "  Baseline: the same memory in an instruction file every session loads.\n  \
         Tokens are characters ÷ {}, counted once per session, so treat this as a floor.",
        crate::usage::PERTOKEN
    );
}

fn document(report: &Report) -> serde_json::Value {
    let totals = &report.totals;
    let sizes = |scale: fn(i64) -> i64| {
        serde_json::json!({
            "reach": scale(totals.reach),
            "sent": scale(totals.sent),
            "trimmed": scale(totals.trimmed),
            "readback": scale(totals.readback),
            "overhead": scale(totals.overhead),
            "kept": scale(totals.kept()),
            "net": scale(totals.net()),
        })
    };
    serde_json::json!({
        "estimated": true,
        "characterspertoken": crate::usage::PERTOKEN,
        "days": report.days,
        "since": report.since,
        "price": report.price,
        "sessions": totals.sessions,
        "recalls": totals.recalls,
        "reads": totals.reads,
        "memories": totals.memories,
        "chars": sizes(|value| value),
        "tokens": sizes(tokens),
        "dollars": crate::usage::dollars(tokens(totals.net()), report.price),
        "daily": report.daily.iter().map(|day| serde_json::json!({
            "date": day.date,
            "tokens": tokens(day.net),
            "sessions": day.sessions,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_window_defaults_to_a_month_and_can_be_widened_or_narrowed() {
        assert_eq!(options(&arguments(&[])).unwrap(), (Some(30), false));
        assert_eq!(options(&arguments(&["--all"])).unwrap(), (None, false));
        assert_eq!(
            options(&arguments(&["--days", "7", "--json"])).unwrap(),
            (Some(7), true)
        );
        assert!(options(&arguments(&["--days", "0"])).is_err());
        assert!(options(&arguments(&["--days"])).is_err());
        assert!(options(&arguments(&["--weeks"])).is_err());
    }

    #[test]
    fn a_window_reads_as_words() {
        assert_eq!(window(None), "since counting began");
        assert_eq!(window(Some(1)), "today");
        assert_eq!(window(Some(7)), "last 7 days");
    }
}
