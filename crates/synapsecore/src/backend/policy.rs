//! Quiet local choice ranking from observed outcomes.
//!
//! The policy only sees choices that the caller has already declared eligible. It cannot grant a
//! capability, widen a scope, or cross a safety boundary. Sparse evidence stays in shadow mode;
//! low confidence falls back to the caller's first choice; regressions disable learned selection
//! for a day and start a fresh comparison window.

mod rank;
mod store;

use self::rank::{
    ARMTRIALS, EXPLOREVERY, MINCONFIDENCE, WARMUP, confidence, distinct, now, recommend,
    validaction, validlabel,
};
use super::{Backend, PolicyArm, PolicyChoice, PolicyDecision, PolicyState};
use anyhow::{Context, Result};

pub(crate) use self::rank::cleancontext;

impl Backend {
    /// Rank choices that have already passed the caller's deterministic filters.
    ///
    /// The first choice is the baseline. Calling this with a different order changes the fallback,
    /// which is intentional: policy failure must preserve the caller's current deterministic rule.
    pub async fn choose(
        &self,
        kind: &str,
        context: &str,
        choices: &[String],
        runid: &str,
    ) -> Result<PolicyChoice> {
        validlabel(kind, "policy kind")?;
        let context = cleancontext(context);
        let choices = distinct(choices)?;
        let baseline = choices
            .first()
            .context("policy needs at least one eligible choice")?;
        let now = now();
        let state = store::state(&self.pool, kind, &context).await?;
        let arms = store::arms(&self.pool, kind, &context, &choices).await?;
        let total = arms.values().map(|arm| arm.trials).sum::<i64>();
        let recommendation = recommend(&choices, &arms, total);
        let confidence = confidence(&recommendation, &choices, &arms);
        let ready = total >= WARMUP
            && choices
                .iter()
                .all(|choice| arms.get(choice).is_some_and(|arm| arm.trials >= ARMTRIALS))
            && confidence >= MINCONFIDENCE;
        let sequence = store::sequence(&self.pool, kind, &context).await? + 1;
        let exploring = total >= WARMUP
            && sequence % EXPLOREVERY == 0
            && choices
                .iter()
                .any(|choice| arms.get(choice).map_or(0, |arm| arm.trials) < ARMTRIALS);
        let active = state.enabled && state.disableduntil <= now;
        let (selected, mode, reason) = if exploring && active {
            let selected = choices
                .iter()
                .min_by_key(|choice| arms.get(*choice).map_or(0, |arm| arm.trials))
                .expect("choices is not empty")
                .clone();
            (
                selected,
                "explore",
                "bounded exploration among eligible choices".to_owned(),
            )
        } else if ready && active {
            (
                recommendation.clone(),
                "learned",
                "ranked from local outcomes".to_owned(),
            )
        } else if total > 0 {
            (
                baseline.clone(),
                "shadow",
                if active {
                    "insufficient evidence; used deterministic fallback".to_owned()
                } else {
                    "policy paused after regression; used deterministic fallback".to_owned()
                },
            )
        } else {
            (
                baseline.clone(),
                "baseline",
                "no local evidence; used deterministic fallback".to_owned(),
            )
        };
        let decision = sqlx::query_as::<_, PolicyDecision>(
      "INSERT INTO policydecision(kind,context,baseline,selected,recommendation,mode,confidence,\
       reason,runid,created,updated) VALUES(?,?,?,?,?,?,?,?,?,?,?) RETURNING *",
    )
    .bind(kind)
    .bind(&context)
    .bind(baseline)
    .bind(&selected)
    .bind(&recommendation)
    .bind(mode)
    .bind(confidence)
    .bind(reason)
    .bind(runid)
    .bind(now)
    .bind(now)
    .fetch_one(&self.pool)
    .await?;
        store::prune(&self.pool, now).await?;
        Ok(PolicyChoice {
            action: selected,
            decision,
        })
    }

    /// Record a choice made outside the policy so normal work still becomes evidence.
    pub(crate) async fn observechoice(
        &self,
        kind: &str,
        context: &str,
        action: &str,
        runid: &str,
    ) -> Result<PolicyDecision> {
        validlabel(kind, "policy kind")?;
        validaction(action)?;
        let context = cleancontext(context);
        let now = now();
        let decision = sqlx::query_as(
      "INSERT INTO policydecision(kind,context,baseline,selected,recommendation,mode,confidence,\
       reason,runid,created,updated) VALUES(?,?,?,?,?,'observed',0,?,?,?,?) RETURNING *",
    )
    .bind(kind)
    .bind(&context)
    .bind(action)
    .bind(action)
    .bind(action)
    .bind("choice observed from normal work")
    .bind(runid)
    .bind(now)
    .bind(now)
    .fetch_one(&self.pool)
        .await?;
        store::prune(&self.pool, now).await?;
        Ok(decision)
    }

