use super::{Fixture, Outcome, Verification};
use std::ffi::OsString;
use std::time::Duration;

fn command(code: u8) -> Vec<OsString> {
    #[cfg(unix)]
    let args = vec!["sh".to_owned(), "-c".into(), format!("exit {code}")];
    #[cfg(windows)]
    let args = vec!["cmd.exe".to_owned(), "/C".into(), format!("exit {code}")];
    args.into_iter().map(OsString::from).collect()
}

#[tokio::test]
async fn observed_checks_record_exit_status_without_resolving_an_unknown_cause() {
    let f = Fixture::new().await;
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Unresolved), &f.root, "one")
        .await
        .unwrap();
    let pass = f
        .brain
        .checkfeedback(record.id, &f.root, &command(0), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(pass.exit, Some(0));
    assert!(!pass.timedout);
    assert_eq!(pass.feedback.verification, Verification::Checkpassed);
    assert_eq!(pass.feedback.outcome, Outcome::Unresolved);
    assert!(!pass.feedback.qualified());
    let fail = f
        .brain
        .checkfeedback(record.id, &f.root, &command(7), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(fail.exit, Some(7));
    assert_eq!(fail.feedback.verification, Verification::Checkfailed);
    assert!(f.brain.learningcases(&f.root).await.unwrap().is_empty());
    assert!(
        f.brain
            .checkfeedback(record.id, &f.other, &command(0), Duration::from_secs(5))
            .await
            .is_err()
    );
    assert!(
        f.brain
            .checkfeedback(record.id, &f.root, &command(0), Duration::from_secs(301))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn checks_preserve_user_verdicts_and_passing_retries_do_not_erase_failures() {
    let f = Fixture::new().await;
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    f.brain
        .confirmfeedback(record.id, &f.root, false)
        .await
        .unwrap();
    let pass = f
        .brain
        .checkfeedback(record.id, &f.root, &command(0), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(pass.feedback.judgment, "rejected");
    assert_eq!(pass.feedback.checkstate, "passed");
    assert!(!pass.feedback.qualified());
    assert!(
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .unwrap()
            .qualified()
    );
    let fail = f
        .brain
        .checkfeedback(record.id, &f.root, &command(1), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(fail.feedback.judgment, "accepted");
    assert_eq!(fail.feedback.checkstate, "failed");
    assert!(fail.feedback.unstable);
    let retry = f
        .brain
        .checkfeedback(record.id, &f.root, &command(0), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(retry.feedback.unstable);
    assert!(!retry.feedback.qualified());
    assert!(
        f.brain
            .confirmfeedback(record.id, &f.root, true)
            .await
            .unwrap()
            .qualified()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn slow_checks_are_killed_and_cannot_turn_into_verified_success() {
    let f = Fixture::new().await;
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    let command = [
        OsString::from("sh"),
        OsString::from("-c"),
        OsString::from("sleep 30"),
    ];
    let check = f
        .brain
        .checkfeedback(record.id, &f.root, &command, Duration::from_millis(100))
        .await
        .unwrap();
    assert!(check.timedout);
    assert_eq!(check.exit, None);
    assert_eq!(check.feedback.verification, Verification::Checktimedout);
    assert!(!check.feedback.qualified());
}

#[cfg(unix)]
#[tokio::test]
async fn evidence_cannot_be_replaced_while_a_check_is_running() {
    let f = Fixture::new().await;
    let record = f
        .brain
        .feedback(f.feedback(Outcome::Helpful), &f.root, "one")
        .await
        .unwrap();
    let command = [
        OsString::from("sh"),
        OsString::from("-c"),
        OsString::from("sleep 0.3"),
    ];
    let checking = f
        .brain
        .checkfeedback(record.id, &f.root, &command, Duration::from_secs(5));
    let change = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        f.brain
            .feedback(f.feedback(Outcome::Unresolved), &f.root, "one")
            .await
            .unwrap();
    };
    let (result, _) = tokio::join!(checking, change);
    assert!(result.unwrap_err().to_string().contains("feedback changed"));
    assert_eq!(
        f.brain
            .feedbackrecord(record.id, &f.root)
            .await
            .unwrap()
            .verification,
        Verification::Reported
    );
}
