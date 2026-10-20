use super::{Fixture, Outcome, Verification};
use synapsecore::brain::Optimization;

#[tokio::test]
async fn repeated_verified_lessons_propose_once_without_installing_anything() {
    let f = Fixture::new().await;
    for session in ["one", "two", "three"] {
        let record = f
            .brain
            .feedback(f.feedback(Outcome::Helpful), &f.root, session)
            .await
            .unwrap();
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .unwrap();
    }
    let proposed = f.brain.learningproposals(&f.root).await.unwrap();
    assert_eq!(proposed.len(), 1);
    assert!(proposed[0].instructions.contains("preserving its commits"));
    assert_eq!(proposed[0].state, "proposed");
    let installs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skillinstall")
        .fetch_one(f.brain.pool())
        .await
        .unwrap();
    assert_eq!(installs, 0);
    let fourth = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "four")
        .await
        .unwrap();
    f.brain
        .confirmfeedback(fourth.id, &f.root, true)
        .await
        .unwrap();
    assert_eq!(f.brain.learningproposals(&f.root).await.unwrap().len(), 1);
    assert!(
        f.brain
            .learningproposals(&f.other)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        f.brain
            .rejectlearning(proposed[0].id, &f.other)
            .await
            .is_err()
    );
    f.brain
        .rejectlearning(proposed[0].id, &f.root)
        .await
        .unwrap();
    f.brain
        .confirmfeedback(fourth.id, &f.root, true)
        .await
        .unwrap();
    assert!(f.brain.learningproposals(&f.root).await.unwrap().is_empty());
}

#[tokio::test]
async fn retries_self_reports_and_unresolved_success_do_not_promote() {
    let f = Fixture::new().await;
    for _ in 0..5 {
        let record = f
            .brain
            .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
            .await
            .unwrap();
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .unwrap();
    }
    for session in ["two", "three", "four"] {
        let record = f
            .brain
            .feedback(f.feedback(Outcome::Helpful), &f.root, session)
            .await
            .unwrap();
        assert_eq!(record.verification, Verification::Reported);
    }
    assert!(f.brain.learningproposals(&f.root).await.unwrap().is_empty());
    for session in ["five", "six", "seven"] {
        let record = f
            .brain
            .feedback(f.feedback(Outcome::Unresolved), &f.root, session)
            .await
            .unwrap();
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .unwrap();
    }
    assert!(f.brain.learningproposals(&f.root).await.unwrap().is_empty());
}

#[tokio::test]
async fn evidence_from_different_content_versions_does_not_combine() {
    let f = Fixture::new().await;
    for session in ["one", "two", "three"] {
        f.brain
            .updatememory(
                f.memory,
                &format!("Release policy version {session} preserves commits."),
                None,
            )
            .await
            .unwrap();
        let record = f
            .brain
            .feedback(f.feedback(Outcome::Helpful), &f.root, session)
            .await
            .unwrap();
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .unwrap();
    }
    assert!(f.brain.learningproposals(&f.root).await.unwrap().is_empty());
}

#[tokio::test]
async fn regression_cases_need_review_and_replay_real_scoped_recall() {
    let f = Fixture::new().await;
    f.brain
        .updatememory(
            f.memory,
            "Release merge history is retained by preserving its commits.",
            None,
        )
        .await
        .unwrap();
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    assert!(f.brain.learningcases(&f.root).await.unwrap().is_empty());
    f.brain
        .confirmfeedback(record.id, &f.root, true)
        .await
        .unwrap();
    let cases = f.brain.learningcases(&f.root).await.unwrap();
    assert_eq!(cases.len(), 1);
    assert!(!cases[0].reviewed);
    assert!(
        f.brain
            .evaluatelearning(&f.root, 4, Some(Optimization::Lean))
            .await
            .is_err()
    );
    assert!(
        f.brain
            .reviewlearningcase(cases[0].id, &f.other, true)
            .await
            .is_err()
    );
    f.brain
        .reviewlearningcase(cases[0].id, &f.root, true)
        .await
        .unwrap();
    let score = f
        .brain
        .evaluatelearning(&f.root, 4, Some(Optimization::Lean))
        .await
        .unwrap();
    assert_eq!(score.found, 1.0);
    assert_eq!(score.shown, 1.0);
    assert!(score.cases[0].characters > 0);
    assert!(score.cases[0].characters <= 2_800);
    assert!(score.cases[0].micros > 0);
    assert!(f.brain.evaluatelearning(&f.other, 4, None).await.is_err());
    f.brain
        .updatememory(
            f.memory,
            "Release configuration is stored elsewhere now.",
            None,
        )
        .await
        .unwrap();
    let regressed = f.brain.evaluatelearning(&f.root, 4, None).await.unwrap();
    assert_eq!(regressed.found, 0.0);
}