    /// Attach or revise an outcome. Revisions adjust aggregates by their delta instead of counting
    /// one run twice, which lets a later evaluator replace a coarse terminal-status reward.
    pub async fn rewarddecision(
        &self,
        id: i64,
        reward: f64,
        duration: i64,
    ) -> Result<PolicyDecision> {
        anyhow::ensure!(
            (0.0..=1.0).contains(&reward),
            "policy reward must be between 0 and 1"
        );
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let current: PolicyDecision = sqlx::query_as("SELECT * FROM policydecision WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?
            .context("policy decision does not exist")?;
        let first = current.reward.is_none();
        let oldreward = current.reward.unwrap_or(0.0);
        let oldduration = current.duration;
        let aggregateexists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM policyarm WHERE kind=? AND context=? AND action=?)",
        )
        .bind(&current.kind)
        .bind(&current.context)
        .bind(&current.selected)
        .fetch_one(&mut *transaction)
        .await?;
        let duration = duration.max(0);
        let now = now();
        sqlx::query("UPDATE policydecision SET reward=?,duration=?,updated=? WHERE id=?")
            .bind(reward)
            .bind(duration)
            .bind(now)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        // A reset deliberately removes aggregates while retaining the decision audit trail. A
        // late revision may update that trail, but must not silently recreate a partial arm with
        // zero trials and only the reward delta.
        if !first && !aggregateexists {
            transaction.commit().await?;
            return sqlx::query_as("SELECT * FROM policydecision WHERE id=?")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(Into::into);
        }
        sqlx::query(
      "INSERT INTO policyarm(kind,context,action,trials,reward,duration,updated) VALUES(?,?,?,?,?,?,?) \
       ON CONFLICT(kind,context,action) DO UPDATE SET \
       trials=policyarm.trials+excluded.trials,reward=policyarm.reward+excluded.reward,\
       duration=MAX(0,policyarm.duration+excluded.duration),updated=excluded.updated",
    )
    .bind(&current.kind)
    .bind(&current.context)
    .bind(&current.selected)
    .bind(i64::from(first))
    .bind(reward - oldreward)
    .bind(duration - oldduration)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
        let learned = current.mode == "learned";
        let baseline = matches!(current.mode.as_str(), "baseline" | "shadow" | "observed");
        sqlx::query(
            "INSERT INTO policystate(kind,context,baselinetrials,baselinereward,learnedtrials,\
       learnedreward,updated) VALUES(?,?,?,?,?,?,?) ON CONFLICT(kind,context) DO UPDATE SET \
       baselinetrials=policystate.baselinetrials+excluded.baselinetrials,\
       baselinereward=policystate.baselinereward+excluded.baselinereward,\
       learnedtrials=policystate.learnedtrials+excluded.learnedtrials,\
       learnedreward=policystate.learnedreward+excluded.learnedreward,updated=excluded.updated",
        )
        .bind(&current.kind)
        .bind(&current.context)
        .bind(i64::from(first && baseline))
        .bind(if baseline { reward - oldreward } else { 0.0 })
        .bind(i64::from(first && learned))
        .bind(if learned { reward - oldreward } else { 0.0 })
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        store::rollback(&mut transaction, &current.kind, &current.context, now).await?;
        transaction.commit().await?;
        sqlx::query_as("SELECT * FROM policydecision WHERE id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn policydecisions(&self, limit: i64) -> Result<Vec<PolicyDecision>> {
        sqlx::query_as("SELECT * FROM policydecision ORDER BY id DESC LIMIT ?")
            .bind(limit.clamp(1, 500))
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn policyarms(&self) -> Result<Vec<PolicyArm>> {
        sqlx::query_as(
            "SELECT * FROM policyarm ORDER BY kind,context,reward/NULLIF(trials,0) DESC,action",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn policystates(&self) -> Result<Vec<PolicyState>> {
        sqlx::query_as("SELECT * FROM policystate ORDER BY kind,context")
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// Enable or disable one policy context. This is an observability control, not part of normal
    /// operation; an absent row is enabled by default.
    pub async fn setpolicyenabled(
        &self,
        kind: &str,
        context: &str,
        enabled: bool,
    ) -> Result<PolicyState> {
        validlabel(kind, "policy kind")?;
        let context = cleancontext(context);
        let now = now();
        sqlx::query(
            "INSERT INTO policystate(kind,context,enabled,updated) VALUES(?,?,?,?) \
       ON CONFLICT(kind,context) DO UPDATE SET enabled=excluded.enabled,disableduntil=0,\
       version=policystate.version+1,updated=excluded.updated",
        )
        .bind(kind)
        .bind(&context)
        .bind(enabled)
        .bind(now)
        .execute(&self.pool)
        .await?;
        store::state(&self.pool, kind, &context).await
    }

    /// Forget learned aggregates while retaining the audit trail of decisions.
    pub async fn resetpolicy(&self, kind: &str, context: &str) -> Result<()> {
        validlabel(kind, "policy kind")?;
        let context = cleancontext(context);
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM policyarm WHERE kind=? AND context=?")
            .bind(kind)
            .bind(&context)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM policystate WHERE kind=? AND context=?")
            .bind(kind)
            .bind(&context)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn rewardrun(&self, runid: &str, reward: f64, duration: i64) -> Result<()> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT MAX(id) FROM policydecision WHERE runid=? AND kind IN ('runner','route') \
             GROUP BY kind",
        )
        .bind(runid)
        .fetch_all(&self.pool)
        .await?;
        for id in ids {
            self.rewarddecision(id, reward, duration).await?;
        }
        Ok(())
    }
}
