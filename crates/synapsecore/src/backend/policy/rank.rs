use anyhow::Result;
use sqlx::FromRow;
use std::collections::{HashMap, HashSet};

pub(super) const WARMUP: i64 = 20;
pub(super) const ARMTRIALS: i64 = 5;
pub(super) const EXPLOREVERY: i64 = 20;
pub(super) const MINCONFIDENCE: f64 = 0.1;

#[derive(FromRow)]
pub(super) struct ArmRow {
    pub action: String,
    pub trials: i64,
    pub reward: f64,
    pub duration: i64,
}

pub(super) fn recommend(choices: &[String], arms: &HashMap<String, ArmRow>, total: i64) -> String {
    choices
        .iter()
        .max_by(|left, right| {
            score(arms.get(*left), total)
                .total_cmp(&score(arms.get(*right), total))
                .then_with(|| right.cmp(left))
        })
        .expect("choices is not empty")
        .clone()
}

fn score(arm: Option<&ArmRow>, total: i64) -> f64 {
    let Some(arm) = arm else { return f64::INFINITY };
    if arm.trials == 0 {
        return f64::INFINITY;
    }
    utility(arm) + (2.0 * ((total + 1) as f64).ln() / arm.trials as f64).sqrt()
}

pub(super) fn confidence(
    recommendation: &str,
    choices: &[String],
    arms: &HashMap<String, ArmRow>,
) -> f64 {
    let Some(best) = arms.get(recommendation).filter(|arm| arm.trials > 0) else {
        return 0.0;
    };
    let samples = best.trials;
    let best = utility(best);
    let next = choices
        .iter()
        .filter(|choice| choice.as_str() != recommendation)
        .filter_map(|choice| arms.get(choice))
        .filter(|arm| arm.trials > 0)
        .map(utility)
        .max_by(f64::total_cmp)
        .unwrap_or(0.0);
    let evidence = samples as f64 / (samples + 5) as f64;
    ((best - next) * evidence).clamp(0.0, 1.0)
}

fn utility(arm: &ArmRow) -> f64 {
    let quality = arm.reward / arm.trials as f64;
    let millis = arm.duration as f64 / arm.trials as f64;
    let latency = (millis / 60_000.0).clamp(0.0, 1.0) * 0.05;
    quality - latency
}

pub(super) fn distinct(choices: &[String]) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut clean = Vec::new();
    for choice in choices {
        validaction(choice)?;
        if seen.insert(choice) {
            clean.push(choice.clone());
        }
    }
    Ok(clean)
}

pub(super) fn validaction(value: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/')),
        "policy action contains unsupported characters"
    );
    Ok(())
}

pub(super) fn validlabel(value: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 32
            && value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "{label} contains unsupported characters"
    );
    Ok(())
}

pub(crate) fn cleancontext(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    [
        ("review", ["review", "audit", "critique"].as_slice()),
        ("test", ["test", "check", "verify"].as_slice()),
        ("docs", ["document", "readme", "docs", "writing"].as_slice()),
        ("research", ["research", "search", "investigate"].as_slice()),
        (
            "ops",
            ["deploy", "release", "server", "incident", "publish"].as_slice(),
        ),
        (
            "code",
            ["code", "build", "implement", "fix", "refactor"].as_slice(),
        ),
    ]
    .into_iter()
    .find(|(_, words)| words.iter().any(|word| value.contains(word)))
    .map(|(class, _)| class)
    .unwrap_or("default")
    .to_owned()
}

pub(super) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