#[tokio::test]
async fn feedback_expiry_and_candidate_floods_preserve_reviewed_regressions() {
    let f = Fixture::new().await;
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    f.brain
        .confirmfeedback(record.id, &f.root, true)
        .await
        .unwrap();
    let case = f.brain.learningcases(&f.root).await.unwrap().remove(0);
    f.brain
        .reviewlearningcase(case.id, &f.root, true)
        .await
        .unwrap();
    sqlx::query("UPDATE learningfeedback SET updated = 0 WHERE id = ?")
        .bind(record.id)
        .execute(f.brain.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE learningcase SET created = 0 WHERE id = ?")
        .bind(case.id)
        .execute(f.brain.pool())
        .await
        .unwrap();
    let fresh = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "two")
        .await
        .unwrap();
    assert!(f.brain.feedbackrecord(record.id, &f.root).await.is_err());
    let cases = f.brain.learningcases(&f.root).await.unwrap();
    assert!(cases[0].reviewed);
    assert_eq!(cases[0].feedback, None);
    let mut transaction = f.brain.pool().begin().await.unwrap();
    for n in 0..550 {
        sqlx::query("INSERT INTO learningcase(project,feedback,query,expected,created) VALUES (?,?,?,'expected',CAST(strftime('%s','now') AS INTEGER))")
            .bind(&record.project).bind(fresh.id).bind(format!("candidate {n}"))
            .execute(&mut *transaction).await.unwrap();
    }
    transaction.commit().await.unwrap();
    f.brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "three")
        .await
        .unwrap();
    let cases = f.brain.learningcases(&f.root).await.unwrap();
    assert!(cases.len() <= 500);
    assert!(cases.iter().any(|kept| kept.id == case.id && kept.reviewed));
    assert_eq!(
        f.brain
            .evaluatelearning(&f.root, 4, None)
            .await
            .unwrap()
            .cases
            .len(),
        1
    );
}

#[tokio::test]
async fn checked_failures_veto_a_proposal_until_the_conflicting_evidence_is_reviewed() {
    let f = Fixture::new().await;
    for session in ["one", "two", "three"] {
        let record = f
            .brain
            .feedback(f.feedback(Outcome::Helpful), &f.root, session)
            .await
            .unwrap();
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .unwrap();
    }
    let proposal = f.brain.learningproposals(&f.root).await.unwrap().remove(0);
    let failed = f
        .brain
        .feedback(f.feedback(Outcome::Failed), &f.root, "four")
        .await
        .unwrap();
    f.brain
        .confirmfeedback(failed.id, &f.root, true)
        .await
        .unwrap();
    let review = f.brain.reviewlearning(proposal.id, &f.root).await.unwrap();
    assert_eq!(review.assessment.support, 3);
    assert_eq!(review.assessment.blocked, 1);
    assert!(review.fresh);
    let error = f
        .brain
        .approvelearning(proposal.id, &f.root, f._directory.path(), false)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unresolved evidence"));
    f.brain
        .confirmfeedback(failed.id, &f.root, false)
        .await
        .unwrap();
    assert_eq!(
        f.brain
            .reviewlearning(proposal.id, &f.root)
            .await
            .unwrap()
            .assessment
            .blocked,
        0
    );
}

#[tokio::test]
async fn dismissing_a_regression_case_does_not_make_it_reappear_on_the_next_confirmation() {
    let f = Fixture::new().await;
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    f.brain
        .confirmfeedback(record.id, &f.root, true)
        .await
        .unwrap();
    let case = f.brain.learningcases(&f.root).await.unwrap().remove(0);
    f.brain
        .reviewlearningcase(case.id, &f.root, false)
        .await
        .unwrap();
    f.brain
        .confirmfeedback(record.id, &f.root, true)
        .await
        .unwrap();
    assert!(f.brain.learningcases(&f.root).await.unwrap().is_empty());
    f.brain
        .reviewlearningcase(case.id, &f.root, true)
        .await
        .unwrap();
    assert!(f.brain.learningcases(&f.root).await.unwrap()[0].reviewed);
}
