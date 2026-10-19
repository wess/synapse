use std::time::{SystemTime, UNIX_EPOCH};
use synapsecore::brain::Brain;
use synapsecore::usage::{self, Entry, RETENTIONDAYS};

#[tokio::test]
async fn recording_usage_prunes_old_rows_and_keeps_recent_history() {
    let directory = tempfile::tempdir().unwrap();
    let brain = Brain::open(directory.path().join("brain.db"))
        .await
        .unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for age in [RETENTIONDAYS + 1, 1] {
        sqlx::query(
            "INSERT INTO usage(kind, memories, reach, matched, sent, overhead, created) \
       VALUES ('session', 0, 100, 0, 0, 0, ?)",
        )
        .bind(now - age * 86_400)
        .execute(brain.pool())
        .await
        .unwrap();
    }

    usage::record(&brain, Entry::session(200, 0)).await.unwrap();
    let report = usage::report(&brain, None).await.unwrap();
    assert_eq!(report.totals.sessions, 2);
    assert_eq!(report.totals.reach, 300);
}
