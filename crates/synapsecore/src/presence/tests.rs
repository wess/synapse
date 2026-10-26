#![cfg(test)]
use super::*;
use crate::brain::Brain;

#[tokio::test]
async fn concurrent_sessions_have_separate_rooms_and_reconnect_keeps_appearance() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    let (a, b) = tokio::join!(
        join(pool, "a", "harness", "/work"),
        join(pool, "b", "harness", "/work")
    );
    a.unwrap();
    b.unwrap();
    let rows = sessions(pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].slot, rows[1].slot);
    let first = rows.iter().find(|row| row.id == "a").unwrap();
    appearance(pool, &first.identity(), 17).await.unwrap();
    let slot = first.slot;
    leave(pool, "a").await.unwrap();
    join(pool, "c", "harness", "/work").await.unwrap();
    let rows = sessions(pool).await.unwrap();
    let reconnect = rows.iter().find(|row| row.id == "c").unwrap();
    assert_eq!(reconnect.slot, slot);
    assert_eq!(reconnect.appearance, 17);
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn heartbeat_is_not_a_work_report_and_departure_expires() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    join(pool, "a", "test", "/work").await.unwrap();
    let row = sessions(pool).await.unwrap().remove(0);
    assert_eq!(row.state(now()), State::Connected);
    assert_eq!(row.state(now() + LIVESECONDS), State::Disconnected);
    report(pool, "a", "builder", "blocked", "Needs a decision")
        .await
        .unwrap();
    touch(pool, "a").await.unwrap();
    assert_eq!(
        sessions(pool).await.unwrap()[0].state(now()),
        State::Blocked
    );
    leave(pool, "a").await.unwrap();
    touch(pool, "a").await.unwrap();
    assert_eq!(
        sessions(pool).await.unwrap()[0].state(now()),
        State::Disconnected
    );
}

#[tokio::test]
async fn a_suspended_session_returns_to_its_own_room_after_reclamation() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    join(pool, "old", "test", "/work").await.unwrap();
    sqlx::query("UPDATE presence SET seen = ? WHERE id = 'old'")
        .bind(now() - LIVESECONDS)
        .execute(pool)
        .await
        .unwrap();
    join(pool, "new", "test", "/work").await.unwrap();
    touch(pool, "old").await.unwrap();
    let rows = sessions(pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.online(now())));
    assert_ne!(rows[0].slot, rows[1].slot);
}

#[tokio::test]
async fn joining_prunes_old_avatars_and_bounds_the_rest() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    sqlx::query(
        "WITH RECURSIVE ids(value) AS (SELECT 0 UNION ALL SELECT value + 1 FROM ids WHERE value < 4200) INSERT INTO sceneavatar(identity,appearance,updated) SELECT 'avatar' || value, 0, ? FROM ids",
    )
    .bind(now())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO sceneavatar(identity,appearance,updated) VALUES('old',0,?)")
        .bind(now() - AVATARRETENTION - 1)
        .execute(pool)
        .await
        .unwrap();

    join(pool, "session", "test", "/work").await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sceneavatar")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(count, AVATARLIMIT);
    let old: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sceneavatar WHERE identity = 'old'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(old, 0);
}
