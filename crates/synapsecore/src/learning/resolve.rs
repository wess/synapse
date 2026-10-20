use super::{clean, digest, now, project, retention};
use crate::brain::drift::{Checkout, Concern, Finding, scan};
use crate::brain::{Brain, Memory, MemoryScope};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Disposition {
    Corrected,
    Contextual,
    Falsepositive,
}

impl Disposition {
    pub fn value(self) -> &'static str {
        match self {
            Self::Corrected => "corrected",
            Self::Contextual => "contextual",
            Self::Falsepositive => "falsepositive",
        }
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema, FromRow)]
pub struct Resolution {
    pub id: i64,
    pub project: String,
    pub fingerprint: String,
    pub disposition: Disposition,
    pub note: String,
    pub created: i64,
}

fn memoryhash(memory: &Memory) -> Result<String> {
    digest(&(
        memory.id,
        &memory.body,
        &memory.source,
        memory.scope,
        &memory.project,
        memory.superseded,
    ))
}

fn fingerprint(
    finding: &Finding,
    hashes: &HashMap<i64, String>,
    checkout: Option<&Path>,
) -> Result<String> {
    let mut ids = finding.ids.clone();
    ids.sort_unstable();
    let bodies: Vec<_> = ids
        .iter()
        .map(|id| {
            Ok((
                *id,
                hashes.get(id).context("finding names an unknown memory")?,
            ))
        })
        .collect::<Result<_>>()?;
    let mut missing = finding.missing.clone();
    missing.sort();
    let checkout = (finding.concern == Concern::Stale)
        .then_some(checkout)
        .flatten();
    digest(&(finding.concern, bodies, missing, checkout))
}

