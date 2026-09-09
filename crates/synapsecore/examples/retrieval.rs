//! synthetic retrieval measurements in a disposable store; no user data or network.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::future::Future;
use std::time::Instant;
use synapsecore::brain::{Brain, Optimization, RecallResponse};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() <= 2, "usage: retrieval [memories] [iterations]");
    let count = argument(&args, 0, 10_000)?;
    let iterations = argument(&args, 1, 30)?;
    anyhow::ensure!(
        (1..=1_000_000).contains(&count),
        "memories must be 1..1000000"
    );
    anyhow::ensure!(
        (1..=1_000).contains(&iterations),
        "iterations must be 1..1000"
    );
    let folder = tempfile::tempdir()?;
    let database = folder.path().join("brain.db");
    let brain = Brain::open(&database).await?;
    let started = Instant::now();
    seed(&brain, count).await?;
    let seed_ms = started.elapsed().as_secs_f64() * 1_000.0;
    brain.pool().close().await;
    drop(brain);
    let started = Instant::now();
    let brain = Brain::open(&database).await?;
    let open_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let mut measurements = Vec::new();
    for (name, query) in [
        ("selective", "zebra"),
        ("common", "database"),
        ("recent", ""),
    ] {
        measurements.push(
            measure(name, iterations, || async {
                let (settings, memories) = brain
                    .recallscoped(query, 4, Some(Optimization::Lean), None)
                    .await?;
                let response = RecallResponse {
                    optimization: settings.optimization,
                    memories,
                };
                Ok(serde_json::to_vec(&response)?.len())
            })
            .await?,
        );
    }
    measurements.push(
        measure("readmemory", iterations, || async {
            let response = brain
                .readscoped(1, 0, Some(Optimization::Lean), None)
                .await?;
            Ok(serde_json::to_vec(&response)?.len())
        })
        .await?,
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "memories": count,
            "iterations": iterations,
            "seed_ms": seed_ms,
            "database_open_ms": open_ms,
            "database_bytes": std::fs::metadata(&database)?.len(),
            "measurements": measurements,
        }))?
    );
    brain.pool().close().await;
    Ok(())
}

fn argument(args: &[String], index: usize, default: usize) -> Result<usize> {
    args.get(index)
        .map(|value| value.parse().context("expected an integer"))
        .unwrap_or(Ok(default))
}

async fn seed(brain: &Brain, count: usize) -> Result<()> {
    let mut transaction = brain.pool().begin().await?;
    for id in 1..=count {
        let body = format!(
            "Project {} memory {id}. Release builds validate signed packages and database migrations. \
             The worker pool handles connection scheduling and storage durability for repeated operations. {}",
            id % 100,
            if id % 1_000 == 0 {
                "zebra pinning policy"
            } else {
                "ordinary configuration guidance"
            },
        );
        sqlx::query("INSERT INTO memory(rowid, body, source, created) VALUES (?, ?, 'fixture', ?)")
            .bind(id as i64)
            .bind(body)
            .bind(1_788_000_000 + id as i64)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query(
        "INSERT INTO memorymeta(memoryid, scope, project, native, created) \
         SELECT rowid, 'global', '', 1, CAST(created AS INTEGER) FROM memory",
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn measure<F, Fut>(name: &str, iterations: usize, mut run: F) -> Result<Value>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<usize>>,
{
    run().await?;
    let mut times = Vec::with_capacity(iterations);
    let mut bytes = 0;
    for _ in 0..iterations {
        let started = Instant::now();
        bytes = bytes.max(run().await?);
        times.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    times.sort_by(f64::total_cmp);
    Ok(json!({
        "name": name,
        "p50_ms": times[iterations.div_ceil(2) - 1],
        "p95_ms": times[(iterations * 95).div_ceil(100) - 1],
        "max_ms": times[iterations - 1],
        "structured_response_bytes": bytes,
    }))
}
