use crate::brain::{Memory, MemoryScope, Optimization, Settings, Stats};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Brain {
    pub(super) pool: SqlitePool,
    path: PathBuf,
    _lock: Arc<File>,
}

impl Brain {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        Self::from(crate::database::open(&path).await?, path)
    }

    /// Open only to report on what is stored. See [`crate::database::glance`].
    pub async fn glance(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        Self::from(crate::database::glance(&path).await?, path)
    }

    fn from(opened: crate::database::Opened, path: PathBuf) -> Result<Self> {
        Ok(Self {
            pool: opened.pool,
            path,
            _lock: opened.lock,
        })
    }

    pub async fn rememberscoped(
        &self,
        body: &str,
        source: Option<&str>,
        scope: MemoryScope,
        project: Option<&Path>,
    ) -> Result<i64> {
        let body = body.trim();
        anyhow::ensure!(!body.is_empty(), "memory content cannot be empty");
        let project = scope.project(project)?;

        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_secs() as i64;
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query("INSERT INTO memory(body, source, created) VALUES (?, ?, ?)")
            .bind(body)
            .bind(source.unwrap_or(""))
            .bind(created)
            .execute(&mut *transaction)
            .await?;
        let id = result.last_insert_rowid();
        sqlx::query("INSERT INTO memorymeta(memoryid, scope, project, native) VALUES (?, ?, ?, 1)")
            .bind(id)
            .bind(scope.value())
            .bind(project)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(id)
    }

    pub async fn recallscoped(
        &self,
        query: &str,
        limit: u32,
        budget: Option<Optimization>,
        project: Option<&Path>,
    ) -> Result<(Settings, Vec<Memory>)> {
        let configured = self.settings().await?;
        let settings = Settings::from(configured.optimization.constrained(budget));
        let project = project
            .map(crate::brain::projectroot)
            .transpose()?
            .flatten()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let memories = self
            .searchscoped(query, limit.clamp(1, settings.resultlimit), &project)
            .await?;
        Ok((settings, crate::brain::optimize::recall(memories, settings)))
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<Memory>> {
        let limit = limit.clamp(1, 200) as i64;
        let query = query.trim();
        if query.is_empty() {
            sqlx::query_as::<_, Memory>(
                "SELECT memory.rowid AS id, memory.body, memory.source, meta.scope, meta.project, \
                 CAST(memory.created AS INTEGER) AS created \
                 FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 ORDER BY memory.created DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not read recent memories")
        } else {
            let expression = search_expression(query);
            sqlx::query_as::<_, Memory>(
                "SELECT memory.rowid AS id, memory.body, memory.source, meta.scope, meta.project, \
                 CAST(memory.created AS INTEGER) AS created \
                 FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 WHERE memory MATCH ? ORDER BY rank LIMIT ?",
            )
            .bind(expression)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not search memories")
        }
    }

    async fn searchscoped(&self, query: &str, limit: u32, project: &str) -> Result<Vec<Memory>> {
        let limit = limit.clamp(1, 200) as i64;
        let query = query.trim();
        if query.is_empty() {
            sqlx::query_as::<_, Memory>(
                "SELECT memory.rowid AS id, memory.body, memory.source, meta.scope, meta.project, \
                 CAST(memory.created AS INTEGER) AS created \
                 FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 WHERE meta.scope = 'global' OR (meta.scope = 'project' AND meta.project = ?) \
                 ORDER BY memory.created DESC LIMIT ?",
            )
            .bind(project)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not read scoped memories")
        } else {
            let expression = search_expression(query);
            sqlx::query_as::<_, Memory>(
                "SELECT memory.rowid AS id, memory.body, memory.source, meta.scope, meta.project, \
                 CAST(memory.created AS INTEGER) AS created \
                 FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 WHERE memory MATCH ? \
                 AND (meta.scope = 'global' OR (meta.scope = 'project' AND meta.project = ?)) \
                 ORDER BY rank LIMIT ?",
            )
            .bind(expression)
            .bind(project)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not search scoped memories")
        }
    }

    /// How many memories a recall from `project` could draw on: everything
    /// global, plus everything stored for that project. The same scope rule
    /// `recall` uses, so the number a session reports is the number it can see.
    pub async fn reach(&self, project: Option<&Path>) -> Result<i64> {
        let project = project
            .map(crate::brain::projectroot)
            .transpose()?
            .flatten()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM memorymeta \
             WHERE scope = 'global' OR (scope = 'project' AND project = ?)",
        )
        .bind(project)
        .fetch_one(&self.pool)
        .await
        .context("could not count memories for this project")
    }

    pub async fn memory(&self, id: i64) -> Result<Option<Memory>> {
        sqlx::query_as(
            "SELECT memory.rowid AS id, memory.body, memory.source, meta.scope, meta.project, \
             CAST(memory.created AS INTEGER) AS created \
             FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
             WHERE memory.rowid = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("could not read memory")
    }

    pub async fn updatememory(
        &self,
        id: i64,
        body: &str,
        source: Option<&str>,
    ) -> Result<Option<Memory>> {
        let memory = self.memory(id).await?;
        let Some(memory) = memory else {
            return Ok(None);
        };
        let project = if memory.scope == MemoryScope::Project {
            Some(Path::new(&memory.project))
        } else {
            None
        };
        self.updatememoryscoped(id, body, source, memory.scope, project)
            .await
    }

    pub async fn updatememoryscoped(
        &self,
        id: i64,
        body: &str,
        source: Option<&str>,
        scope: MemoryScope,
        project: Option<&Path>,
    ) -> Result<Option<Memory>> {
        let body = body.trim();
        anyhow::ensure!(!body.is_empty(), "memory content cannot be empty");
        let project = scope.project(project)?;
        let mut transaction = self.pool.begin().await?;
        let result =
            sqlx::query("UPDATE memory SET body = ?, source = COALESCE(?, source) WHERE rowid = ?")
                .bind(body)
                .bind(source.map(str::trim))
                .bind(id)
                .execute(&mut *transaction)
                .await
                .context("could not update memory")?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        sqlx::query("UPDATE memorymeta SET scope = ?, project = ?, native = 1 WHERE memoryid = ?")
            .bind(scope.value())
            .bind(project)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        self.memory(id).await
    }

    pub async fn deletememory(&self, id: i64) -> Result<Option<Memory>> {
        let memory = self.memory(id).await?;
        if memory.is_some() {
            let mut transaction = self.pool.begin().await?;
            sqlx::query("DELETE FROM memoryorigin WHERE memoryid = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("DELETE FROM memorymeta WHERE memoryid = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("DELETE FROM memory WHERE rowid = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await
                .context("could not delete memory")?;
            transaction.commit().await?;
        }
        Ok(memory)
    }

    pub async fn wipememories(&self) -> Result<u64> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM memoryorigin")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM importbatch")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM memorymeta")
            .execute(&mut *transaction)
            .await?;
        let count = sqlx::query("DELETE FROM memory")
            .execute(&mut *transaction)
            .await
            .map(|result| result.rows_affected())
            .context("could not wipe memories")?;
        transaction.commit().await?;
        Ok(count)
    }

    pub async fn settings(&self) -> Result<Settings> {
        crate::brain::settings::read(&self.pool).await
    }

    pub async fn setoptimization(&self, optimization: Optimization) -> Result<()> {
        crate::brain::settings::write(&self.pool, optimization).await
    }

    pub async fn mesh(&self) -> Result<bool> {
        crate::brain::settings::mesh(&self.pool).await
    }

    pub async fn setmesh(&self, enabled: bool) -> Result<()> {
        crate::brain::settings::writemesh(&self.pool, enabled).await
    }

    pub async fn preference(&self, key: &str) -> Result<Option<String>> {
        crate::brain::settings::value(&self.pool, key).await
    }

    pub async fn setpreference(&self, key: &str, value: &str) -> Result<()> {
        crate::brain::settings::writevalue(&self.pool, key, value).await
    }

    pub async fn stats(&self) -> Result<Stats> {
        let entries = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory")
            .fetch_one(&self.pool)
            .await?;
        let bytes = std::fs::metadata(&self.path)
            .map(|item| item.len())
            .unwrap_or(0);
        Ok(Stats { entries, bytes })
    }
}

