use synapsecore::brain::Brain;

#[tokio::test]
async fn a_dense_store_has_bounded_drift_output() {
    let directory = tempfile::tempdir().unwrap();
    let brain = Brain::open(directory.path().join("brain.db"))
        .await
        .unwrap();
    let mut transaction = brain.pool().begin().await.unwrap();
    for id in 1..=5_000 {
        sqlx::query("INSERT INTO memory(rowid, body, source, created) VALUES (?, ?, '', ?)")
            .bind(id)
            .bind("Always keep server logs for seven days during production deployments.")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO memorymeta(memoryid, scope, project, native, created) \
         SELECT rowid, 'global', '', 1, CAST(created AS INTEGER) FROM memory",
    )
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    let drift = brain.drift(None).await.unwrap();
    assert!(drift.partial);
    assert!(drift.findings.len() <= 1_000);
}
