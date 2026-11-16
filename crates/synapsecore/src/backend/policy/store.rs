use super::rank::ArmRow;
use crate::backend::PolicyState;
use anyhow::Result;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::collections::HashMap;

const ROLLBACKTRIALS: i64 = 4;
const REGRESSION: f64 = 0.15;
const DISABLEFOR: i64 = 86_400;
const DECISIONDAYS: i64 = 90;
const ARMRETENTIONDAYS: i64 = 365;
const DECISIONLIMIT: i64 = 50_000;
const ARMLIMIT: i64 = 10_000;

pub(super) async fn state(pool: &SqlitePool, kind: &str, context: &str) -> Result<PolicyState> {
    Ok(
        sqlx::query_as("SELECT * FROM policystate WHERE kind=? AND context=?")
            .bind(kind)
            .bind(context)
            .fetch_optional(pool)
            .await?
            .unwrap_or_else(|| PolicyState::new(kind, context)),
    )
}

pub(super) async fn arms(
    pool: &SqlitePool,
    kind: &str,
    context: &str,
    choices: &[String],
) -> Result<HashMap<String, ArmRow>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT action,trials,reward,duration FROM policyarm WHERE kind=",
    );
    query
        .push_bind(kind)
        .push(" AND context=")
        .push_bind(context);
    query.push(" AND action IN (");
    let mut separated = query.separated(",");
    for choice in choices {
        separated.push_bind(choice);
    }
    separated.push_unseparated(")");
    Ok(query
        .build_query_as::<ArmRow>()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|arm| (arm.action.clone(), arm))
        .collect())
}

pub(super) async fn sequence(pool: &SqlitePool, kind: &str, context: &str) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM policydecision WHERE kind=? AND context=?")
        .bind(kind)
        .bind(context)
        .fetch_one(pool)
        .await
        .map_err(Into::into)
}

pub(super) async fn prune(pool: &SqlitePool, now: i64) -> Result<()> {
    sqlx::query("DELETE FROM policydecision WHERE created<?")
        .bind(now - DECISIONDAYS * 86_400)
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM policydecision WHERE id NOT IN \
         (SELECT id FROM policydecision ORDER BY id DESC LIMIT ?)",
    )
    .bind(DECISIONLIMIT)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM policyarm WHERE updated<?")
        .bind(now - ARMRETENTIONDAYS * 86_400)
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM policyarm WHERE rowid NOT IN \
         (SELECT rowid FROM policyarm ORDER BY updated DESC LIMIT ?)",
    )
    .bind(ARMLIMIT)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn rollback(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    kind: &str,
    context: &str,
    now: i64,
) -> Result<()> {
    let state: PolicyState = sqlx::query_as("SELECT * FROM policystate WHERE kind=? AND context=?")
        .bind(kind)
        .bind(context)
        .fetch_one(&mut **transaction)
        .await?;
    if state.disableduntil > now
        || state.baselinetrials < ROLLBACKTRIALS
        || state.learnedtrials < ROLLBACKTRIALS
    {
        return Ok(());
    }
    let baseline = state.baselinereward / state.baselinetrials as f64;
    let learned = state.learnedreward / state.learnedtrials as f64;
    if learned + REGRESSION >= baseline {
        return Ok(());
    }
    sqlx::query(
        "UPDATE policystate SET disableduntil=?,regressions=regressions+1,version=version+1,\
         learnedtrials=0,learnedreward=0,updated=? WHERE kind=? AND context=?",
    )
    .bind(now + DISABLEFOR)
    .bind(now)
    .bind(kind)
    .bind(context)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
