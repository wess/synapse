use crate::brain::MemoryScope;
use crate::imports::{ImportCandidate, ImportProvider};
use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

pub async fn scan(home: &Path) -> Result<Vec<ImportCandidate>> {
    let root = std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let Some(memories) = newest(&root, "memories_", ".sqlite")? else {
        return Ok(Vec::new());
    };
    let pool = readonly(&memories).await?;
    requirecolumns(
        &pool,
        "stage1_outputs",
        &[
            "thread_id",
            "source_updated_at",
            "raw_memory",
            "rollout_slug",
        ],
    )
    .await?;
    let projects = match newest(&root, "state_", ".sqlite")? {
        Some(state) => threadprojects(&state).await.unwrap_or_default(),
        None => HashMap::new(),
    };
    let rows = sqlx::query(
        "SELECT thread_id, source_updated_at, raw_memory, rollout_slug \
         FROM stage1_outputs WHERE trim(raw_memory) <> '' ORDER BY source_updated_at",
    )
    .fetch_all(&pool)
    .await
    .context("could not read the Codex memory store")?;
    pool.close().await;
    let mut candidates = Vec::new();
    for row in rows {
        let thread: String = row.try_get("thread_id")?;
        let raw: String = row.try_get("raw_memory")?;
        let slug: Option<String> = row.try_get("rollout_slug")?;
        let mut created: i64 = row.try_get("source_updated_at")?;
        if created > 10_000_000_000 {
            created /= 1_000;
        }
        let project = projects
            .get(&thread)
            .and_then(|path| crate::brain::projectroot(path).ok().flatten());
        let warning = crate::imports::warning(&raw).or_else(|| {
            project
                .is_none()
                .then(|| "could not map this memory to a project root".to_owned())
        });
        let label = slug
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("thread {}", thread.chars().take(8).collect::<String>()));
        for (index, body) in split(&raw).into_iter().enumerate() {
            candidates.push(ImportCandidate {
                provider: ImportProvider::Codex,
                externalid: format!("{thread}#{index}"),
                body,
                source: format!("Codex · {label}"),
                scope: project
                    .as_ref()
                    .map(|_| MemoryScope::Project)
                    .unwrap_or(MemoryScope::Global),
                project: project
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                path: memories.clone(),
                created,
                warning: warning.clone(),
            });
        }
    }
    Ok(candidates)
}

async fn readonly(path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite://synapseimport")?
        .filename(path)
        .read_only(true)
        .busy_timeout(Duration::from_secs(3));
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("could not open {}", path.display()))
}

async fn requirecolumns(pool: &SqlitePool, table: &str, required: &[&str]) -> Result<()> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let columns = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .collect::<Vec<_>>();
    anyhow::ensure!(!columns.is_empty(), "unsupported Codex memory database");
    for required in required {
        anyhow::ensure!(
            columns.iter().any(|column| column == required),
            "unsupported Codex memory database: missing {required}"
        );
    }
    Ok(())
}

async fn threadprojects(path: &Path) -> Result<HashMap<String, PathBuf>> {
    let pool = readonly(path).await?;
    requirecolumns(&pool, "threads", &["id", "cwd"]).await?;
    let rows = sqlx::query("SELECT id, cwd FROM threads WHERE trim(cwd) <> ''")
        .fetch_all(&pool)
        .await?;
    pool.close().await;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("id")?,
                PathBuf::from(row.try_get::<String, _>("cwd")?),
            ))
        })
        .collect()
}

fn newest(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    if !root.is_dir() {
        return Ok(None);
    }
    let mut matches = fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    matches.sort();
    Ok(matches.pop())
}

fn split(body: &str) -> Vec<String> {
    const CHUNK: usize = 6_000;
    let body = body.trim();
    if body.chars().count() <= CHUNK {
        return vec![body.to_owned()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in body.split("\n\n") {
        if current.chars().count() + paragraph.chars().count() + 2 > CHUNK && !current.is_empty() {
            chunks.push(current.trim().to_owned());
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_owned());
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_supported_memory_rows_with_project_scope() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".codex");
        let project = home.path().join("project");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join(".git")).unwrap();
        let memorypath = root.join("memories_1.sqlite");
        let statepath = root.join("state_1.sqlite");
        let memory = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite://test")
                    .unwrap()
                    .filename(&memorypath)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE stage1_outputs(thread_id TEXT, source_updated_at INTEGER, raw_memory TEXT, rollout_slug TEXT)")
            .execute(&memory)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO stage1_outputs VALUES ('thread1', 10, 'Use the shared choice.', 'choice')",
        )
        .execute(&memory)
        .await
        .unwrap();
        memory.close().await;
        let state = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite://test")
                    .unwrap()
                    .filename(&statepath)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE threads(id TEXT, cwd TEXT)")
            .execute(&state)
            .await
            .unwrap();
        sqlx::query("INSERT INTO threads VALUES ('thread1', ?)")
            .bind(project.display().to_string())
            .execute(&state)
            .await
            .unwrap();
        state.close().await;

        let candidates = scan(home.path()).await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].scope, MemoryScope::Project);
        assert!(candidates[0].warning.is_none());
    }
}
