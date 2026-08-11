//! What sync does to the local store.
//!
//! Deliberately not driven through a running server. `synapseserve` is its own
//! workspace on purpose — so a headless Linux box can build it without a
//! checkout of a GPUI theming library — and dev-depending on it here would undo
//! that for the sake of a test. What is worth checking anyway is not that HTTP
//! works: it is that an arriving record lands as the same memory it was on the
//! machine that made it, and that identity is stable enough for two machines to
//! converge rather than accumulate copies.

use synapsecore::brain::{Brain, MemoryScope};
use synapsecore::sync;
use tempfile::TempDir;

async fn store() -> (Brain, TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let brain = Brain::open(directory.path().join("brain.db"))
        .await
        .unwrap();
    (brain, directory)
}

#[tokio::test]
async fn a_store_with_no_server_configured_syncs_nowhere_and_says_so() {
    // The default, and it has to stay a no-op: a machine with no server keeps
    // every memory locally and loses no capability, which is also why an outage
    // is not an emergency.
    let (brain, _dir) = store().await;
    brain
        .rememberscoped("local only", Some("session"), MemoryScope::Global, None)
        .await
        .unwrap();
    assert!(sync::configured(&brain).await.unwrap().is_none());
    assert!(sync::once(&brain).await.unwrap().is_none());
    assert_eq!(brain.search("local", 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_half_configured_store_syncs_nowhere_rather_than_guessing() {
    let (brain, _dir) = store().await;
    brain
        .setpreference(sync::SERVER, "http://127.0.0.1:1")
        .await
        .unwrap();
    assert!(
        sync::configured(&brain).await.unwrap().is_none(),
        "a server alone is not a configuration"
    );
    brain
        .setpreference(sync::TOKEN, "0123456789abcdef0123456789abcdef")
        .await
        .unwrap();
    assert!(
        sync::configured(&brain).await.unwrap().is_none(),
        "a key is still missing"
    );
}

#[tokio::test]
async fn a_key_that_is_not_thirty_two_bytes_is_refused() {
    // Silently accepting a short key would seal envelopes under something
    // nobody chose, and the failure would only show as memories that will not
    // open on the other machine.
    let (brain, _dir) = store().await;
    brain
        .setpreference(sync::SERVER, "http://127.0.0.1:1")
        .await
        .unwrap();
    brain
        .setpreference(sync::TOKEN, "0123456789abcdef0123456789abcdef")
        .await
        .unwrap();
    brain.setpreference(sync::KEY, "abcd").await.unwrap();
    assert!(sync::configured(&brain).await.is_err());
}

#[tokio::test]
async fn every_memory_gets_an_identity_including_ones_older_than_sync() {
    // Turning sync on has to work on a store that has been in use for months,
    // so identities are assigned before a push rather than at write time.
    let (brain, _dir) = store().await;
    for body in [
        "uses bun",
        "postgres only",
        "deploys through site/deploy.sh",
    ] {
        brain
            .rememberscoped(body, Some("session"), MemoryScope::Global, None)
            .await
            .unwrap();
    }
    assert_eq!(sync::identify(&brain).await.unwrap(), 3);
    // Idempotent: running it again claims nothing new.
    assert_eq!(sync::identify(&brain).await.unwrap(), 0);
}

#[tokio::test]
async fn the_same_sentence_in_the_same_scope_is_one_identity() {
    // Which is what lets two machines converge instead of accumulating a copy
    // per machine, and is also why the server keys its log on (tenant, opid).
    let (first, _a) = sync::identity(
        "uses bun",
        "session",
        MemoryScope::Global,
        "",
        1_700_000_000,
    );
    let (again, _b) = sync::identity(
        "uses bun",
        "session",
        MemoryScope::Global,
        "",
        1_700_000_000,
    );
    assert_eq!(first, again);

    let (other, _c) = sync::identity(
        "uses bun",
        "session",
        MemoryScope::Project,
        "",
        1_700_000_000,
    );
    assert_ne!(first, other, "scope has to be part of what a memory is");
}

#[tokio::test]
async fn a_memory_from_another_machine_keeps_its_own_time_and_project() {
    // `rememberscoped` stamps now and derives a project from a path on this
    // disk. Neither is right for a memory that already happened somewhere else:
    // keeping both is what makes it the same memory rather than a copy dated
    // whenever it arrived.
    let (brain, _dir) = store().await;
    let id = brain
        .storeforeign(
            "written elsewhere",
            "session",
            MemoryScope::Project,
            "github.com/wess/devpipe",
            1_600_000_000,
        )
        .await
        .unwrap();

    let stored = brain
        .memory(id)
        .await
        .unwrap()
        .expect("the memory was not stored");
    assert_eq!(stored.created, 1_600_000_000);
    assert_eq!(stored.project, "github.com/wess/devpipe");
    assert_eq!(stored.body, "written elsewhere");
}
