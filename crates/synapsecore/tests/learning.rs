use synapsecore::brain::{Brain, MemoryScope};
use synapsecore::learning::{Disposition, Feedback, Outcome, Target, Verification};

#[path = "learning/check.rs"]
mod check;
#[path = "learning/cli.rs"]
mod cli;
#[path = "learning/concurrency.rs"]
mod concurrency;
#[path = "learning/promotion.rs"]
mod promotion;

struct Fixture {
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
    other: std::path::PathBuf,
    brain: Brain,
    memory: i64,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("app");
        let other = directory.path().join("other");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(other.join(".git")).unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain.setlearn(true).await.unwrap();
        let memory = brain
            .rememberscoped(
                "Always squash commits when merging a pull request into main.",
                None,
                MemoryScope::Project,
                Some(&root),
            )
            .await
            .unwrap();
        Self {
            _directory: directory,
            root,
            other,
            brain,
            memory,
        }
    }

    fn feedback(&self, outcome: Outcome) -> Feedback {
        Feedback {
            target: Target::Memory { id: self.memory },
            outcome,
            problem: "The release process lost the reviewed commit history.".into(),
            correction: "Merge the reviewed branch while preserving its commits.".into(),
            evidence: "The release check retained every reviewed commit.".into(),
            query: "release merge history".into(),
            expected: "preserving its commits".into(),
        }
    }

    async fn conflict(&self) -> synapsecore::brain::drift::Finding {
        self.brain
            .rememberscoped(
                "Never squash commits when merging a pull request into main.",
                None,
                MemoryScope::Project,
                Some(&self.root),
            )
            .await
            .unwrap();
        self.brain
            .drift(Some(&self.root))
            .await
            .unwrap()
            .findings
            .remove(0)
    }
}

#[tokio::test]
async fn retries_count_once_and_claims_need_separate_confirmation() {
    let f = Fixture::new().await;
    let first = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "session-one")
        .await
        .unwrap();
    assert_eq!(first.verification, Verification::Reported);
    assert!(!first.qualified());
    let retry = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "session-one")
        .await
        .unwrap();
    assert_eq!(first.id, retry.id);
    assert_ne!(first.session, "session-one");
    assert_eq!(
        f.brain.feedbackrecords(&f.root, 100).await.unwrap().len(),
        1
    );
    let confirmed = f
        .brain
        .confirmfeedback(first.id, &f.root, true)
        .await
        .unwrap();
    assert!(confirmed.qualified());
    let different = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "session-two")
        .await
        .unwrap();
    assert_ne!(different.id, first.id);
    assert_eq!(different.verification, Verification::Reported);
    assert_eq!(different.lesson, first.lesson);
}

#[tokio::test]
async fn changing_the_claim_or_target_invalidates_verification() {
    let f = Fixture::new().await;
    let first = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    f.brain
        .confirmfeedback(first.id, &f.root, true)
        .await
        .unwrap();
    let changed = f
        .brain
        .feedback(f.feedback(Outcome::Unresolved), &f.root, "one")
        .await
        .unwrap();
    assert_eq!(changed.verification, Verification::Reported);
    let confirmed = f
        .brain
        .confirmfeedback(changed.id, &f.root, true)
        .await
        .unwrap();
    assert_eq!(confirmed.outcome, Outcome::Unresolved);
    assert!(!confirmed.qualified());
    f.brain
        .updatememory(
            f.memory,
            "Keep each reviewed commit during a release.",
            None,
        )
        .await
        .unwrap();
    assert!(
        f.brain
            .confirmfeedback(first.id, &f.root, true)
            .await
            .is_err()
    );
    let fresh = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    assert_ne!(fresh.id, first.id);
    assert_ne!(fresh.digest, first.digest);
}

