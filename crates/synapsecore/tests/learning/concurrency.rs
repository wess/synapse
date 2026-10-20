use super::Fixture;
use anyhow::Context;
use std::sync::Arc;
use synapsecore::brain::{Brain, MemoryScope};
use synapsecore::usage::{self, Entry};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn memory_writes_and_passive_bookkeeping_can_arrive_together() {
    let f = Fixture::new().await;
    let peer = Brain::glance(f._directory.path().join("brain.db"))
        .await
        .unwrap();
    let ready = Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = Vec::new();
    for worker in 0..16 {
        let brain = if worker % 4 < 2 {
            f.brain.clone()
        } else {
            peer.clone()
        };
        let folder = f.root.clone();
        let ready = ready.clone();
        tasks.push(tokio::spawn(async move {
            ready.wait().await;
            for n in 0..50 {
                if worker % 2 == 0 {
                    brain
                        .rememberscoped(
                            &format!("Worker {worker} preserves release verification fact {n}."),
                            None,
                            MemoryScope::Project,
                            Some(&folder),
                        )
                        .await
                        .with_context(|| format!("memory worker {worker}, write {n}"))?;
                } else {
                    usage::record(&brain, Entry::session(1_000, 100))
                        .await
                        .with_context(|| format!("bookkeeping worker {worker}, write {n}"))?;
                }
            }
            anyhow::Ok(())
        }));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    assert_eq!(f.brain.stats().await.unwrap().entries, 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_writers_leave_connections_available_for_reads() {
    let f = Fixture::new().await;
    let held = f.brain.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let mut writers = Vec::new();
    for _ in 0..16 {
        let brain = f.brain.clone();
        writers.push(tokio::spawn(async move {
            usage::record(&brain, Entry::session(1_000, 100))
                .await
                .unwrap();
        }));
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let stats = tokio::time::timeout(std::time::Duration::from_secs(2), f.brain.stats()).await;
    held.commit().await.unwrap();
    for writer in writers {
        writer.await.unwrap();
    }
    assert_eq!(
        stats
            .expect("waiting writers occupied every read connection")
            .unwrap()
            .entries,
        1
    );
}
