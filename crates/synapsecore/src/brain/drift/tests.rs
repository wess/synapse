use super::*;
use crate::brain::MemoryScope::{Global, Project};

fn repository(root: &Path, url: &str) {
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(
        root.join(".git/config"),
        format!("[remote \"origin\"]\n\turl = {url}\n"),
    )
    .unwrap();
}

fn file(root: &Path, relative: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "").unwrap();
}

fn memory(id: i64, body: &str, scope: MemoryScope, project: &str, created: i64) -> Memory {
    Memory {
        id,
        body: body.to_owned(),
        source: String::new(),
        scope,
        project: project.to_owned(),
        created,
        superseded: 0,
        abridged: false,
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    brain: Brain,
}

async fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("app");
    repository(&root, "git@github.com:acme/app.git");
    file(&root, "crates/core/src/vault/entry.rs");
    file(&root, "docs/retrieval.md");
    let brain = Brain::open(directory.path().join("brain.db"))
        .await
        .unwrap();
    Fixture {
        _directory: directory,
        root,
        brain,
    }
}

impl Fixture {
    async fn remember(&self, body: &str, scope: MemoryScope) -> i64 {
        let at = (scope == Project).then_some(self.root.as_path());
        self.brain
            .rememberscoped(body, None, scope, at)
            .await
            .unwrap()
    }

    async fn drift(&self) -> Drift {
        self.brain.drift(Some(&self.root)).await.unwrap()
    }
}

#[tokio::test]
async fn a_moved_file_is_found_and_a_present_one_is_not() {
    let fixture = fixture().await;
    let kept = fixture
        .remember(
            "Secret entry guards live in crates/core/src/vault/entry.rs.",
            Project,
        )
        .await;
    let moved = fixture
        .remember(
            "Ranking is in crates/core/src/brain/rank.rs and documented in docs/retrieval.md.",
            Project,
        )
        .await;

    let drift = fixture.drift().await;
    assert_eq!(drift.project.as_deref(), Some("github.com/acme/app"));
    assert_eq!(drift.scanned, 2);
    assert_eq!(drift.findings.len(), 1, "{:#?}", drift.findings);
    let finding = &drift.findings[0];
    assert_eq!(finding.concern, Concern::Stale);
    assert_eq!(finding.ids, vec![moved]);
    assert_eq!(finding.missing, vec!["crates/core/src/brain/rank.rs"]);
    assert!(
        finding.note.contains("is no longer there"),
        "{}",
        finding.note
    );
    assert!(
        !drift
            .findings
            .iter()
            .any(|finding| finding.ids.contains(&kept))
    );

    // The check reads the checkout as it is now; nothing is stored.
    file(&fixture.root, "crates/core/src/brain/rank.rs");
    assert!(fixture.drift().await.findings.is_empty());
}

/// A global memory's relative path could belong to any repository.
#[tokio::test]
async fn a_relative_path_in_global_memory_is_never_called_missing() {
    let fixture = fixture().await;
    fixture
        .remember("Every repository keeps its plan in docs/plan.md.", Global)
        .await;
    assert!(fixture.drift().await.findings.is_empty());
}

#[tokio::test]
async fn memory_from_another_checkout_is_not_judged_against_this_one() {
    let fixture = fixture().await;
    let other = fixture._directory.path().join("other");
    repository(&other, "git@github.com:acme/other.git");
    fixture
        .brain
        .rememberscoped(
            "The other app keeps routes in src/routes/index.ts.",
            None,
            Project,
            Some(&other),
        )
        .await
        .unwrap();
    // Not visible from this checkout; flagged from its own.
    assert_eq!(fixture.drift().await.scanned, 0);
    let there = fixture.brain.drift(Some(&other)).await.unwrap();
    assert_eq!(there.scanned, 1);
    assert_eq!(there.findings[0].missing, vec!["src/routes/index.ts"]);
}

#[tokio::test]
async fn a_conflict_names_the_older_memory_first_and_superseding_settles_it() {
    let fixture = fixture().await;
    let older = fixture
        .remember(
            "Always squash commits when merging a pull request into main.",
            Project,
        )
        .await;
    let newer = fixture
        .remember(
            "Never squash commits when merging a pull request into main.",
            Project,
        )
        .await;

    let drift = fixture.drift().await;
    assert_eq!(drift.findings.len(), 1, "{:#?}", drift.findings);
    let finding = &drift.findings[0];
    assert_eq!(finding.concern, Concern::Conflict);
    assert_eq!(finding.ids, vec![older, newer]);
    assert_eq!(
        finding.note,
        format!(
            "#{older} says `always` where #{newer} says `never` about the same thing. \
             If they disagree, #{newer} is the newer."
        )
    );

    fixture.brain.supersede(older, newer).await.unwrap();
    assert!(fixture.drift().await.findings.is_empty());
}