#[tokio::test]
async fn feedback_cannot_cross_project_boundaries_or_write_while_disabled() {
    let f = Fixture::new().await;
    assert!(
        f.brain
            .feedback(f.feedback(Outcome::Helpful), &f.other, "one")
            .await
            .is_err()
    );
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    assert!(f.brain.feedbackrecord(record.id, &f.other).await.is_err());
    assert!(
        f.brain
            .confirmfeedback(record.id, &f.other, true)
            .await
            .is_err()
    );
    f.brain.setlearn(false).await.unwrap();
    assert!(
        f.brain
            .feedback(f.feedback(Outcome::Helpful), &f.root, "two")
            .await
            .is_err()
    );
    assert!(
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn evidence_is_bounded_redacted_and_cannot_claim_its_own_verification() {
    let f = Fixture::new().await;
    let mut input = f.feedback(Outcome::Helpful);
    input.evidence = "PASSWORD=hunter2 <private>do not keep this</private>".into();
    let record = f.brain.feedback(input, &f.root, "one").await.unwrap();
    assert!(!record.evidence.contains("hunter2"));
    assert!(!record.evidence.contains("do not keep"));
    assert_eq!(record.verification, Verification::Reported);
    let mut input = f.feedback(Outcome::Helpful);
    input.correction = "x".repeat(4_001);
    assert!(f.brain.feedback(input, &f.root, "one").await.is_err());
    let mut json = serde_json::to_value(f.feedback(Outcome::Helpful)).unwrap();
    json["verification"] = serde_json::json!("confirmed");
    assert!(serde_json::from_value::<Feedback>(json).is_err());
}

#[tokio::test]
async fn drift_resolutions_are_scoped_reversible_and_expire_when_content_changes() {
    let f = Fixture::new().await;
    let finding = f.conflict().await;
    let resolution = f
        .brain
        .resolvefinding(
            &f.root,
            &finding,
            Disposition::Contextual,
            "Different release branches use different merge policies.",
        )
        .await
        .unwrap();
    assert!(
        f.brain
            .drift(Some(&f.root))
            .await
            .unwrap()
            .findings
            .is_empty()
    );
    let (_, recalled) = f
        .brain
        .recallscoped("merge commits", 5, None, Some(&f.root))
        .await
        .unwrap();
    assert!(
        f.brain
            .drifted(&recalled, Some(&f.root))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        f.brain
            .related(f.memory, Some(&f.root))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !f.brain
            .restoreresolution(resolution.id, &f.other)
            .await
            .unwrap()
    );
    assert!(
        f.brain
            .restoreresolution(resolution.id, &f.root)
            .await
            .unwrap()
    );
    assert_eq!(
        f.brain.drift(Some(&f.root)).await.unwrap().findings.len(),
        1
    );
    f.brain
        .resolvefinding(
            &f.root,
            &finding,
            Disposition::Falsepositive,
            "Reviewed both rules.",
        )
        .await
        .unwrap();
    f.brain
        .updatememory(
            f.memory,
            "Always squash commits when merging a pull request into the main release branch.",
            None,
        )
        .await
        .unwrap();
    assert!(
        !f.brain
            .drift(Some(&f.root))
            .await
            .unwrap()
            .findings
            .is_empty()
    );
}

#[tokio::test]
async fn abridged_recall_uses_the_full_content_fingerprint_for_resolutions() {
    let f = Fixture::new().await;
    let filler =
        "This release also preserves the reviewed history for audit and maintenance. ".repeat(150);
    f.brain
        .updatememory(
            f.memory,
            &format!("Always squash commits when merging a pull request into main. {filler}"),
            None,
        )
        .await
        .unwrap();
    f.brain
        .rememberscoped(
            &format!("Never squash commits when merging a pull request into main. {filler}"),
            None,
            MemoryScope::Project,
            Some(&f.root),
        )
        .await
        .unwrap();
    let finding = f
        .brain
        .drift(Some(&f.root))
        .await
        .unwrap()
        .findings
        .remove(0);
    f.brain
        .resolvefinding(
            &f.root,
            &finding,
            Disposition::Contextual,
            "Reviewed branch-specific policies.",
        )
        .await
        .unwrap();
    let (_, memories) = f
        .brain
        .recallscoped(
            "squash merge",
            4,
            Some(synapsecore::brain::Optimization::Lean),
            Some(&f.root),
        )
        .await
        .unwrap();
    assert!(memories.iter().any(|memory| memory.abridged));
    assert!(
        f.brain
            .drifted(&memories, Some(&f.root))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn whitespace_compaction_keeps_a_saved_assessment_valid() {
    let f = Fixture::new().await;
    f.brain
        .updatememory(
            f.memory,
            "Always squash commits\n\nwhen merging a pull request into main.",
            None,
        )
        .await
        .unwrap();
    let finding = f.conflict().await;
    f.brain
        .resolvefinding(
            &f.root,
            &finding,
            Disposition::Contextual,
            "Reviewed policies for distinct release branches.",
        )
        .await
        .unwrap();
    let (_, memories) = f
        .brain
        .recallscoped(
            "squash merge",
            4,
            Some(synapsecore::brain::Optimization::Lean),
            Some(&f.root),
        )
        .await
        .unwrap();
    assert!(memories.iter().all(|memory| !memory.abridged));
    let rendered = memories
        .iter()
        .find(|memory| memory.id == f.memory)
        .unwrap();
    assert!(!rendered.body.contains("\n\n"));
    assert!(
        f.brain
            .drifted(&memories, Some(&f.root))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_resolution_cannot_hide_another_projects_memory_or_a_fabricated_finding() {
    let f = Fixture::new().await;
    let mut finding = f.conflict().await;
    assert!(
        f.brain
            .resolvefinding(&f.other, &finding, Disposition::Falsepositive, "hide")
            .await
            .is_err()
    );
    assert!(
        f.brain
            .resolvefinding(&f.root, &finding, Disposition::Corrected, "retry passed")
            .await
            .is_err()
    );
    finding.missing.push("fabricated/path.rs".into());
    assert!(
        f.brain
            .resolvefinding(&f.root, &finding, Disposition::Falsepositive, "hide")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn retention_has_an_age_limit_and_a_hard_row_limit() {
    let f = Fixture::new().await;
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    sqlx::query("UPDATE learningfeedback SET updated = 0 WHERE id = ?")
        .bind(record.id)
        .execute(f.brain.pool())
        .await
        .unwrap();
    let mut transaction = f.brain.pool().begin().await.unwrap();
    for n in 0..5_050 {
        sqlx::query("INSERT INTO learningfeedback(project,target,digest,lesson,session,outcome,problem,correction,evidence,created,updated) VALUES ('p','t','d','l',?,'helpful','','','',1,CAST(strftime('%s','now') AS INTEGER))")
            .bind(n.to_string()).execute(&mut *transaction).await.unwrap();
    }
    transaction.commit().await.unwrap();
    f.brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "two")
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learningfeedback")
        .fetch_one(f.brain.pool())
        .await
        .unwrap();
    assert!(count <= synapsecore::learning::FEEDBACKLIMIT);
    assert!(f.brain.feedbackrecord(record.id, &f.root).await.is_err());
}

#[tokio::test]
async fn concurrent_reports_remain_idempotent_and_bounded() {
    let f = Fixture::new().await;
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let brain = f.brain.clone();
        let folder = f.root.clone();
        let input = f.feedback(Outcome::Helpful);
        tasks.push(tokio::spawn(async move {
            brain.feedback(input, &folder, "same-session").await
        }));
    }
    let mut ids = Vec::new();
    for task in tasks {
        ids.push(task.await.unwrap().unwrap().id);
    }
    assert!(ids.iter().all(|id| *id == ids[0]));
    assert_eq!(
        f.brain.feedbackrecords(&f.root, 100).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn migration_from_the_released_schema_preserves_memory() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("brain.db");
    let brain = Brain::open(&path).await.unwrap();
    let id = brain
        .rememberscoped(
            "Release history survives schema upgrades.",
            None,
            MemoryScope::Global,
            None,
        )
        .await
        .unwrap();
    for table in [
        "presence",
        "sceneavatar",
        "learningpromotion",
        "learningcase",
        "driftresolution",
        "learningfeedback",
    ] {
        sqlx::query(&format!("DROP TABLE {table}"))
            .execute(brain.pool())
            .await
            .unwrap();
    }
    sqlx::query("PRAGMA user_version = 14")
        .execute(brain.pool())
        .await
        .unwrap();
    brain.pool().close().await;
    drop(brain);
    let upgraded = Brain::open(&path).await.unwrap();
    assert!(
        upgraded
            .memory(id)
            .await
            .unwrap()
            .unwrap()
            .body
            .contains("survives")
    );
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(upgraded.pool())
        .await
        .unwrap();
    assert_eq!(version, 17);
    let backups = synapsecore::database::backupfolder(&path).unwrap();
    assert!(!synapsecore::database::snapshots(&backups).is_empty());
}
