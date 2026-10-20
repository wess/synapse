use super::Fixture;
use std::sync::Arc;
use synapsecore::brain::MemoryScope;
use synapsecore::usage::{self, Entry};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn memory_writes_and_passive_bookkeeping_can_arrive_together() {
    let f = Fixture::new().await;
    let ready = Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = Vec::new();
    for worker in 0..16 {
        let brain = f.brain.clone();
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
                        .await?;
                } else {
                    usage::record(&brain, Entry::session(1_000, 100)).await?;
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
