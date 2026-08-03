use crate::brain::Brain;
use crate::imports::{
    ImportBatch, ImportCandidate, ImportItem, ImportPreview, ImportProvider, ImportReport,
    ImportStatus,
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

impl Brain {
    pub async fn importpreview(
        &self,
        provider: ImportProvider,
        candidates: Vec<ImportCandidate>,
    ) -> Result<ImportPreview> {
        let mut items = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let existing = sqlx::query_scalar::<_, i64>(
                "SELECT memoryid FROM memoryorigin WHERE provider = ? AND externalid = ?",
            )
            .bind(candidate.provider.value())
            .bind(&candidate.externalid)
            .fetch_optional(&self.pool)
            .await?
            .is_some();
            let status = if existing {
                ImportStatus::Existing
            } else if candidate.warning.is_some() {
                ImportStatus::Flagged
            } else {
                ImportStatus::Ready
            };
            items.push(ImportItem {
                provider: candidate.provider,
                externalid: candidate.externalid.clone(),
                preview: if candidate.warning.is_some() {
                    "Content hidden until the source file is reviewed.".to_owned()
                } else {
                    preview(&candidate.body)
                },
                source: candidate.source.clone(),
                scope: candidate.scope,
                project: candidate.project.clone(),
                path: candidate.path.display().to_string(),
                status,
                warning: candidate.warning.clone(),
                candidate,
            });
        }
        let ready = count(&items, ImportStatus::Ready);
        let existing = count(&items, ImportStatus::Existing);
        let flagged = count(&items, ImportStatus::Flagged);
        Ok(ImportPreview {
            provider,
            found: items.len(),
            ready,
            existing,
            flagged,
            items,
        })
    }

    pub async fn importmemories(
        &self,
        preview: ImportPreview,
        includeflagged: bool,
    ) -> Result<ImportReport> {
        let created = now()?;
        let mut transaction = self.pool.begin().await?;
        let batchid = sqlx::query("INSERT INTO importbatch(provider, created) VALUES (?, ?)")
            .bind(preview.provider.value())
            .bind(created)
            .execute(&mut *transaction)
            .await?
            .last_insert_rowid();
        let mut imported = 0_i64;
        let mut linked = 0_i64;
        let mut skipped = 0_i64;
        let mut flagged = 0_i64;
        for item in preview.items {
            match item.status {
                ImportStatus::Existing => {
                    skipped += 1;
                    continue;
                }
                ImportStatus::Flagged if !includeflagged => {
                    flagged += 1;
                    continue;
                }
                ImportStatus::Ready | ImportStatus::Flagged => {}
            }
            let candidate = item.candidate;
            let duplicate = sqlx::query_scalar::<_, i64>(
                "SELECT memory.rowid FROM memory \
                 JOIN memorymeta meta ON meta.memoryid = memory.rowid \
                 WHERE memory.body = ? AND meta.scope = ? AND meta.project = ? LIMIT 1",
            )
            .bind(candidate.body.trim())
            .bind(candidate.scope.value())
            .bind(&candidate.project)
            .fetch_optional(&mut *transaction)
            .await?;
            let memoryid = match duplicate {
                Some(id) => {
                    linked += 1;
                    id
                }
                None => {
                    let timestamp = if candidate.created > 0 {
                        candidate.created
                    } else {
                        created
                    };
                    let id =
                        sqlx::query("INSERT INTO memory(body, source, created) VALUES (?, ?, ?)")
                            .bind(candidate.body.trim())
                            .bind(&candidate.source)
                            .bind(timestamp)
                            .execute(&mut *transaction)
                            .await?
                            .last_insert_rowid();
                    sqlx::query(
                        "INSERT INTO memorymeta(memoryid, scope, project, native, created) \
                         VALUES (?, ?, ?, 0, ?)",
                    )
                    .bind(id)
                    .bind(candidate.scope.value())
                    .bind(&candidate.project)
                    .bind(timestamp)
                    .execute(&mut *transaction)
                    .await?;
                    imported += 1;
                    id
                }
            };
            sqlx::query(
                "INSERT INTO memoryorigin(memoryid, batchid, provider, externalid, digest, path) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(memoryid)
            .bind(batchid)
            .bind(candidate.provider.value())
            .bind(&candidate.externalid)
            .bind(digest(&candidate.body))
            .bind(candidate.path.display().to_string())
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "UPDATE importbatch SET imported = ?, linked = ?, skipped = ?, flagged = ? WHERE id = ?",
        )
        .bind(imported)
        .bind(linked)
        .bind(skipped)
        .bind(flagged)
        .bind(batchid)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let batch = self
            .importbatch(batchid)
            .await?
            .context("import batch was not saved")?;
        Ok(ImportReport {
            batch,
            stored: imported,
            linked,
            skipped,
            flagged,
        })
    }

    pub async fn importbatches(&self) -> Result<Vec<ImportBatch>> {
        sqlx::query_as(
            "SELECT id, provider, created, imported, linked, skipped, flagged, \
             CAST(undone AS BOOLEAN) AS undone FROM importbatch ORDER BY created DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("could not read import history")
    }

    pub async fn importbatch(&self, id: i64) -> Result<Option<ImportBatch>> {
        sqlx::query_as(
            "SELECT id, provider, created, imported, linked, skipped, flagged, \
             CAST(undone AS BOOLEAN) AS undone FROM importbatch WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("could not read import batch")
    }

    pub async fn undoimport(&self, id: i64) -> Result<u64> {
        let batch = self
            .importbatch(id)
            .await?
            .context("import batch not found")?;
        anyhow::ensure!(!batch.undone, "import batch {id} was already undone");
        let mut transaction = self.pool.begin().await?;
        let memories = sqlx::query_scalar::<_, i64>(
            "SELECT DISTINCT memoryid FROM memoryorigin WHERE batchid = ?",
        )
        .bind(id)
        .fetch_all(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM memoryorigin WHERE batchid = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        let mut deleted = 0_u64;
        for memoryid in memories {
            let native = sqlx::query_scalar::<_, bool>(
                "SELECT CAST(native AS BOOLEAN) FROM memorymeta WHERE memoryid = ?",
            )
            .bind(memoryid)
            .fetch_optional(&mut *transaction)
            .await?
            .unwrap_or(true);
            let origins = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM memoryorigin WHERE memoryid = ?",
            )
            .bind(memoryid)
            .fetch_one(&mut *transaction)
            .await?;
            if !native && origins == 0 {
                sqlx::query("DELETE FROM memorymeta WHERE memoryid = ?")
                    .bind(memoryid)
                    .execute(&mut *transaction)
                    .await?;
                deleted += sqlx::query("DELETE FROM memory WHERE rowid = ?")
                    .bind(memoryid)
                    .execute(&mut *transaction)
                    .await?
                    .rows_affected();
            }
        }
        sqlx::query("UPDATE importbatch SET undone = 1 WHERE id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(deleted)
    }
}

fn count(items: &[ImportItem], status: ImportStatus) -> usize {
    items.iter().filter(|item| item.status == status).count()
}

fn preview(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = compact.chars();
    let preview = characters.by_ref().take(120).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn digest(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MemoryScope;
    use std::path::PathBuf;

    fn candidate(externalid: &str, body: &str) -> ImportCandidate {
        ImportCandidate {
            provider: ImportProvider::Markdown,
            externalid: externalid.to_owned(),
            body: body.to_owned(),
            source: "Markdown · test".to_owned(),
            scope: MemoryScope::Global,
            project: String::new(),
            path: PathBuf::from("test.md"),
            created: 1,
            warning: None,
        }
    }

    #[tokio::test]
    async fn imports_are_idempotent_and_reversible() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let preview = brain
            .importpreview(
                ImportProvider::Markdown,
                vec![candidate("one", "durable imported choice")],
            )
            .await
            .unwrap();
        let report = brain.importmemories(preview, false).await.unwrap();
        assert_eq!(report.stored, 1);
        assert_eq!(brain.stats().await.unwrap().entries, 1);

        let repeated = brain
            .importpreview(
                ImportProvider::Markdown,
                vec![candidate("one", "durable imported choice")],
            )
            .await
            .unwrap();
        assert_eq!(repeated.existing, 1);
        assert_eq!(brain.undoimport(report.batch.id).await.unwrap(), 1);
        assert_eq!(brain.stats().await.unwrap().entries, 0);
    }

    #[tokio::test]
    async fn undo_preserves_an_imported_memory_after_manual_editing() {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let preview = brain
            .importpreview(ImportProvider::Markdown, vec![candidate("one", "original")])
            .await
            .unwrap();
        let report = brain.importmemories(preview, false).await.unwrap();
        brain.updatememory(1, "corrected", None).await.unwrap();

        assert_eq!(brain.undoimport(report.batch.id).await.unwrap(), 0);
        assert_eq!(brain.memory(1).await.unwrap().unwrap().body, "corrected");
    }
}
