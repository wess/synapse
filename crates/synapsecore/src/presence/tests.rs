#![cfg(test)]
use super::*;
use crate::brain::Brain;

/// An id whose process is this one, so the session reads as running.
fn running(label: &str) -> String {
    format!("{}-{label}", std::process::id())
}

/// An id whose process has already exited.
fn exited(label: &str) -> String {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--list")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let process = child.id();
    child.wait().unwrap();
    format!("{process}-{label}")
}

#[tokio::test]
async fn concurrent_sessions_have_separate_slots_and_reconnect_reuses_one() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    let (a, b, c) = (running("a"), running("b"), running("c"));
    let (first, second) = tokio::join!(
        join(pool, &a, "harness", "/work"),
        join(pool, &b, "harness", "/work")
    );
    first.unwrap();
    second.unwrap();
    let rows = sessions(pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].slot, rows[1].slot);
    let slot = rows.iter().find(|row| row.id == a).unwrap().slot;
    leave(pool, &a).await.unwrap();
    join(pool, &c, "harness", "/work").await.unwrap();
    let rows = sessions(pool).await.unwrap();
    let reconnect = rows.iter().find(|row| row.id == c).unwrap();
    assert_eq!(reconnect.slot, slot);
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn a_work_report_changes_the_state_and_a_departure_ends_it() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    let id = running("a");
    join(pool, &id, "test", "/work").await.unwrap();
    assert_eq!(sessions(pool).await.unwrap()[0].state(), State::Connected);
    report(pool, &id, "builder", "blocked", "Needs a decision")
        .await
        .unwrap();
    assert_eq!(sessions(pool).await.unwrap()[0].state(), State::Blocked);
    leave(pool, &id).await.unwrap();
    // A report that arrives after the goodbye must not bring the session back.
    report(pool, &id, "builder", "working", "").await.unwrap();
    assert_eq!(
        sessions(pool).await.unwrap()[0].state(),
        State::Disconnected
    );
}

#[tokio::test]
async fn lifecycle_changes_are_visible_in_the_event_ledger() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let backend = crate::backend::Backend::sharing(&brain).unwrap();
    let pool = brain.pool();
    let id = running("events");

    join(pool, &id, "test", "/work").await.unwrap();
    report(pool, &id, "builder", "working", "building")
        .await
        .unwrap();
    leave(pool, &id).await.unwrap();

    let events = backend
        .events(&crate::backend::EventFilter {
            source: Some("harness".into()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>(),
        ["harness.join", "harness.status", "harness.leave"]
    );
}

#[tokio::test]
async fn a_quiet_session_stays_and_keeps_its_room_however_long_it_says_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    let (quiet, new) = (running("quiet"), running("new"));
    join(pool, &quiet, "test", "/work").await.unwrap();
    // Nothing refreshes `seen` while a session idles, so a week of silence is
    // what an open, unused session looks like — not what a dead one does.
    sqlx::query("UPDATE presence SET seen = ?, started = ? WHERE id = ?")
        .bind(now() - 7 * 86400)
        .bind(now() - 7 * 86400)
        .bind(&quiet)
        .execute(pool)
        .await
        .unwrap();
    join(pool, &new, "test", "/work").await.unwrap();
    let rows = sessions(pool).await.unwrap();
    assert_eq!(rows.len(), 2, "joining must not prune a session still open");
    assert!(rows.iter().all(Session::online));
    assert_ne!(rows[0].slot, rows[1].slot);
}

#[tokio::test]
async fn a_session_whose_process_is_gone_reads_as_ended_and_gives_up_its_room() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    let (killed, next) = (exited("killed"), running("next"));
    // Killed rather than closed: it joined and never said goodbye.
    join(pool, &killed, "test", "/work").await.unwrap();
    let rows = sessions(pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state(), State::Disconnected);
    assert!(
        rows[0].ended > 0,
        "whoever looks next ends it on its behalf"
    );
    let slot = rows[0].slot;

    join(pool, &next, "test", "/work").await.unwrap();
    let rows = sessions(pool).await.unwrap();
    assert_eq!(rows.len(), 1, "one room, and the running session is in it");
    assert_eq!(rows[0].id, next);
    assert_eq!(rows[0].slot, slot);
    assert!(rows[0].online());
}

#[tokio::test]
async fn an_id_that_names_no_process_is_never_drawn_as_connected() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    for id in ["nameless", "-7", "0-zero", "4294967295-everything"] {
        join(pool, id, "test", id).await.unwrap();
    }
    let rows = sessions(pool).await.unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| !row.online()));
}

#[tokio::test]
async fn an_ended_session_is_forgotten_a_day_after_it_ended_not_after_it_last_spoke() {
    let folder = tempfile::tempdir().unwrap();
    let brain = Brain::open(folder.path().join("brain.db")).await.unwrap();
    let pool = brain.pool();
    let (old, recent) = (running("old"), running("recent"));
    join(pool, &old, "test", "/old").await.unwrap();
    join(pool, &recent, "test", "/recent").await.unwrap();
    leave(pool, &old).await.unwrap();
    leave(pool, &recent).await.unwrap();
    sqlx::query("UPDATE presence SET ended = ? WHERE id = ?")
        .bind(now() - RETENTION - 1)
        .bind(&old)
        .execute(pool)
        .await
        .unwrap();
    join(pool, &running("now"), "test", "/now").await.unwrap();
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM presence ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(!ids.contains(&old));
    assert!(ids.contains(&recent));
}
