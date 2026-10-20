//! Measuring whether recall returns what it should.
//!
//! Changes to search have been judged by reading a few results and deciding
//! they looked better, which cannot tell a real improvement from a lucky
//! query. This replays a fixed set of questions against the store, through
//! the same scoped recall an agent gets, and scores where the memory that
//! answers each one landed.
//!
//! The labels are written by a person. Synapse runs no model, and a model
//! grading its own retrieval is a second thing to measure before the first
//! can be trusted. A case names the text a useful memory contains rather than
//! its id, so one file keeps working across machines, re-imports, and
//! corrections that replace a memory with a new one.

use crate::brain::{Brain, Optimization};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The file `synapse memory eval` reads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cases {
    #[serde(rename = "case", default)]
    pub cases: Vec<Case>,
}

/// One question and what a useful answer to it contains.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub query: String,
    /// The folder the question is asked from, as an agent would pass it.
    /// Absent asks from no project, which recalls global memory alone.
    #[serde(default)]
    pub project: Option<PathBuf>,
    /// Text a useful memory contains, matched case-insensitively. Any one is
    /// enough. Empty means nothing stored answers this question, and the best
    /// recall can do is return nothing.
    #[serde(default)]
    pub expect: Vec<String>,
}

/// Where one case's answer landed.
#[derive(Debug, Clone, Serialize)]
pub struct Scored {
    pub query: String,
    /// Whether this case has an answer to find.
    pub answerable: bool,
    /// The 1-based position of the first useful memory, if any came back.
    pub rank: Option<usize>,
    /// Whether the useful text itself was in what recall returned, after the
    /// response budget shortened it. A memory ranked first whose answer was
    /// cut away is a memory the agent has to read again to use.
    pub shown: bool,
    /// How many memories came back.
    pub returned: usize,
    /// Memory characters actually handed over, after the recall budget.
    pub characters: usize,
    /// Recall latency, excluding the evaluation's label lookup.
    pub micros: u64,
}

/// The whole run.
#[derive(Debug, Clone, Serialize)]
pub struct Evaluation {
    /// How many memories each recall was allowed to return.
    pub limit: u32,
    pub cases: Vec<Scored>,
    /// Answerable cases whose answer came back first.
    pub top: f64,
    /// Answerable cases whose answer came back at all.
    pub found: f64,
    /// Mean reciprocal rank over answerable cases: 1.0 is always first, 0.5
    /// is always second, and a miss counts as zero.
    pub mrr: f64,
    /// Answerable cases whose answer text was in the returned response.
    pub shown: f64,
    /// Memories returned per unanswerable case. Every one is something an
    /// agent read for nothing, and might act on.
    pub noise: f64,
    /// Extra memories returned alongside the answer, per answered case.
    pub surplus: f64,
}

/// Reads a cases file.
pub fn cases(raw: &str) -> Result<Vec<Case>> {
    let parsed: Cases = toml::from_str(raw).context("could not read the cases file")?;
    anyhow::ensure!(
        !parsed.cases.is_empty(),
        "the cases file has no [[case]] entries"
    );
    for case in &parsed.cases {
        anyhow::ensure!(!case.query.trim().is_empty(), "a case has an empty query");
    }
    Ok(parsed.cases)
}

impl Brain {
    /// Runs every case and scores it.
    ///
    /// Ranks are judged on whole memories even when recall abridged one to
    /// fit its budget. Whether the right memory came back and how high is the
    /// question; how much of it fit is the budget's, and has its own setting.
    pub async fn evaluate(
        &self,
        cases: &[Case],
        limit: u32,
        budget: Option<Optimization>,
    ) -> Result<Evaluation> {
        let mut scored = Vec::with_capacity(cases.len());
        for case in cases {
            let started = std::time::Instant::now();
            let (_, recalled) = self
                .recallscoped(&case.query, limit, budget, case.project.as_deref())
                .await?;
            let micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let characters = recalled
                .iter()
                .map(|memory| memory.body.chars().count())
                .sum();
            let expect: Vec<String> = case.expect.iter().map(|text| text.to_lowercase()).collect();
            let mut rank = None;
            let mut shown = false;
            for (position, hit) in recalled.iter().enumerate() {
                let body = match self.memory(hit.id).await? {
                    Some(memory) => memory.body,
                    None => hit.body.clone(),
                }
                .to_lowercase();
                if expect.iter().any(|text| body.contains(text.as_str())) {
                    rank = Some(position + 1);
                    let returned = hit.body.to_lowercase();
                    shown = expect.iter().any(|text| returned.contains(text.as_str()));
                    break;
                }
            }
            scored.push(Scored {
                query: case.query.clone(),
                answerable: !expect.is_empty(),
                rank,
                shown,
                returned: recalled.len(),
                characters,
                micros,
            });
        }
        Ok(summarize(scored, limit))
    }
}