#[tokio::test]
async fn a_duplicate_and_a_changed_value_are_both_found() {
    let fixture = fixture().await;
    let first = fixture
        .remember(
            "Use bun for every JavaScript task: install, test, and scripts.",
            Global,
        )
        .await;
    let second = fixture
        .remember(
            "Use bun for every JavaScript task, including install, tests, and scripts.",
            Global,
        )
        .await;
    let old = fixture
        .remember(
            "The production database is Postgres 16 with a read replica.",
            Project,
        )
        .await;
    let new = fixture
        .remember(
            "The production database is Postgres 17 with a read replica.",
            Project,
        )
        .await;

    let drift = fixture.drift().await;
    let found: Vec<(Concern, Vec<i64>)> = drift
        .findings
        .iter()
        .map(|finding| (finding.concern, finding.ids.clone()))
        .collect();
    assert_eq!(
        found,
        vec![
            (Concern::Conflict, vec![old, new]),
            (Concern::Duplicate, vec![first, second]),
        ]
    );
    assert!(
        drift.findings[0].note.contains("says 16 where"),
        "{}",
        drift.findings[0].note
    );
}

#[test]
fn findings_do_not_depend_on_the_order_memories_arrive_in() {
    let older = memory(
        4,
        "Consumers commit offsets after processing each message batch.",
        Global,
        "",
        10,
    );
    let newer = memory(
        9,
        "Consumers commit offsets before processing each message batch.",
        Global,
        "",
        20,
    );
    let checkout = Checkout::new(None, None, None);
    let forward = scan(&[older.clone(), newer.clone()], &checkout);
    let backward = scan(&[newer, older], &checkout);
    assert_eq!(forward, backward);
    assert_eq!(forward[0].ids, vec![4, 9]);
    assert!(
        forward[0]
            .note
            .starts_with("#4 says `after` where #9 says `before`"),
        "{}",
        forward[0].note
    );
}

/// Prefiltered results must match brute force exactly.
#[test]
fn the_prefilter_misses_nothing_that_comparing_every_pair_finds() {
    // Big enough that words shared by every memory are common, which small
    // fixtures never hit.
    let details = [
        "owners",
        "alerts",
        "budgets",
        "reviews",
        "rollouts",
        "quotas",
        "incidents",
        "dashboards",
        "runbooks",
        "contracts",
        "audits",
        "migrations",
        "backups",
        "tickets",
        "pagers",
        "schedules",
        "licenses",
        "vendors",
        "metrics",
        "traces",
        "tokens",
        "regions",
        "replicas",
        "shards",
        "queues",
        "caches",
        "fixtures",
        "mocks",
        "flags",
        "experiments",
        "cohorts",
        "funnels",
        "invoices",
        "refunds",
        "payouts",
        "ledgers",
        "exports",
        "imports",
        "webhooks",
        "sockets",
    ];
    let mut bodies: Vec<String> = Vec::new();
    for topic in [
        "deploy",
        "database",
        "queue",
        "frontend",
        "credentials",
        "logging",
    ] {
        for detail in details {
            bodies.push(format!(
                "The {topic} group tracks {detail} for the platform."
            ));
        }
    }
    assert!(bodies.len() > 200);
    bodies.push("Always pin the toolchain version in every workspace manifest file.".into());
    bodies.push("Never pin the toolchain version in every workspace manifest file.".into());
    bodies.push("Cache entries expire after 300 seconds in the edge proxy layer.".into());
    bodies.push("Cache entries expire after 900 seconds in the edge proxy layer.".into());
    let memories: Vec<Memory> = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| memory(index as i64 + 1, body, Global, "", index as i64))
        .collect();
    let profiles: Vec<Profile> = memories
        .iter()
        .map(|memory| compare::profile(&memory.body))
        .collect();
    let mut everything = Vec::new();
    for first in 0..memories.len() {
        for second in first + 1..memories.len() {
            if let Some(relation) = compare::relate(&profiles[first], &profiles[second]) {
                everything.push(paired(&memories[first], &memories[second], relation));
            }
        }
    }
    everything.sort_by(|a, b| a.concern.cmp(&b.concern).then(a.ids.cmp(&b.ids)));
    let checkout = Checkout::new(None, None, None);
    let found = scan(&memories, &checkout);
    assert_eq!(found, everything);
    let conflicts = found
        .iter()
        .filter(|finding| finding.concern == Concern::Conflict)
        .count();
    assert!(conflicts >= 2, "the planted pairs: {found:#?}");
}

#[test]
fn an_ordinary_store_reports_nothing_until_something_contradicts() {
    let corpus = [
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
        "Frontend components live in one folder per component.",
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
    let mut memories: Vec<Memory> = corpus
        .iter()
        .enumerate()
        .map(|(index, body)| memory(index as i64 + 1, body, Global, "", index as i64))
        .collect();
    let checkout = Checkout::new(None, None, None);
    assert_eq!(scan(&memories, &checkout), Vec::new());

    memories.push(memory(
        100,
        "The queue retries a failed job five times with exponential backoff.",
        Global,
        "",
        100,
    ));
    let found = scan(&memories, &checkout);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].concern, Concern::Conflict);
    assert_eq!(found[0].ids, vec![21, 100]);
}

