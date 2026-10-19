//! Recall's relevance floor and drift flags, measured on a small hand-labeled
//! corpus where the same words (deploy, database, credentials) show up across
//! topics. `search` has no floor, so it serves as the baseline.

use synapsecore::brain::{Brain, Case, MemoryScope, Optimization};

const CORPUS: &[&str] = &[
    "Deploys run from the release branch after the tag is signed by two maintainers.",
    "Staging deploys happen automatically on every merge to main.",
    "Never deploy on Fridays unless the change is a security fix.",
    "Rollback a bad deploy with the previous release tag, not a revert commit.",
    "The production database is Postgres 16 with a read replica in a second region.",
    "Database migrations are hand-written reversible SQL files, applied in order.",
    "Run database migrations against a copy of production before merging them.",
    "The test database is recreated from migrations at the start of every test run.",
    "Credentials live in the vault, never in the repository or a commit.",
    "Rotate database credentials every ninety days through the vault rotate command.",
    "CI reads deploy credentials from the vault at job start, scoped to that job.",
    "Unit tests must not touch the network; mock the HTTP client instead.",
    "Integration tests run against the test database and a local queue.",
    "Flaky tests are quarantined with a tracking issue, never deleted.",
    "Frontend components live in src/components, one folder per component.",
    "Use bun for every JavaScript task: install, test, and scripts.",
    "The design system's color tokens are defined once and never hard-coded.",
    "Logs ship to the central collector; never log credentials or tokens.",
    "On-call rotates weekly; the handoff note lists open incidents.",
    "Incidents get a written review within five working days.",
    "The queue retries a failed job three times with exponential backoff.",
    "Feature flags are removed within two releases of reaching everyone.",
    "API errors return a stable code and a human message, never a stack trace.",
    "Rate limits are per token, per minute, and documented in the API reference.",
];

/// (query, text only the answer contains). Empty means unanswerable.
const CASES: &[(&str, &str)] = &[
    ("rotate database credentials", "ninety days"),
    ("deploy release branch tag", "signed by two maintainers"),
    (
        "test database migrations recreated",
        "start of every test run",
    ),
    ("ci deploy credentials vault", "scoped to that job"),
    ("rollback bad deploy release", "previous release tag"),
    ("flaky tests quarantined issue", "tracking issue"),
    ("unit tests network mock", "mock the http client"),
    ("production database replica region", "read replica"),
    ("queue retries failed job backoff", "exponential backoff"),
    ("log credentials tokens collector", "central collector"),
    ("kubernetes cluster autoscaling nodes", ""),
    ("graphql schema federation gateway", ""),
];

async fn store() -> (tempfile::TempDir, Brain) {
    let root = tempfile::tempdir().unwrap();
    let brain = Brain::open(root.path().join("brain.db")).await.unwrap();
    for body in CORPUS {
        brain
            .rememberscoped(body, None, MemoryScope::Global, None)
            .await
            .unwrap();
    }
    (root, brain)
}

fn cases() -> Vec<Case> {
    CASES
        .iter()
        .map(|(query, expect)| Case {
            query: (*query).to_owned(),
            project: None,
            expect: match expect.is_empty() {
                true => Vec::new(),
                false => vec![(*expect).to_owned()],
            },
        })
        .collect()
}

/// (rank of the answer, results returned) without the floor.
async fn unfloored(brain: &Brain, case: &Case, limit: u32) -> (Option<usize>, usize) {
    let found = brain.search(&case.query, limit).await.unwrap();
    let expect: Vec<String> = case.expect.iter().map(|text| text.to_lowercase()).collect();
    let rank = found
        .iter()
        .position(|memory| {
            let body = memory.body.to_lowercase();
            expect.iter().any(|text| body.contains(text.as_str()))
        })
        .map(|position| position + 1);
    (rank, found.len())
}