impl Brain {
    pub async fn resolvefinding(
        &self,
        folder: &Path,
        finding: &Finding,
        disposition: Disposition,
        note: &str,
    ) -> Result<Resolution> {
        let project = project(Some(folder))?;
        anyhow::ensure!(
            !finding.ids.is_empty() && finding.ids.len() <= 2,
            "invalid finding ids"
        );
        let mut memories = Vec::new();
        for id in &finding.ids {
            let memory = self.memory(*id).await?.context("memory does not exist")?;
            anyhow::ensure!(
                memory.scope == MemoryScope::Global || memory.project == project,
                "memory is outside this project"
            );
            memories.push(memory);
        }
        anyhow::ensure!(
            scan(&memories, &Checkout::at(Some(folder)))
                .iter()
                .any(|actual| {
                    actual.concern == finding.concern
                        && actual.ids == finding.ids
                        && actual.missing == finding.missing
                }),
            "finding changed; check drift again"
        );
        if disposition == Disposition::Corrected {
            anyhow::ensure!(
                memories.iter().any(|memory| memory.superseded != 0),
                "a corrected finding needs a superseded memory; a successful retry is insufficient"
            );
        }
        let note = clean(note, 1_000, "resolution note")?;
        anyhow::ensure!(!note.is_empty(), "a resolution needs a reason");
        let hashes = memories
            .iter()
            .map(|memory| Ok((memory.id, memoryhash(memory)?)))
            .collect::<Result<_>>()?;
        let checkout = crate::brain::projectroot(folder)?;
        let fingerprint = fingerprint(finding, &hashes, checkout.as_deref())?;
        let now = now()?;
        let _writer = self.writing().await;
        let mut transaction = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO driftresolution(project, fingerprint, memoryid, disposition, note, created) \
             VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(project, fingerprint) DO UPDATE SET \
             disposition = excluded.disposition, note = excluded.note, created = excluded.created \
             RETURNING id",
        ).bind(&project).bind(fingerprint).bind(finding.ids[0]).bind(disposition.value()).bind(note).bind(now)
            .fetch_one(&mut *transaction).await?;
        retention::prune(&mut transaction, now).await?;
        transaction.commit().await?;
        Ok(sqlx::query_as("SELECT * FROM driftresolution WHERE id = ?")
            .bind(id)
            .fetch_one(self.pool())
            .await?)
    }

    pub async fn driftresolutions(&self, folder: &Path) -> Result<Vec<Resolution>> {
        Ok(sqlx::query_as(
            "SELECT * FROM driftresolution WHERE project = ? ORDER BY id DESC LIMIT 100",
        )
        .bind(project(Some(folder))?)
        .fetch_all(self.pool())
        .await?)
    }

    /// Removing a resolution restores the warning, never a different memory.
    pub async fn restoreresolution(&self, id: i64, folder: &Path) -> Result<bool> {
        Ok(
            sqlx::query("DELETE FROM driftresolution WHERE id = ? AND project = ?")
                .bind(id)
                .bind(project(Some(folder))?)
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }

    /// Recall can compact whitespace without marking a memory abridged. Only
    /// raw stored rows can be hashed as supplied; rendered rows need a reread.
    pub(crate) async fn filterdrift(
        &self,
        findings: Vec<Finding>,
        memories: &[&Memory],
        folder: Option<&Path>,
        stored: bool,
    ) -> Result<Vec<Finding>> {
        if findings.is_empty() {
            return Ok(findings);
        }
        let Some(folder) = folder else {
            return Ok(findings);
        };
        let Some(project) = crate::brain::projectkey(folder)? else {
            return Ok(findings);
        };
        let ids: Vec<_> = findings
            .iter()
            .filter_map(|finding| finding.ids.first().copied())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let mut resolved = HashSet::new();
        let mut affected = HashSet::new();
        for chunk in ids.chunks(200) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT fingerprint,memoryid FROM driftresolution WHERE project = ",
            );
            query
                .push_bind(&project)
                .push(" AND disposition != 'corrected' AND created >= ")
                .push_bind(now()? - retention::RETENTIONDAYS * 86_400)
                .push(" AND memoryid IN (");
            let mut separated = query.separated(",");
            for id in chunk {
                separated.push_bind(id);
            }
            separated.push_unseparated(")");
            let Ok(matches) = query
                .build_query_as::<(String, i64)>()
                .fetch_all(self.pool())
                .await
            else {
                return Ok(findings);
            };
            for (fingerprint, id) in matches {
                resolved.insert(fingerprint);
                affected.insert(id);
            }
        }
        if resolved.is_empty() {
            return Ok(findings);
        }
        let needed: HashSet<_> = findings
            .iter()
            .filter(|finding| finding.ids.first().is_some_and(|id| affected.contains(id)))
            .flat_map(|finding| finding.ids.iter().copied())
            .collect();
        let mut full = Vec::new();
        let neededids: Vec<_> = needed.iter().copied().collect();
        for chunk in neededids.chunks(200).filter(|_| !stored) {
            let mut query = QueryBuilder::<Sqlite>::new(format!(
                "SELECT {} FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid WHERE memory.rowid IN (",
                crate::brain::COLUMNS,
            ));
            let mut ids = query.separated(",");
            for id in chunk {
                ids.push_bind(id);
            }
            ids.push_unseparated(")");
            let Ok(memories) = query
                .build_query_as::<Memory>()
                .fetch_all(self.pool())
                .await
            else {
                return Ok(findings);
            };
            full.extend(memories);
        }
        let mut hashes = HashMap::new();
        for memory in memories.iter().copied().chain(full.iter()) {
            if needed.contains(&memory.id) {
                hashes.insert(memory.id, memoryhash(memory)?);
            }
        }
        let checkout = crate::brain::projectroot(folder)?;
        Ok(findings
            .into_iter()
            .filter(|finding| {
                !finding.ids.first().is_some_and(|id| affected.contains(id))
                    || fingerprint(finding, &hashes, checkout.as_deref())
                        .map_or(true, |key| !resolved.contains(&key))
            })
            .collect())
    }
}