#[tokio::test]
async fn a_store_past_the_ceiling_says_it_read_only_part_of_it() {
    let fixture = fixture().await;
    let mut transaction = fixture.brain.pool().begin().await.unwrap();
    for id in 1..=SCANCEILING + 3 {
        sqlx::query("INSERT INTO memory(rowid, body, source, created) VALUES (?, ?, '', ?)")
            .bind(id)
            .bind(format!("Fixture memory number {id}."))
            .bind(id)
            .execute(&mut *transaction)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO memorymeta(memoryid, scope, project, native, created) \
         SELECT rowid, 'global', '', 1, CAST(created AS INTEGER) FROM memory",
    )
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    let drift = fixture.drift().await;
    assert!(drift.capped);
    assert_eq!(drift.scanned, SCANCEILING as usize);
}

/// The newer memory doesn't match the query, so recall alone would only return
/// the outdated one.
#[tokio::test]
async fn a_correction_the_query_missed_is_quoted_beside_what_it_corrects() {
    let fixture = fixture().await;
    let older = fixture
        .remember(
            "Always squash commits when merging a pull request into main, per the 2024 policy.",
            Project,
        )
        .await;
    let newer = fixture
        .remember(
            "Never squash commits when merging a pull request into main.",
            Project,
        )
        .await;
    let (_, recalled) = fixture
        .brain
        .recallscoped("policy", 8, None, Some(&fixture.root))
        .await
        .unwrap();
    assert_eq!(
        recalled.iter().map(|memory| memory.id).collect::<Vec<_>>(),
        vec![older]
    );

    let findings = fixture
        .brain
        .drifted(&recalled, Some(&fixture.root))
        .await
        .unwrap();
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert_eq!(findings[0].concern, Concern::Conflict);
    assert_eq!(findings[0].ids, vec![older, newer]);
    assert!(
        findings[0].note.ends_with(&format!(
            "#{newer} is not in this response; it says: “Never squash commits when merging a pull request into main.”"
        )),
        "{}",
        findings[0].note
    );
}

#[tokio::test]
async fn a_current_memory_does_not_bring_back_what_it_replaced() {
    let fixture = fixture().await;
    fixture
        .remember(
            "Always squash commits when merging a pull request into main.",
            Project,
        )
        .await;
    let newer = fixture
        .remember(
            "Never squash commits when merging a pull request into main, per the 2026 policy.",
            Project,
        )
        .await;
    let (_, recalled) = fixture
        .brain
        .recallscoped("policy", 8, None, Some(&fixture.root))
        .await
        .unwrap();
    assert_eq!(
        recalled.iter().map(|memory| memory.id).collect::<Vec<_>>(),
        vec![newer]
    );
    let findings = fixture
        .brain
        .drifted(&recalled, Some(&fixture.root))
        .await
        .unwrap();
    assert!(findings.is_empty(), "{findings:#?}");
}

#[tokio::test]
async fn writing_a_memory_says_what_it_contradicts_or_repeats() {
    let fixture = fixture().await;
    let original = fixture
        .remember(
            "The production database is Postgres 16 with a read replica.",
            Project,
        )
        .await;
    let copy = fixture
        .remember(
            "Use bun for every JavaScript task: install, test, and scripts.",
            Global,
        )
        .await;

    let changed = fixture
        .remember(
            "The production database is Postgres 17 with a read replica.",
            Project,
        )
        .await;
    let found = fixture
        .brain
        .related(changed, Some(&fixture.root))
        .await
        .unwrap();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        (found[0].concern, found[0].ids.clone()),
        (Concern::Conflict, vec![original, changed])
    );

    let again = fixture
        .remember(
            "Use bun for every JavaScript task, including install, tests, and scripts.",
            Global,
        )
        .await;
    let found = fixture
        .brain
        .related(again, Some(&fixture.root))
        .await
        .unwrap();
    assert_eq!(
        found
            .iter()
            .map(|finding| (finding.concern, finding.ids.clone()))
            .collect::<Vec<_>>(),
        vec![(Concern::Duplicate, vec![copy, again])]
    );

    let unrelated = fixture
        .remember(
            "Feature flags are removed within two releases of reaching everyone.",
            Project,
        )
        .await;
    assert!(
        fixture
            .brain
            .related(unrelated, Some(&fixture.root))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_correction_that_supersedes_reports_nothing() {
    let fixture = fixture().await;
    let original = fixture
        .remember(
            "The production database is Postgres 16 with a read replica.",
            Project,
        )
        .await;
    let (corrected, _) = fixture
        .brain
        .rememberreplacing(
            "The production database is Postgres 17 with a read replica.",
            None,
            Project,
            Some(&fixture.root),
            &[original],
        )
        .await
        .unwrap();
    assert!(
        fixture
            .brain
            .related(corrected, Some(&fixture.root))
            .await
            .unwrap()
            .is_empty()
    );
}
