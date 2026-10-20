use anyhow::Result;
use sqlx::{Sqlite, Transaction};

pub const RETENTIONDAYS: i64 = 180;
pub const FEEDBACKLIMIT: i64 = 5_000;
pub(crate) const RESOLUTIONLIMIT: i64 = 2_000;
pub(crate) const CASELIMIT: i64 = 500;
pub(crate) const PROMOTIONLIMIT: i64 = 500;

pub(crate) async fn prune(transaction: &mut Transaction<'_, Sqlite>, now: i64) -> Result<()> {
    let before = now - RETENTIONDAYS * 86_400;
    for (table, column, limit) in [
        ("learningfeedback", "updated", FEEDBACKLIMIT),
        ("driftresolution", "created", RESOLUTIONLIMIT),
        ("learningpromotion", "created", PROMOTIONLIMIT),
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE {column} < ?"))
            .bind(before)
            .execute(&mut **transaction)
            .await?;
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE rowid IN (SELECT rowid FROM {table} \
             ORDER BY {column} DESC, rowid DESC LIMIT -1 OFFSET ?)"
        ))
        .bind(limit)
        .execute(&mut **transaction)
        .await?;
    }
    sqlx::query("DELETE FROM learningcase WHERE reviewed = 0 AND created < ?")
        .bind(before)
        .execute(&mut **transaction)
        .await?;
    sqlx::query(
        "DELETE FROM learningcase WHERE id IN (SELECT id FROM learningcase \
         ORDER BY reviewed DESC, created DESC, id DESC LIMIT -1 OFFSET ?)",
    )
    .bind(CASELIMIT)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
