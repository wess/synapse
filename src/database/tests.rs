use super::*;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

#[tokio::test]
async fn migrates_existing_data_and_creates_backup() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("synapse");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("brain.db");
    let options = SqliteConnectOptions::from_str("sqlite://test")
        .unwrap()
        .filename(&path)
        .create_if_missing(true);
    let old = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE setting(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .execute(&old)
        .await
        .unwrap();
    sqlx::query("INSERT INTO setting(key, value) VALUES ('appearance', 'dark')")
        .execute(&old)
        .await
        .unwrap();
    old.close().await;

    let opened = open(&path).await.unwrap();
    assert_eq!(version(&opened.pool).await.unwrap(), migration::LATEST);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT value FROM setting WHERE key = 'appearance'")
            .fetch_one(&opened.pool)
            .await
            .unwrap(),
        "dark"
    );
    opened.pool.close().await;
    drop(opened.lock);
    let backupfolder = folder.join("backups");
    let backups = std::fs::read_dir(&backupfolder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.with_extension("lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(backupfolder)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&backups[0]).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn exports_checks_and_restores_consistent_database() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("synapse");
    let path = folder.join("brain.db");
    let exported = directory.path().join("export.db");

    let opened = open(&path).await.unwrap();
    sqlx::query("INSERT INTO memory(body, source, created) VALUES ('original', 'test', 1)")
        .execute(&opened.pool)
        .await
        .unwrap();
    opened.pool.close().await;
    drop(opened.lock);
    export(&path, &exported).await.unwrap();

    let opened = open(&path).await.unwrap();
    sqlx::query("UPDATE memory SET body = 'changed'")
        .execute(&opened.pool)
        .await
        .unwrap();
    opened.pool.close().await;
    drop(opened.lock);

    let backup = restore(&path, &exported).await.unwrap().unwrap();
    assert!(backup.is_file());
    let report = check(&path).await.unwrap();
    assert_eq!(report.integrity, "ok");
    let opened = open(&path).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT body FROM memory")
            .fetch_one(&opened.pool)
            .await
            .unwrap(),
        "original"
    );
}

#[tokio::test]
async fn rejects_foreign_key_corruption() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("synapse");
    let path = folder.join("brain.db");
    let opened = open(&path).await.unwrap();
    opened.pool.close().await;
    drop(opened.lock);

    let options = SqliteConnectOptions::from_str("sqlite://corrupt")
        .unwrap()
        .filename(&path)
        .foreign_keys(false);
    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO secret(vaultid, name, env, account, created) \
         VALUES (999, 'token', 'TOKEN', 'missing.token', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let error = match open(&path).await {
        Ok(_) => panic!("foreign key corruption was accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("relationship check failed"));
}
