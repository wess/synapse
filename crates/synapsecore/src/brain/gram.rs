//! Keeping the trigram index in step with `memory`.
//!
//! `memorygram` holds no text of its own; it indexes the rows of `memory` by
//! rowid. SQLite does not update an index like that by itself, and a virtual
//! table cannot carry a trigger, so every statement that changes `memory` is
//! paired with one of these: `unindex` before a row changes or goes, `index`
//! after it is written.
//!
//! An index that drifts does not fail loudly. It returns memories for words
//! they no longer contain and misses ones they do, so the store test that
//! drives every write path ends with SQLite's own integrity check.

use anyhow::{Context, Result};
use sqlx::SqliteConnection;

/// Adds memory `id`, as it now stands, to the index.
pub(super) async fn index(connection: &mut SqliteConnection, id: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO memorygram(rowid, body, source) \
         SELECT rowid, body, source FROM memory WHERE rowid = ?",
    )
    .bind(id)
    .execute(connection)
    .await
    .with_context(|| format!("could not index memory #{id}"))?;
    Ok(())
}

/// Takes memory `id` out of the index.
pub(super) async fn unindex(connection: &mut SqliteConnection, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM memorygram WHERE rowid = ?")
        .bind(id)
        .execute(connection)
        .await
        .with_context(|| format!("could not remove memory #{id} from the index"))?;
    Ok(())
}

/// Empties the index, for when every memory is going.
pub(super) async fn clear(connection: &mut SqliteConnection) -> Result<()> {
    sqlx::query("DELETE FROM memorygram")
        .execute(connection)
        .await
        .context("could not clear the search index")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::brain::{Brain, MemoryScope::Global};
    use crate::imports::{ImportCandidate, ImportProvider};
    use std::path::PathBuf;

    /// The rows the index holds and the rows `memory` holds, which must be the
    /// same set after every kind of write.
    async fn drift(brain: &Brain) -> (Vec<i64>, Vec<i64>) {
        let rows = |sql: &'static str| async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_all(brain.pool())
                .await
                .unwrap()
        };
        (
            rows("SELECT rowid FROM memory ORDER BY rowid").await,
            rows("SELECT rowid FROM memorygram ORDER BY rowid").await,
        )
    }

    async fn fragment(brain: &Brain, text: &str) -> Vec<i64> {
        sqlx::query_scalar::<_, i64>("SELECT rowid FROM memorygram WHERE memorygram MATCH ?")
            .bind(format!("\"{text}\""))
            .fetch_all(brain.pool())
            .await
            .unwrap()
    }

    fn assert_in_step((memory, gram): (Vec<i64>, Vec<i64>), after: &str) {
        assert_eq!(memory, gram, "the index drifted from memory after {after}");
    }

    #[tokio::test]
    async fn every_write_keeps_the_index_in_step() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();

        let kept = brain
            .rememberscoped("Deploys use scripts/release.sh", None, Global, None)
            .await
            .unwrap();
        let gone = brain
            .rememberscoped("Temporary note about flamingos", None, Global, None)
            .await
            .unwrap();
        brain
            .storeforeign("Arrived from another machine", "sync", Global, "", 1)
            .await
            .unwrap();
        assert_in_step(drift(&brain).await, "remembering");

        brain
            .updatememory(kept, "Deploys use scripts/ship.sh", None)
            .await
            .unwrap();
        assert_in_step(drift(&brain).await, "an edit");
        assert_eq!(fragment(&brain, "ship.sh").await, vec![kept]);
        assert!(
            fragment(&brain, "release.sh").await.is_empty(),
            "an edit must take the old text out of the index"
        );

        brain.deletememory(gone).await.unwrap();
        assert_in_step(drift(&brain).await, "a delete");
        assert!(fragment(&brain, "flamingo").await.is_empty());

        let candidate = ImportCandidate {
            provider: ImportProvider::Markdown,
            externalid: "one".to_owned(),
            body: "Imported rule about pelicans".to_owned(),
            source: "Markdown · test".to_owned(),
            scope: Global,
            project: String::new(),
            path: PathBuf::from("test.md"),
            created: 1,
            warning: None,
        };
        let preview = brain
            .importpreview(ImportProvider::Markdown, vec![candidate])
            .await
            .unwrap();
        let report = brain.importmemories(preview, false).await.unwrap();
        assert_in_step(drift(&brain).await, "an import");
        assert_eq!(fragment(&brain, "pelican").await.len(), 1);
        brain.undoimport(report.batch.id).await.unwrap();
        assert_in_step(drift(&brain).await, "undoing an import");
        assert!(fragment(&brain, "pelican").await.is_empty());

        brain.wipememories().await.unwrap();
        assert_in_step(drift(&brain).await, "a wipe");
        assert!(drift(&brain).await.1.is_empty());
    }
}
