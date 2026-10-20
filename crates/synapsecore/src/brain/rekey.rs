//! Moving project memory from a folder's path to its repository's key.
//!
//! Memory written before projects were keyed by `origin` is filed under an
//! absolute path, and recall now looks for the key — so without this, upgrading
//! would hide every project memory a user has. It cannot be a schema migration:
//! the key comes from each repository's config on this disk, which SQL cannot
//! read.

use crate::brain::{Brain, MemoryScope};
use anyhow::{Context, Result};
use std::path::Path;

impl Brain {
    /// Re-files every project stored under a path that is now a repository
    /// with an `origin`, and returns how many memories moved.
    ///
    /// Runs on every open, because the answer can change after the first: a
    /// remote added later, a store synced from a machine still on paths, or a
    /// folder that was not checked out yet. It costs one indexed read of the
    /// distinct projects and a config read for each that is still a path.
    ///
    /// Only a path that is itself a repository root moves. Walking up to find
    /// one would re-file memory from a plain folder under whatever repository
    /// happens to contain it, which is a different project.
    pub(super) async fn rekey(&self) -> Result<usize> {
        let projects = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT project FROM memorymeta \
             WHERE scope = 'project'",
        )
        .fetch_all(&self.pool)
        .await
        .context("could not read project keys")?;

        let mut moved = 0;
        for project in projects {
            let path = Path::new(&project);
            if !path.is_absolute() {
                continue;
            }
            let Some(key) = super::origin::origin(path) else {
                continue;
            };
            moved += self.move_project(&project, &key).await?;
        }
        Ok(moved)
    }

    /// One project's memory, moved in one transaction with its sync identity.
    ///
    /// A synced memory's identity is a digest that includes its project, so it
    /// is recomputed and marked unsent. Every machine that moves the same
    /// memory arrives at the same new identity, so the copies meet as one
    /// memory on the server instead of arriving at each other as duplicates.
    /// Where two checkouts of one repository each hold an identical memory, the
    /// second keeps its old identity rather than colliding with the first.
    async fn move_project(&self, from: &str, to: &str) -> Result<usize> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let rows = sqlx::query_as::<_, (i64, String, String, i64)>(
            "SELECT meta.memoryid, memory.body, memory.source, CAST(memory.created AS INTEGER) \
             FROM memorymeta meta JOIN memory ON memory.rowid = meta.memoryid \
             WHERE meta.scope = 'project' AND meta.project = ?",
        )
        .bind(from)
        .fetch_all(&mut *transaction)
        .await?;
        sqlx::query("UPDATE memorymeta SET project = ? WHERE scope = 'project' AND project = ?")
            .bind(to)
            .bind(from)
            .execute(&mut *transaction)
            .await
            .with_context(|| format!("could not re-file memory for {from}"))?;
        for (id, body, source, created) in &rows {
            let (opid, _) = crate::sync::identity(body, source, MemoryScope::Project, to, *created);
            sqlx::query("UPDATE OR IGNORE memorysync SET opid = ?, pushed = 0 WHERE memoryid = ?")
                .bind(opid)
                .bind(id)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(rows.len())
    }
}

#[cfg(test)]
mod tests {
    use crate::brain::{Brain, MemoryScope};

    fn repository(root: &std::path::Path, url: &str) {
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(
            root.join(".git/config"),
            format!("[remote \"origin\"]\n\turl = {url}\n"),
        )
        .unwrap();
    }

    /// The upgrade case: memory filed under a checkout's path before keys
    /// existed has to be recalled by that checkout afterwards.
    #[tokio::test]
    async fn memory_filed_under_a_path_moves_to_the_repository_key() {
        let directory = tempfile::tempdir().unwrap();
        let checkout = directory.path().join("app");
        repository(&checkout, "git@github.com:wess/app.git");
        let path = checkout.canonicalize().unwrap().display().to_string();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let id = brain
            .storeforeign(
                "Deploys go through CI",
                "test",
                MemoryScope::Project,
                &path,
                1,
            )
            .await
            .unwrap();

        assert_eq!(brain.rekey().await.unwrap(), 1);
        assert_eq!(
            brain.memory(id).await.unwrap().unwrap().project,
            "github.com/wess/app"
        );
        let (_, recalled) = brain
            .recallscoped("deploys", 5, None, Some(&checkout))
            .await
            .unwrap();
        assert_eq!(recalled.len(), 1);
        assert_eq!(
            brain.rekey().await.unwrap(),
            0,
            "a second pass moves nothing"
        );
    }

    /// A second checkout of the same repository, anywhere, sees the memory.
    #[tokio::test]
    async fn another_checkout_of_the_same_repository_shares_its_memory() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("one");
        let second = directory.path().join("elsewhere").join("two");
        repository(&first, "https://github.com/wess/app.git");
        repository(&second, "git@github.com:wess/app");
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped("Use bun", None, MemoryScope::Project, Some(&first))
            .await
            .unwrap();

        let (_, recalled) = brain
            .recallscoped("bun", 5, None, Some(&second))
            .await
            .unwrap();
        assert_eq!(recalled.len(), 1);
    }

    #[tokio::test]
    async fn a_moved_memory_gets_a_new_identity_and_is_sent_again() {
        let directory = tempfile::tempdir().unwrap();
        let checkout = directory.path().join("app");
        repository(&checkout, "https://github.com/wess/app");
        let path = checkout.canonicalize().unwrap().display().to_string();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let id = brain
            .storeforeign("Pools at five", "test", MemoryScope::Project, &path, 7)
            .await
            .unwrap();
        crate::sync::identify(&brain).await.unwrap();
        sqlx::query("UPDATE memorysync SET pushed = 1")
            .execute(brain.pool())
            .await
            .unwrap();

        brain.rekey().await.unwrap();

        let (opid, pushed) = sqlx::query_as::<_, (String, i64)>(
            "SELECT opid, pushed FROM memorysync WHERE memoryid = ?",
        )
        .bind(id)
        .fetch_one(brain.pool())
        .await
        .unwrap();
        let (expected, _) = crate::sync::identity(
            "Pools at five",
            "test",
            MemoryScope::Project,
            "github.com/wess/app",
            7,
        );
        assert_eq!(opid, expected);
        assert_eq!(pushed, 0);
    }

    /// A plain folder inside a repository is its own project, not the
    /// repository's.
    #[tokio::test]
    async fn a_path_that_is_not_a_repository_root_stays_where_it_is() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("notes");
        std::fs::create_dir(&folder).unwrap();
        let path = folder.canonicalize().unwrap().display().to_string();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let id = brain
            .storeforeign("Notes", "test", MemoryScope::Project, &path, 1)
            .await
            .unwrap();

        assert_eq!(brain.rekey().await.unwrap(), 0);
        assert_eq!(brain.memory(id).await.unwrap().unwrap().project, path);
    }
}
