use crate::brain::{Brain, MemoryScope, Optimization, Settings};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

const PAGEBYTES: usize = 6_000;
const SOURCEBYTES: usize = 240;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadRequest {
    /// Memory id returned by recall.
    pub id: i64,
    /// UTF-8 byte offset from the previous page's next field. Defaults to zero.
    pub offset: Option<u32>,
    /// Optional smaller response budget; never raises the configured ceiling.
    pub budget: Option<Optimization>,
    /// Absolute project root, using the same scope as recall.
    pub project: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ReadResponse {
    pub optimization: Optimization,
    /// Null when the id is missing, superseded, or outside this project's scope.
    pub memory: Option<MemoryPage>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryPage {
    pub id: i64,
    /// Exact stored text for this page, without compaction or an added ellipsis.
    pub body: String,
    pub source: String,
    /// The source label exceeded its separate 240-byte allowance.
    pub sourceabridged: bool,
    pub scope: MemoryScope,
    pub project: String,
    pub created: i64,
    /// UTF-8 byte offsets; next is null at the end of the memory.
    pub offset: u32,
    pub next: Option<u32>,
    pub total: u32,
}

#[derive(sqlx::FromRow)]
struct PageRow {
    body: Vec<u8>,
    source: Vec<u8>,
    sourcebytes: i64,
    scope: MemoryScope,
    project: String,
    created: i64,
    total: u32,
}

impl Brain {
    pub async fn readscoped(
        &self,
        id: i64,
        offset: u32,
        budget: Option<Optimization>,
        project: Option<&Path>,
    ) -> Result<ReadResponse> {
        anyhow::ensure!(id > 0, "memory id must be positive");
        let configured = self.settings().await?;
        let optimization = configured.optimization.constrained(budget);
        let bytes = Settings::from(optimization)
            .characterbudget
            .unwrap_or(PAGEBYTES)
            .min(PAGEBYTES);
        let project = project
            .map(crate::brain::projectroot)
            .transpose()?
            .flatten()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        // slice in SQLite so a small read never loads the entire body into Rust.
        // blobs preserve embedded NULs; SQLite's text length stops at the first one.
        let row = sqlx::query_as::<_, PageRow>(
            "SELECT substr(CAST(memory.body AS BLOB), ?3 + 1, ?4) AS body, \
             substr(CAST(memory.source AS BLOB), 1, ?5) AS source, \
             length(CAST(memory.source AS BLOB)) AS sourcebytes, \
             meta.scope, meta.project, CAST(memory.created AS INTEGER) AS created, \
             length(CAST(memory.body AS BLOB)) AS total \
             FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
             WHERE memory.rowid = ?1 AND meta.superseded = 0 AND \
             (meta.scope = 'global' OR (meta.scope = 'project' AND meta.project = ?2))",
        )
        .bind(id)
        .bind(project)
        .bind(i64::from(offset))
        .bind(bytes as i64)
        .bind(SOURCEBYTES as i64)
        .fetch_optional(&self.pool)
        .await
        .context("could not read memory page")?;
        let memory = row
            .map(|row| -> Result<MemoryPage> {
                anyhow::ensure!(offset <= row.total, "offset exceeds memory length");
                let body = text(row.body).context("offset must be a UTF-8 character boundary")?;
                let source = text(row.source)?;
                let end = offset + body.len() as u32;
                Ok(MemoryPage {
                    id,
                    body,
                    sourceabridged: (source.len() as i64) < row.sourcebytes,
                    source,
                    scope: row.scope,
                    project: row.project,
                    created: row.created,
                    offset,
                    next: (end < row.total).then_some(end),
                    total: row.total,
                })
            })
            .transpose()?;
        Ok(ReadResponse {
            optimization,
            memory,
        })
    }
}

fn text(mut bytes: Vec<u8>) -> Result<String> {
    if let Err(error) = std::str::from_utf8(&bytes) {
        anyhow::ensure!(error.error_len().is_none(), "invalid UTF-8 page boundary");
        bytes.truncate(error.valid_up_to());
    }
    Ok(String::from_utf8(bytes)?)
}
