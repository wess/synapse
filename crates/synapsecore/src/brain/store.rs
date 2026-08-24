use crate::brain::{Explanation, Memory, MemoryScope, Optimization, Ranked, Settings, Stats};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// The columns every read of a memory selects, in the order `Memory` declares
/// them. One place, because a column added to `Memory` and forgotten in one of
/// these reads is a row that silently decodes with a default nobody wrote.
const COLUMNS: &str = "memory.rowid AS id, memory.body, memory.source, meta.scope, meta.project, \
                       CAST(memory.created AS INTEGER) AS created, meta.superseded";

/// Restricts a read to memory a session in `project` can see: everything
/// global, plus that project's own, and nothing that has been replaced.
const VISIBLE: &str = "(meta.scope = 'global' OR (meta.scope = 'project' AND meta.project = ?)) AND meta.superseded = 0";

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
        self.rememberreplacing(body, source, scope, project, &[])
            .await
            .map(|(id, _)| id)
    }

    /// Stores a memory that replaces others, and returns it with the ids it
    /// actually replaced.
    ///
    /// One transaction, because the two halves are one decision: a store that
    /// succeeded while the supersession failed leaves both versions live and
    /// contradicting each other, which is the state this exists to end.
    ///
    /// An id that does not resolve is skipped rather than refused. The caller
    /// is usually a model working from a recall it made earlier, and failing
    /// the whole write because one id has since been deleted would lose the
    /// correction to protect the bookkeeping.
    pub async fn rememberreplacing(
        &self,
        body: &str,
        source: Option<&str>,
        scope: MemoryScope,
        project: Option<&Path>,
        supersedes: &[i64],
    ) -> Result<(i64, Vec<i64>)> {
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
        sqlx::query(
            "INSERT INTO memorymeta(memoryid, scope, project, native, created) \
             VALUES (?, ?, ?, 1, ?)",
        )
        .bind(id)
        .bind(scope.value())
        .bind(project)
        .bind(created)
        .execute(&mut *transaction)
        .await?;
        let mut replaced = Vec::new();
        for old in supersedes {
            let done = sqlx::query("UPDATE memorymeta SET superseded = ? WHERE memoryid = ?")
                .bind(id)
                .bind(old)
                .execute(&mut *transaction)
                .await
                .with_context(|| format!("could not supersede memory #{old}"))?;
            if done.rows_affected() > 0 && *old != id {
                replaced.push(*old);
            }
        }
        transaction.commit().await?;
        Ok((id, replaced))
    }

    /// Marks `old` as replaced by `new`, without deleting either.
    ///
    /// A memory cannot supersede itself: that would hide it behind a pointer to
    /// itself and no command would bring it back.
    pub async fn supersede(&self, old: i64, new: i64) -> Result<bool> {
        anyhow::ensure!(old != new, "a memory cannot supersede itself");
        anyhow::ensure!(
            self.memory(new).await?.is_some(),
            "memory #{new} does not exist"
        );
        let done = sqlx::query("UPDATE memorymeta SET superseded = ? WHERE memoryid = ?")
            .bind(new)
            .bind(old)
            .execute(&self.pool)
            .await
            .with_context(|| format!("could not supersede memory #{old}"))?;
        Ok(done.rows_affected() > 0)
    }

    /// Puts a superseded memory back in circulation.
    pub async fn unsupersede(&self, id: i64) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE memorymeta SET superseded = 0 WHERE memoryid = ? AND superseded != 0",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .with_context(|| format!("could not restore memory #{id}"))?;
        Ok(done.rows_affected() > 0)
    }

    /// Stores a memory exactly as another machine recorded it.
    ///
    /// `rememberscoped` is for a memory being made here: it stamps the time and
    /// derives a project identity from a path on this disk. Neither is right
    /// for one arriving from the log, which already carries the identity the
    /// machine that made it derived and the moment it was made. Keeping those
    /// is what makes a memory the same memory on both machines rather than a
    /// copy dated whenever it happened to arrive.
    pub async fn storeforeign(
        &self,
        body: &str,
        source: &str,
        scope: MemoryScope,
        project: &str,
        created: i64,
    ) -> Result<i64> {
        let body = body.trim();
        anyhow::ensure!(!body.is_empty(), "memory content cannot be empty");
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query("INSERT INTO memory(body, source, created) VALUES (?, ?, ?)")
            .bind(body)
            .bind(source)
            .bind(created)
            .execute(&mut *transaction)
            .await?;
        let id = result.last_insert_rowid();
        // `native` is 0: this memory was not made on this machine, which is
        // the same distinction an import already draws.
        sqlx::query(
            "INSERT INTO memorymeta(memoryid, scope, project, native, created) \
             VALUES (?, ?, ?, 0, ?)",
        )
        .bind(id)
        .bind(scope.value())
        .bind(project)
        .bind(created)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(id)
    }

    /// The pool, for the sync module's own bookkeeping table.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
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

    /// Every memory matching `query`, superseded ones included.
    ///
    /// The scoped read behind recall hides what has been replaced; this one
    /// does not, because it answers a person browsing their own store and a
    /// memory somebody has to be able to find in order to restore it.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<Memory>> {
        let limit = limit.clamp(1, 200) as i64;
        if let Some(expression) = search_expression(query.trim()) {
            sqlx::query_as::<_, Memory>(&format!(
                "SELECT {COLUMNS} \
                 FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 WHERE memory MATCH ? ORDER BY rank LIMIT ?"
            ))
            .bind(expression)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not search memories")
        } else {
            sqlx::query_as::<_, Memory>(&format!(
                "SELECT {COLUMNS} \
                 FROM memorymeta meta JOIN memory ON memory.rowid = meta.memoryid \
                 WHERE meta.memoryid IN (\
                 SELECT memoryid FROM memorymeta ORDER BY created DESC, memoryid DESC LIMIT ?) \
                 ORDER BY meta.created DESC, meta.memoryid DESC"
            ))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not read recent memories")
        }
    }

    /// Memories containing `pattern` exactly, most recent first.
    ///
    /// `search` drops function words and ranks what is left, which is right for
    /// a question and wrong for an exact string — an identifier, a flag, a
    /// path, a word this store treats as noise. Nothing here is tokenized,
    /// ranked, or dropped: the characters given have to appear, in that order.
    /// Matching is case-insensitive for ASCII, which is SQLite's own `LIKE`.
    pub async fn grep(&self, pattern: &str, limit: u32) -> Result<Vec<Memory>> {
        let pattern = pattern.trim();
        anyhow::ensure!(!pattern.is_empty(), "there is nothing to search for");
        let limit = limit.clamp(1, 200) as i64;
        sqlx::query_as::<_, Memory>(&format!(
            "SELECT {COLUMNS} \
             FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
             WHERE memory.body LIKE '%' || ? || '%' ESCAPE '\\' \
             ORDER BY meta.created DESC, meta.memoryid DESC LIMIT ?"
        ))
        .bind(escaped(pattern))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("could not search memory text")
    }

    /// What a query became and what it matched, with the score behind each hit.
    pub async fn explain(&self, query: &str, limit: u32) -> Result<Explanation> {
        let query = query.trim();
        let limit = limit.clamp(1, 200) as i64;
        let (kept, dropped) = terms(query);
        let expression = search_expression(query);
        let matches = match &expression {
            Some(expression) => sqlx::query_as::<_, Scored>(&format!(
                "SELECT {COLUMNS}, rank AS score \
                 FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 WHERE memory MATCH ? ORDER BY rank LIMIT ?"
            ))
            .bind(expression)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not search memories")?
            .into_iter()
            .map(|row| Ranked {
                memory: row.memory,
                rank: Some(row.score),
            })
            .collect(),
            None => self
                .search(query, limit as u32)
                .await?
                .into_iter()
                .map(|memory| Ranked { memory, rank: None })
                .collect(),
        };
        Ok(Explanation {
            query: query.to_owned(),
            expression,
            kept,
            dropped,
            matches,
        })
    }

    async fn searchscoped(&self, query: &str, limit: u32, project: &str) -> Result<Vec<Memory>> {
        let limit = limit.clamp(1, 200) as i64;
        // A query of nothing but function words asks for nothing in particular,
        // so it is answered the way an empty one is — with what is most recent,
        // rather than with whatever happened to contain the word `the`.
        let expression = search_expression(query.trim());
        if let Some(expression) = expression {
            sqlx::query_as::<_, Memory>(&format!(
                "SELECT {COLUMNS} \
                 FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 WHERE memory MATCH ? AND {VISIBLE} \
                 ORDER BY rank LIMIT ?"
            ))
            .bind(expression)
            .bind(project)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not search scoped memories")
        } else {
            // Choose the newest ids from the index first, then fetch only
            // those. Filtering and ordering in the outer join instead lets the
            // planner drive from the full-text table, which means reading every
            // memory and sorting all of them to answer for the newest handful.
            sqlx::query_as::<_, Memory>(&format!(
                "SELECT {COLUMNS} \
                 FROM memorymeta meta JOIN memory ON memory.rowid = meta.memoryid \
                 WHERE meta.memoryid IN (\
                 SELECT memoryid FROM memorymeta \
                 WHERE (scope = 'global' OR (scope = 'project' AND project = ?)) AND superseded = 0 \
                 ORDER BY created DESC, memoryid DESC LIMIT ?) \
                 ORDER BY meta.created DESC, meta.memoryid DESC"
            ))
            .bind(project)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .context("could not read scoped memories")
        }
    }

    /// How many memories a recall from `project` could draw on: everything
    /// global, plus everything stored for that project, minus what has been
    /// replaced. The same rule `recall` uses, so the number a session reports
    /// is the number it can see.
    pub async fn reach(&self, project: Option<&Path>) -> Result<i64> {
        let project = project
            .map(crate::brain::projectroot)
            .transpose()?
            .flatten()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM memorymeta \
             WHERE (scope = 'global' OR (scope = 'project' AND project = ?)) AND superseded = 0",
        )
        .bind(project)
        .fetch_one(&self.pool)
        .await
        .context("could not count memories for this project")
    }

    pub async fn memory(&self, id: i64) -> Result<Option<Memory>> {
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} \
             FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
             WHERE memory.rowid = ?"
        ))
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
            // Anything this memory replaced comes back. Leaving those rows
            // pointing at an id that no longer resolves would hide them for
            // good, with nothing left on the machine to name what replaced
            // them — deleting a correction has to undo the correction.
            sqlx::query("UPDATE memorymeta SET superseded = 0 WHERE superseded = ?")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
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

    pub async fn maxworkers(&self) -> Result<usize> {
        crate::brain::settings::maxworkers(&self.pool).await
    }

    pub async fn setmaxworkers(&self, workers: usize) -> Result<()> {
        crate::brain::settings::writemaxworkers(&self.pool, workers).await
    }

    pub async fn learn(&self) -> Result<bool> {
        crate::brain::settings::learn(&self.pool).await
    }

    pub async fn setlearn(&self, enabled: bool) -> Result<()> {
        crate::brain::settings::writelearn(&self.pool, enabled).await
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

/// Words that appear in almost every memory, so matching one says nothing about
/// whether a memory is the right one.
///
/// Only function words are here. Anything a developer might mean — `not`, `no`,
/// `use`, `never`, `always` — carries signal in a stored preference and stays.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "any", "are", "as", "at", "be", "been", "but", "by", "can", "could", "did",
    "do", "does", "for", "from", "had", "has", "have", "how", "i", "if", "in", "into", "is", "it",
    "its", "me", "my", "of", "on", "or", "our", "should", "so", "than", "that", "the", "their",
    "them", "then", "there", "these", "they", "this", "to", "was", "we", "were", "what", "when",
    "where", "which", "who", "why", "will", "with", "would", "you", "your",
];

