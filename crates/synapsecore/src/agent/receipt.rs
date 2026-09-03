//! What a connection was made from.
//!
//! A descriptor is data now, which means it moves: a release can teach Synapse
//! that Ainz wants `--required`, or that a tool's config file is somewhere else,
//! and every machine already connected under the old answer keeps the old
//! answer. Detection cannot see that. It compares the stored command against
//! this binary, so a registration that is present and points at the right place
//! reads as healthy however it was written.
//!
//! So the descriptor's digest is recorded when Synapse connects the tool, and
//! compared against the descriptor that resolves now. A difference is not a
//! fault — it is "this release would connect it differently", which is exactly
//! what a person needs told before they can decide to re-apply it. The same
//! bargain [`crate::skill::Receipts`] makes for an installed skill.
//!
//! An absent row means nothing to compare, never out of date. Every machine
//! that connected its tools before this release has no rows here, and marking
//! all of them stale would be a worse lie than saying nothing.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The digest of a descriptor's text, as stored.
pub fn digest(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Record what `slug` was connected from. Replaces any earlier row: a tool is
/// connected from one descriptor at a time, and the newest answer is the one a
/// comparison should be made against.
pub async fn record(database: &Path, slug: &str, text: &str) -> Result<()> {
    let opened = crate::database::glance(database).await?;
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default();
    sqlx::query(
        "INSERT INTO toolconnect(slug, descriptor, connected) VALUES(?, ?, ?) \
         ON CONFLICT(slug) DO UPDATE SET descriptor = excluded.descriptor, \
         connected = excluded.connected",
    )
    .bind(slug)
    .bind(digest(text))
    .bind(seconds)
    .execute(&opened.pool)
    .await
    .with_context(|| format!("could not record the {slug} connection"))?;
    Ok(())
}

/// Forget what `slug` was connected from, because it is no longer connected.
/// Leaving the row would make a reconnection look current the moment it
/// happened, whatever descriptor it actually used.
pub async fn forget(database: &Path, slug: &str) -> Result<()> {
    let opened = crate::database::glance(database).await?;
    sqlx::query("DELETE FROM toolconnect WHERE slug = ?")
        .bind(slug)
        .execute(&opened.pool)
        .await
        .with_context(|| format!("could not clear the {slug} connection record"))?;
    Ok(())
}

/// Every recorded digest, by slug. One query rather than one per tool: the
/// dashboard asks about the whole list on every redraw.
pub async fn recorded(database: &Path) -> Result<HashMap<String, String>> {
    let opened = crate::database::glance(database).await?;
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT slug, descriptor FROM toolconnect")
        .fetch_all(&opened.pool)
        .await
        .context("could not read the connection records")?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn the_digest_is_stable_and_notices_a_changed_descriptor() {
        let before = "name = \"T\"\ncommand = \"t\"\n";
        assert_eq!(digest(before), digest(before));
        assert_ne!(digest(before), digest("name = \"T\"\ncommand = \"t2\"\n"));
        // Sixty-four hex characters, so a row's width is known.
        assert_eq!(digest(before).len(), 64);
    }

    #[tokio::test]
    async fn a_recorded_connection_survives_and_a_forgotten_one_does_not() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("brain.db");

        // Nothing recorded is not an error, and reads as nothing to compare.
        assert!(recorded(&database).await.unwrap().is_empty());

        record(&database, "ainz", "name = \"Ainz\"\n")
            .await
            .unwrap();
        let held = recorded(&database).await.unwrap();
        assert_eq!(held.get("ainz"), Some(&digest("name = \"Ainz\"\n")));

        // Connecting again from a moved descriptor replaces the row rather than
        // adding a second answer for one tool.
        record(&database, "ainz", "name = \"Ainz\"\ncommand = \"ainz\"\n")
            .await
            .unwrap();
        let held = recorded(&database).await.unwrap();
        assert_eq!(held.len(), 1);
        assert_eq!(
            held.get("ainz"),
            Some(&digest("name = \"Ainz\"\ncommand = \"ainz\"\n"))
        );

        forget(&database, "ainz").await.unwrap();
        assert!(recorded(&database).await.unwrap().is_empty());
        // Forgetting what was never there is not a failure.
        forget(&database, "ainz").await.unwrap();
    }
}