#[tokio::test]
async fn the_floor_costs_no_answer_and_leaves_less_beside_each_one() {
    let (_root, brain) = store().await;
    brain.setoptimization(Optimization::Full).await.unwrap();
    let limit = 8;
    let cases = cases();
    let evaluation = brain.evaluate(&cases, limit, None).await.unwrap();

    let mut before = Vec::new();
    for case in &cases {
        before.push(unfloored(&brain, case, limit).await);
    }

    for ((case, scored), (rank, _)) in cases.iter().zip(&evaluation.cases).zip(&before) {
        if !scored.answerable {
            continue;
        }
        let floored = scored.rank.expect(&case.query);
        let unfloored = rank.expect(&case.query);
        assert!(
            floored <= unfloored,
            "`{}` moved from {unfloored} to {floored}",
            case.query
        );
    }
    assert_eq!(evaluation.found, 1.0, "{:#?}", evaluation.cases);
    assert_eq!(evaluation.top, 1.0, "{:#?}", evaluation.cases);

    let answered: Vec<usize> = before
        .iter()
        .filter_map(|(rank, returned)| rank.map(|_| returned.saturating_sub(1)))
        .collect();
    let surplus = answered.iter().sum::<usize>() as f64 / answered.len() as f64;
    assert!(
        evaluation.surplus < surplus,
        "floored surplus {} should be below unfloored {surplus}",
        evaluation.surplus
    );
    assert!(
        evaluation.surplus <= surplus / 2.0,
        "floored surplus {} against unfloored {surplus}",
        evaluation.surplus
    );
}

#[tokio::test]
async fn every_answer_survives_every_limit() {
    let (_root, brain) = store().await;
    brain.setoptimization(Optimization::Full).await.unwrap();
    let cases = cases();
    for limit in 1..=10 {
        let evaluation = brain.evaluate(&cases, limit, None).await.unwrap();
        assert_eq!(
            evaluation.top, 1.0,
            "limit {limit}: {:#?}",
            evaluation.cases
        );
    }
}

#[tokio::test]
async fn unanswerable_questions_get_no_more_than_before() {
    let (_root, brain) = store().await;
    brain.setoptimization(Optimization::Full).await.unwrap();
    for case in cases().iter().filter(|case| case.expect.is_empty()) {
        let (_, recalled) = brain
            .recallscoped(&case.query, 8, None, None)
            .await
            .unwrap();
        let (_, before) = unfloored(&brain, case, 8).await;
        assert!(recalled.len() <= before, "`{}`", case.query);
    }
}

/// Corrections stored without `supersedes`. Recall may return the outdated
/// memory, but never without a drift finding naming it.
#[tokio::test]
async fn an_outdated_memory_never_comes_back_unmarked() {
    let (_root, brain) = store().await;
    brain.setoptimization(Optimization::Full).await.unwrap();
    let corrections = [
        (
            "The queue retries a failed job three times with exponential backoff.",
            "The queue retries a failed job five times with exponential backoff.",
            "queue retries failed job",
        ),
        (
            "Always squash commits when merging a pull request into main.",
            "Never squash commits when merging a pull request into main.",
            "squash commits merging pull request",
        ),
        (
            "Integration tests run against the shared staging database every night.",
            "Integration tests do not run against the shared staging database every night.",
            "integration tests staging database",
        ),
        (
            "Consumers commit offsets after processing each message batch.",
            "Consumers commit offsets before processing each message batch.",
            "consumers commit offsets processing",
        ),
        (
            "The production database is Postgres 16 with a read replica in a second region.",
            "The production database is Postgres 17 with a read replica in a second region.",
            "production database postgres replica",
        ),
    ];
    let mut outdated = Vec::new();
    for (old, new, _) in &corrections {
        // The Postgres 16 memory is already in the corpus.
        let existing = brain.grep(old, 1).await.unwrap();
        let id = match existing.first() {
            Some(memory) => memory.id,
            None => brain
                .rememberscoped(old, None, MemoryScope::Global, None)
                .await
                .unwrap(),
        };
        outdated.push(id);
        brain
            .rememberscoped(new, None, MemoryScope::Global, None)
            .await
            .unwrap();
    }

    let mut unmarked = 0;
    let mut returned = 0;
    for ((_, _, question), old) in corrections.iter().zip(&outdated) {
        let (_, recalled) = brain.recallscoped(question, 8, None, None).await.unwrap();
        if !recalled.iter().any(|memory| memory.id == *old) {
            continue;
        }
        returned += 1;
        let findings = brain.drifted(&recalled, None).await.unwrap();
        if !findings
            .iter()
            .any(|finding| finding.ids.first() == Some(old))
        {
            unmarked += 1;
            eprintln!("`{question}` returned #{old} unmarked: {findings:#?}");
        }
    }
    // Guards against the loop passing vacuously.
    assert_eq!(returned, corrections.len());
    assert_eq!(unmarked, 0);
}