/// The FTS expression for `query`, or nothing when the query carries no term
/// worth searching for.
///
/// Every term used to be ORed together, stopwords included, which was wrong and
/// slow for the same reason: `where are credentials stored` became a search for
/// `where OR are OR credentials OR stored`, so a memory that merely contained
/// *are* was ranked and returned as the answer. An agent acts on that, which
/// makes a confident wrong answer worse than none. It also meant asking SQLite
/// to rank every memory containing a common word — at 200k memories that is a
/// 437ms query where the same search without the noise takes 0.3ms.
fn search_expression(query: &str) -> Option<String> {
    let (kept, _) = terms(query);
    if kept.is_empty() {
        return None;
    }
    Some(
        kept.into_iter()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// A query split into the terms that were searched for and the ones dropped for
/// matching nearly everything. `search_expression` is built from the first half,
/// so `--explain` and recall can never disagree about what happened.
fn terms(query: &str) -> (Vec<String>, Vec<String>) {
    let (dropped, kept): (Vec<&str>, Vec<&str>) =
        query.split_whitespace().partition(|term| stopword(term));
    (
        kept.into_iter().map(ToOwned::to_owned).collect(),
        dropped.into_iter().map(ToOwned::to_owned).collect(),
    )
}

/// A literal pattern, safe to hand to `LIKE`. Without this a memory containing
/// `%` is a wildcard and `synapse memory grep %` returns the whole store.
fn escaped(pattern: &str) -> String {
    pattern
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// One search hit with the score that put it there. Only `--explain` reads the
/// score, so it stays out of `Memory` itself.
#[derive(sqlx::FromRow)]
struct Scored {
    #[sqlx(flatten)]
    memory: Memory,
    score: f64,
}

/// Whether a term is one of the words that match nearly everything, ignoring
/// the punctuation a spoken-sounding query carries.
fn stopword(term: &str) -> bool {
    let bare: String = term
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    bare.is_empty() || STOPWORDS.contains(&bare.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MemoryScope::Global;

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

    /// The bug this replaced: `where are credentials stored` matched a memory
    /// on the word *are* and returned it as the answer. An agent acts on a
    /// confident wrong answer, so nothing is the better result.
    #[tokio::test]
    async fn a_query_never_matches_on_its_function_words_alone() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped(
                "Postgres connections are pooled at five",
                None,
                MemoryScope::Global,
                None,
            )
            .await
            .unwrap();

        let matches = brain
            .recallscoped("where are credentials stored", 8, None, None)
            .await
            .unwrap()
            .1;

        assert!(
            matches.is_empty(),
            "matched on a function word: {:?}",
            matches.first().map(|memory| &memory.body)
        );
    }

    /// A query with nothing but function words asks for nothing in particular,
    /// so it is answered the way an empty query is.
    #[tokio::test]
    async fn a_query_of_only_function_words_falls_back_to_what_is_recent() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped(
                "Deploys run from the deploy branch",
                None,
                MemoryScope::Global,
                None,
            )
            .await
            .unwrap();

        let matches = brain
            .recallscoped("how do i", 8, None, None)
            .await
            .unwrap()
            .1;

        assert_eq!(
            matches.len(),
            1,
            "should read as an empty query, not a miss"
        );
    }

    #[test]
    fn the_expression_keeps_words_a_developer_might_have_meant() {
        // Negation and emphasis carry the whole meaning of a stored preference.
        for kept in ["not", "no", "never", "always", "use", "run", "all"] {
            assert!(
                search_expression(kept).is_some(),
                "`{kept}` carries meaning and must survive"
            );
        }
        assert_eq!(search_expression("the and of").as_deref(), None);
        assert_eq!(search_expression("").as_deref(), None);
        assert_eq!(
            search_expression("Where, are THE credentials?").as_deref(),
            Some("\"credentials?\""),
            "punctuation and case must not hide a stopword or drop a real term"
        );
    }

    /// Recent means most recently *stored*, which an import can set to a date
    /// long before the row existed — so insertion order is not the answer.
    #[tokio::test]
    async fn the_recent_list_orders_by_when_a_memory_was_made_not_when_it_was_written() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        for body in ["first", "second"] {
            brain
                .rememberscoped(body, None, MemoryScope::Global, None)
                .await
                .unwrap();
        }
        // Stand in for an import: written last, dated long ago.
        let old = brain
            .rememberscoped("imported", None, MemoryScope::Global, None)
            .await
            .unwrap();
        for table in ["memory", "memorymeta"] {
            let column = if table == "memory" {
                "rowid"
            } else {
                "memoryid"
            };
            sqlx::query(&format!(
                "UPDATE {table} SET created = 1 WHERE {column} = ?"
            ))
            .bind(old)
            .execute(&brain.pool)
            .await
            .unwrap();
        }

        let recent = brain.search("", 10).await.unwrap();

        assert_eq!(recent.len(), 3);
        assert_eq!(
            recent.last().unwrap().body,
            "imported",
            "the oldest memory belongs last however recently it was written"
        );
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

    /// The bug this closes: a correction was a second memory contradicting the
    /// first, both were recalled, and the ranking decided which one an agent
    /// acted on.
    #[tokio::test]
    async fn a_correction_stops_the_memory_it_replaced_from_being_recalled() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let old = brain
            .rememberscoped("Deploys run from the main branch", None, Global, None)
            .await
            .unwrap();

        let (new, replaced) = brain
            .rememberreplacing(
                "Deploys run from the release branch",
                None,
                Global,
                None,
                &[old],
            )
            .await
            .unwrap();

        assert_eq!(replaced, vec![old]);
        let recalled = brain
            .recallscoped("deploys branch", 8, None, None)
            .await
            .unwrap()
            .1;
        assert_eq!(recalled.len(), 1, "got {recalled:?}");
        assert_eq!(recalled[0].id, new);
        assert_eq!(
            brain.reach(None).await.unwrap(),
            1,
            "the count follows recall"
        );

        // Hidden, not gone.
        let superseded = brain.memory(old).await.unwrap().unwrap();
        assert_eq!(superseded.superseded, new);
        assert_eq!(superseded.body, "Deploys run from the main branch");
        assert_eq!(brain.search("", 100).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_supersession_can_be_undone_and_a_memory_cannot_replace_itself() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let old = brain
            .rememberscoped("first", None, Global, None)
            .await
            .unwrap();
        let new = brain
            .rememberscoped("second", None, Global, None)
            .await
            .unwrap();

        assert!(brain.supersede(old, new).await.unwrap());
        assert!(!brain.supersede(404, new).await.unwrap(), "no such memory");
        assert!(
            brain.supersede(old, 404).await.is_err(),
            "no such replacement"
        );
        assert!(brain.supersede(old, old).await.is_err(), "a cycle of one");

        assert!(brain.unsupersede(old).await.unwrap());
        assert!(
            !brain.unsupersede(old).await.unwrap(),
            "restoring a live memory changes nothing"
        );
        assert_eq!(
            brain
                .recallscoped("first", 8, None, None)
                .await
                .unwrap()
                .1
                .len(),
            1
        );
    }

    /// Deleting the replacement has to undo the replacement. Otherwise the old
    /// memory points at an id that no longer resolves, nothing on the machine
    /// can name what replaced it, and it is hidden for good.
    #[tokio::test]
    async fn deleting_a_correction_brings_back_what_it_corrected() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let old = brain
            .rememberscoped("Postgres pools at five", None, Global, None)
            .await
            .unwrap();
        let (new, _) = brain
            .rememberreplacing("Postgres pools at twenty", None, Global, None, &[old])
            .await
            .unwrap();

        brain.deletememory(new).await.unwrap();

        let recalled = brain
            .recallscoped("postgres pools", 8, None, None)
            .await
            .unwrap()
            .1;
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].id, old);
        assert_eq!(recalled[0].superseded, 0);
    }

    /// The ranked search drops function words, so the one thing it cannot do is
    /// find an exact string. This is the escape hatch, and it must not treat
    /// the pattern as a wildcard.
    #[tokio::test]
    async fn grep_matches_characters_that_the_ranked_search_throws_away() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped(
                "Pass --no-verify only on the release commit",
                None,
                Global,
                None,
            )
            .await
            .unwrap();
        brain
            .rememberscoped("Coverage sits at 80% and no lower", None, Global, None)
            .await
            .unwrap();

        assert_eq!(brain.grep("--no-verify", 10).await.unwrap().len(), 1);
        assert_eq!(
            brain.grep("NO-VERIFY", 10).await.unwrap().len(),
            1,
            "case-insensitive"
        );
        assert_eq!(brain.grep("80%", 10).await.unwrap().len(), 1);
        assert!(
            brain.grep("%", 10).await.unwrap().len() == 1,
            "a wildcard character is a character, not a wildcard"
        );
        assert!(brain.grep("  ", 10).await.is_err());
    }

    /// Recall is otherwise opaque about which words it searched for, which is
    /// the whole of "why did my memory not come back".
    #[tokio::test]
    async fn explain_says_which_words_were_searched_for_and_which_were_dropped() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped("Credentials live in Keychain", None, Global, None)
            .await
            .unwrap();

        let explained = brain
            .explain("where are the credentials", 10)
            .await
            .unwrap();

        assert_eq!(explained.mode(), "search");
        assert_eq!(explained.kept, vec!["credentials"]);
        assert_eq!(explained.dropped, vec!["where", "are", "the"]);
        assert_eq!(explained.expression.as_deref(), Some("\"credentials\""));
        assert_eq!(explained.matches.len(), 1);
        assert!(
            explained.matches[0].rank.is_some(),
            "a hit carries its score"
        );

        let recent = brain.explain("how do i", 10).await.unwrap();
        assert_eq!(recent.mode(), "recent");
        assert!(recent.expression.is_none());
        assert!(
            recent.matches[0].rank.is_none(),
            "the recent list is not scored"
        );
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