fn summarize(cases: Vec<Scored>, limit: u32) -> Evaluation {
    let answerable: Vec<&Scored> = cases.iter().filter(|case| case.answerable).collect();
    let unanswerable: Vec<&Scored> = cases.iter().filter(|case| !case.answerable).collect();
    let share = |count: usize, of: usize| match of {
        0 => 0.0,
        of => count as f64 / of as f64,
    };
    let top = share(
        answerable
            .iter()
            .filter(|case| case.rank == Some(1))
            .count(),
        answerable.len(),
    );
    let found = share(
        answerable.iter().filter(|case| case.rank.is_some()).count(),
        answerable.len(),
    );
    let mrr = match answerable.len() {
        0 => 0.0,
        count => {
            answerable
                .iter()
                .filter_map(|case| case.rank)
                .map(|rank| 1.0 / rank as f64)
                .sum::<f64>()
                / count as f64
        }
    };
    let shown = share(
        answerable.iter().filter(|case| case.shown).count(),
        answerable.len(),
    );
    let noise = match unanswerable.len() {
        0 => 0.0,
        count => unanswerable.iter().map(|case| case.returned).sum::<usize>() as f64 / count as f64,
    };
    let answered: Vec<&&Scored> = answerable
        .iter()
        .filter(|case| case.rank.is_some())
        .collect();
    let surplus = match answered.len() {
        0 => 0.0,
        count => {
            answered
                .iter()
                .map(|case| case.returned.saturating_sub(1))
                .sum::<usize>() as f64
                / count as f64
        }
    };
    Evaluation {
        limit,
        cases,
        top,
        found,
        mrr,
        shown,
        noise,
        surplus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MemoryScope::{Global, Project};

    #[test]
    fn a_cases_file_reads_and_an_empty_one_is_refused() {
        let parsed = cases(
            r#"
            [[case]]
            query = "where do credentials live"
            expect = ["vault"]

            [[case]]
            query = "what is the office wifi password"
            "#,
        )
        .unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(parsed[1].expect.is_empty());

        assert!(cases("").is_err());
        assert!(cases("[[case]]\nquery = \"x\"\nexpected = [\"typo\"]\n").is_err());
    }

    #[test]
    fn scores_are_computed_over_the_right_cases() {
        let scored = |rank: Option<usize>, answerable: bool, returned: usize| Scored {
            query: String::new(),
            answerable,
            rank,
            shown: rank == Some(1),
            returned,
            characters: 0,
            micros: 0,
        };
        let evaluation = summarize(
            vec![
                scored(Some(1), true, 3),
                scored(Some(2), true, 3),
                scored(None, true, 3),
                scored(None, true, 0),
                scored(None, false, 2),
                scored(None, false, 0),
            ],
            5,
        );
        assert_eq!(evaluation.top, 0.25);
        assert_eq!(evaluation.found, 0.5);
        assert_eq!(evaluation.mrr, 1.5 / 4.0);
        assert_eq!(evaluation.shown, 0.25);
        assert_eq!(evaluation.noise, 1.0);
        // Two answered cases with two extras each; misses don't count.
        assert_eq!(evaluation.surplus, 2.0);
    }

    /// End to end against a real store, and a guard on search itself: the
    /// stopword change exists because a function word used to outrank the
    /// memory that answered the question.
    #[tokio::test]
    async fn recall_is_scored_as_an_agent_would_get_it() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("app");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        for (body, scope) in [
            (
                "Credentials live in the vault, never in the repository.",
                Global,
            ),
            (
                "Where we are with the migration: halfway, see the plan.",
                Global,
            ),
            (
                "Use bun for every JavaScript task in this repository.",
                Project,
            ),
            (
                "Deploys run from the release branch after the tag is signed.",
                Project,
            ),
        ] {
            let at = (scope == Project).then_some(project.as_path());
            brain.rememberscoped(body, None, scope, at).await.unwrap();
        }
        let cases = vec![
            Case {
                query: "where are the credentials".into(),
                project: None,
                expect: vec!["vault".into()],
            },
            Case {
                query: "which javascript runtime".into(),
                project: Some(project.clone()),
                expect: vec!["bun".into()],
            },
            Case {
                query: "deploy branch".into(),
                project: None,
                expect: vec!["release branch".into()],
            },
            Case {
                query: "kubernetes cluster autoscaling".into(),
                project: Some(project.clone()),
                expect: vec![],
            },
        ];

        let evaluation = brain.evaluate(&cases, 5, None).await.unwrap();

        let ranks: Vec<Option<usize>> = evaluation.cases.iter().map(|case| case.rank).collect();
        assert_eq!(ranks, vec![Some(1), Some(1), None, None]);
        assert_eq!(
            evaluation.cases[2].returned, 0,
            "project memory stays in its project"
        );
        assert_eq!(evaluation.noise, 0.0);
        assert!((evaluation.found - 2.0 / 3.0).abs() < 1e-9);
    }
}
