//! Repository evidence attached to implementation-derived memory. A changed source does not
//! erase a claim; it marks the claim for verification until a new evidenced memory supersedes it.

use crate::brain::{Brain, Memory};
use crate::knowledge::{
    check::{Check, Checked},
    io::Folder,
    paths,
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Serialize;
use sqlx::{FromRow, QueryBuilder, Sqlite};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

const SOURCECOUNT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceStatus {
    Current,
    Needsverification,
    Unchecked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MemoryEvidence {
    pub memory: i64,
    pub status: EvidenceStatus,
    pub sources: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unchecked: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Captured {
    pub path: String,
    pub digest: String,
}

#[derive(FromRow)]
struct Stored {
    memoryid: i64,
    path: String,
    digest: String,
}

pub(crate) fn capture(project: &Path, sources: &[String]) -> Result<Vec<Captured>> {
    ensure!(
        !sources.is_empty() && sources.len() <= SOURCECOUNT,
        "supply 1-{SOURCECOUNT} repository source files"
    );
    let root = paths::root(project)?;
    let root = Folder::open(&root)?;
    let mut check = Check::new();
    let mut captured = BTreeMap::new();
    for path in sources {
        paths::validsource(path)?;
        ensure!(
            crate::brain::redact(path).count == 0,
            "source path contains credentials"
        );
        let Checked::Hash(digest) = check.source(&root, path) else {
            anyhow::bail!(
                "source unreadable, changed during inspection, or source scan exceeds 16 MiB; reduce the evidence set"
            );
        };
        captured.insert(path.clone(), digest);
    }
    Ok(captured
        .into_iter()
        .map(|(path, digest)| Captured { path, digest })
        .collect())
}

impl Brain {
    /// Check evidence for the supplied memories against this checkout. Results exist only for
    /// project memories that were stored with evidence and belong to the checkout being inspected.
    pub async fn evidence(
        &self,
        memories: &[Memory],
        project: Option<&Path>,
    ) -> Result<Vec<MemoryEvidence>> {
        // Session/status reporting opens the store read-only and may be the first process an older
        // installation launches after upgrade. No pre-v23 memory can have evidence, so do not make
        // the connection look unavailable while the ordinary writer has not migrated it yet.
        if crate::database::version(&self.pool).await? < 23 {
            return Ok(Vec::new());
        }
        let Some(project) = project else {
            return Ok(Vec::new());
        };
        let Some(key) = crate::brain::projectkey(project)? else {
            return Ok(Vec::new());
        };
        let root = paths::root(project)?;
        let root = Folder::open(&root)?;
        let ids: HashSet<i64> = memories
            .iter()
            .filter(|memory| memory.project == key)
            .map(|memory| memory.id)
            .collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut stored = Vec::new();
        let ids = ids.into_iter().collect::<Vec<_>>();
        for chunk in ids.chunks(400) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT memoryid,path,digest FROM memoryevidence WHERE memoryid IN (",
            );
            let mut separated = query.separated(",");
            for id in chunk {
                separated.push_bind(id);
            }
            separated.push_unseparated(") ORDER BY memoryid,path");
            stored.extend(
                query
                    .build_query_as::<Stored>()
                    .fetch_all(&self.pool)
                    .await
                    .context("could not read memory evidence")?,
            );
        }

        let mut check = Check::new();
        let mut grouped: BTreeMap<i64, Vec<Stored>> = BTreeMap::new();
        for source in stored {
            grouped.entry(source.memoryid).or_default().push(source);
        }
        let mut reports = Vec::new();
        for (memory, sources) in grouped {
            let mut all = Vec::new();
            let mut changed = Vec::new();
            let mut missing = Vec::new();
            let mut unchecked = Vec::new();
            for source in sources {
                all.push(source.path.clone());
                match check.source(&root, &source.path) {
                    Checked::Hash(digest) if digest == source.digest => {}
                    Checked::Hash(_) => changed.push(source.path),
                    Checked::Missing => missing.push(source.path),
                    Checked::Unchecked => unchecked.push(source.path),
                }
            }
            let status = if !changed.is_empty() || !missing.is_empty() {
                EvidenceStatus::Needsverification
            } else if !unchecked.is_empty() {
                EvidenceStatus::Unchecked
            } else {
                EvidenceStatus::Current
            };
            reports.push(MemoryEvidence {
                memory,
                status,
                sources: all,
                changed,
                missing,
                unchecked,
            });
        }
        Ok(reports)
    }

    pub(crate) async fn evidenceone(
        &self,
        memory: i64,
        project: Option<&Path>,
    ) -> Result<Option<MemoryEvidence>> {
        let Some(memory) = self.memory(memory).await? else {
            return Ok(None);
        };
        Ok(self.evidence(&[memory], project).await?.into_iter().next())
    }
}