fn search_expression(query: &str) -> String {
    query
        .split_whitespace()
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_and_finds_memory() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped(
                "Prefer small focused Rust modules",
                Some("preferences"),
                MemoryScope::Global,
                None,
            )
            .await
            .unwrap();

        let matches = brain
            .recallscoped("focused modules", 8, None, None)
            .await
            .unwrap()
            .1;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].source, "preferences");
        assert_eq!(brain.stats().await.unwrap().entries, 1);
    }

    #[tokio::test]
    async fn rejects_empty_memory() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        assert!(
            brain
                .rememberscoped("  ", None, MemoryScope::Global, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn optimization_changes_recall_not_stored_memory() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped(
                "One   durable fact.",
                Some("test"),
                MemoryScope::Global,
                None,
            )
            .await
            .unwrap();
        brain.setoptimization(Optimization::Lean).await.unwrap();
        assert_eq!(
            brain
                .recallscoped("durable", 8, None, None)
                .await
                .unwrap()
                .1[0]
                .body,
            "One durable fact."
        );

        brain.setoptimization(Optimization::Full).await.unwrap();
        assert_eq!(
            brain
                .recallscoped("durable", 8, None, None)
                .await
                .unwrap()
                .1[0]
                .body,
            "One   durable fact."
        );
        brain.setpreference("appearance", "dark").await.unwrap();
        assert_eq!(
            brain.preference("appearance").await.unwrap().as_deref(),
            Some("dark")
        );
    }

    #[tokio::test]
    async fn call_budget_can_shrink_but_not_expand_the_configured_response() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let original = "detail ".repeat(1_000);
        brain
            .rememberscoped(&original, Some("test"), MemoryScope::Global, None)
            .await
            .unwrap();

        brain.setoptimization(Optimization::Full).await.unwrap();
        let (lean, memories) = brain
            .recallscoped("detail", 25, Some(Optimization::Lean), None)
            .await
            .unwrap();
        assert_eq!(lean.optimization, Optimization::Lean);
        assert!(memories[0].body.chars().count() <= 2_800);

        brain.setoptimization(Optimization::Lean).await.unwrap();
        let (stilllean, _) = brain
            .recallscoped("detail", 25, Some(Optimization::Full), None)
            .await
            .unwrap();
        assert_eq!(stilllean.optimization, Optimization::Lean);
        assert_eq!(
            brain.memory(1).await.unwrap().unwrap().body,
            original.trim()
        );
    }

    #[tokio::test]
    async fn updates_deletes_and_wipes_memories() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let first = brain
            .rememberscoped("first", Some("old"), MemoryScope::Global, None)
            .await
            .unwrap();
        brain
            .rememberscoped("second", Some("test"), MemoryScope::Global, None)
            .await
            .unwrap();

        let updated = brain
            .updatememory(first, "updated", Some("new"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.body, "updated");
        assert_eq!(updated.source, "new");
        assert_eq!(brain.search("updated", 100).await.unwrap(), vec![updated]);
        assert!(brain.deletememory(first).await.unwrap().is_some());
        assert_eq!(brain.wipememories().await.unwrap(), 1);
        assert!(brain.search("", 100).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn scoped_recall_combines_global_and_current_project_only() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped("shared global convention", None, MemoryScope::Global, None)
            .await
            .unwrap();
        brain
            .rememberscoped(
                "first project convention",
                None,
                MemoryScope::Project,
                Some(&first),
            )
            .await
            .unwrap();
        brain
            .rememberscoped(
                "second project convention",
                None,
                MemoryScope::Project,
                Some(&second),
            )
            .await
            .unwrap();

        let recalled = brain
            .recallscoped("convention", 8, None, Some(&first))
            .await
            .unwrap()
            .1;
        assert_eq!(recalled.len(), 2);
        assert!(
            recalled
                .iter()
                .any(|memory| memory.body.starts_with("shared"))
        );
        assert!(
            recalled
                .iter()
                .any(|memory| memory.body.starts_with("first"))
        );
        assert!(
            !recalled
                .iter()
                .any(|memory| memory.body.starts_with("second"))
        );
    }
}
