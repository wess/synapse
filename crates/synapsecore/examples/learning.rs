//! on-demand learning cost in a disposable store, including full ledgers.

use anyhow::Result;
use serde_json::{Value, json};
use std::future::Future;
use std::time::Instant;
use synapsecore::brain::{Brain, MemoryScope, Optimization};
use synapsecore::learning::{Disposition, Feedback, Outcome, Target};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let iterations: usize = std::env::args()
        .nth(1)
        .map(|n| n.parse())
        .transpose()?
        .unwrap_or(100);
    anyhow::ensure!(
        (1..=1_000).contains(&iterations),
        "iterations must be 1..1000"
    );
    let directory = tempfile::tempdir()?;
    let folder = directory.path().join("app");
    std::fs::create_dir_all(folder.join(".git"))?;
    let path = directory.path().join("brain.db");
    let brain = Brain::open(&path).await?;
    brain.setlearn(true).await?;
    seed(&brain).await?;
    let first = brain
        .rememberscoped(
            "Always squash commits when merging a pull request into main.",
            None,
            MemoryScope::Project,
            Some(&folder),
        )
        .await?;
    let second = brain
        .rememberscoped(
            "Never squash commits when merging a pull request into main.",
            None,
            MemoryScope::Project,
            Some(&folder),
        )
        .await?;
    let input = Feedback {
        target: Target::Memory { id: first },
        outcome: Outcome::Helpful,
        problem: "Release branches require a documented merge procedure.".into(),
        correction: "Check the branch policy and preserve reviewed history.".into(),
        evidence: "The release verification confirmed the intended history.".into(),
        query: String::new(),
        expected: String::new(),
    };
    let record = brain.feedback(input.clone(), &folder, "benchmark").await?;
    let mut measurements = Vec::new();
    measurements.push(
        measure("recall_with_unresolved_drift", iterations, || async {
            let (_, memories) = brain
                .recallscoped(
                    "squash commits merging",
                    4,
                    Some(Optimization::Lean),
                    Some(&folder),
                )
                .await?;
            let findings = brain.drifted(&memories, Some(&folder)).await?;
            Ok(serde_json::to_vec(&findings)?.len())
        })
        .await?,
    );
    let finding = brain.related(second, Some(&folder)).await?.remove(0);
    let resolution = brain
        .resolvefinding(
            &folder,
            &finding,
            Disposition::Contextual,
            "Different release branches use different policies.",
        )
        .await?;
    let mut transaction = brain.pool().begin().await?;
    for n in 0..4_999 {
        sqlx::query("INSERT INTO learningfeedback(project,target,digest,lesson,session,outcome,problem,correction,evidence,created,updated) \
            VALUES (?,?,?,?,?,'unresolved','fixture','','',CAST(strftime('%s','now') AS INTEGER),CAST(strftime('%s','now') AS INTEGER))")
            .bind(&record.project).bind(&record.target).bind(&record.digest)
            .bind(format!("fixture-{n}")).bind(format!("session-{n}"))
            .execute(&mut *transaction).await?;
    }
    for n in 0..1_999 {
        sqlx::query(
            "INSERT INTO driftresolution(project,fingerprint,memoryid,disposition,note,created) \
            VALUES (?,?,?,'falsepositive','fixture',CAST(strftime('%s','now') AS INTEGER))",
        )
        .bind(&record.project)
        .bind(format!("fixture-{n}"))
        .bind(finding.ids[0])
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    measurements.push(
        measure("recall_with_full_resolution_ledger", iterations, || async {
            let (_, memories) = brain
                .recallscoped(
                    "squash commits merging",
                    4,
                    Some(Optimization::Lean),
                    Some(&folder),
                )
                .await?;
            let findings = brain.drifted(&memories, Some(&folder)).await?;
            anyhow::ensure!(findings.is_empty(), "the saved assessment was lost");
            Ok(serde_json::to_vec(&findings)?.len())
        })
        .await?,
    );
    measurements.push(
        measure("feedback_with_full_ledgers", iterations, || async {
            let updated = brain.feedback(input.clone(), &folder, "benchmark").await?;
            Ok(serde_json::to_vec(&updated)?.len())
        })
        .await?,
    );
    brain.restoreresolution(resolution.id, &folder).await?;
    measurements.push(
        measure("confirm_with_full_ledgers", iterations, || async {
            let updated = brain.confirmfeedback(record.id, &folder, true).await?;
            Ok(serde_json::to_vec(&updated)?.len())
        })
        .await?,
    );
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM learningfeedback),(SELECT COUNT(*) FROM driftresolution)",
    )
    .fetch_one(brain.pool())
    .await?;
    brain.pool().close().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "memories":10_002,"iterations":iterations,"feedback_rows":counts.0,
            "resolution_rows":counts.1,"database_bytes":std::fs::metadata(&path)?.len(),
            "measurements":measurements,
        }))?
    );
    Ok(())
}

async fn measure<F, Fut>(name: &str, iterations: usize, mut work: F) -> Result<Value>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<usize>>,
{
    work().await?;
    let mut times = Vec::with_capacity(iterations);
    let mut bytes = 0;
    for _ in 0..iterations {
        let start = Instant::now();
        bytes = work().await?;
        times.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    times.sort_by(f64::total_cmp);
    Ok(json!({"name":name,"p50_ms":times[iterations/2],
        "p95_ms":times[(iterations*95/100).min(iterations-1)],"response_bytes":bytes}))
}

async fn seed(brain: &Brain) -> Result<()> {
    let mut transaction = brain.pool().begin().await?;
    for n in 0..10_000 {
        sqlx::query("INSERT INTO memory(body,source,created) VALUES (?,'fixture',1700000000)")
            .bind(format!(
                "Service {n} validates database connection scheduling and storage durability."
            ))
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query("INSERT INTO memorymeta(memoryid,scope,project,native,created) SELECT rowid,'global','',1,CAST(created AS INTEGER) FROM memory")
        .execute(&mut *transaction).await?;
    sqlx::query("INSERT INTO memorygram(rowid,body,source) SELECT rowid,body,source FROM memory")
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}
