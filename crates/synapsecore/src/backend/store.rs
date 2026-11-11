use super::{
    Artifact, Capability, Decision, Evaluation, Event, EventDraft, EventFilter, Route, Run,
    RunStatus, Step, StepStatus, Summary, Trigger,
};
use crate::database::Opened;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const EVENTLIMIT: i64 = 50_000;
const EVENTRETENTION: i64 = 30 * 86400;
const MESSAGELIMIT: usize = 16 * 1024;
const FIELDSLIMIT: usize = 64 * 1024;
const ARTIFACTLIMIT: usize = 128 * 1024 * 1024;
const STEPLIMIT: i64 = 1_000;
const CONFIGLIMIT: i64 = 1_024;
const RUNLIMIT: i64 = 10_000;
const RUNRETENTION: i64 = 90 * 86400;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct Backend {
    pool: SqlitePool,
    _lock: Arc<File>,
    artifacts: PathBuf,
}

impl Backend {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::from(
            crate::database::open(path).await?,
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join("artifacts"),
        )
    }

    pub async fn glance(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::from(
            crate::database::glance(path).await?,
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join("artifacts"),
        )
    }

    pub fn sharing(brain: &crate::brain::Brain) -> Result<Self> {
        Self::from(brain.handle(), crate::files::data()?.join("artifacts"))
    }

    fn from(opened: Opened, artifacts: PathBuf) -> Result<Self> {
        Ok(Self {
            pool: opened.pool,
            _lock: opened.lock,
            artifacts,
        })
    }

    pub async fn record(&self, draft: EventDraft) -> Result<Event> {
        recordpool(&self.pool, draft).await
    }

    pub async fn events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let limit = filter.limit.clamp(1, 500);
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT id,created,source,project,agent,session,level,kind,message,fields,runid,traceid FROM event WHERE 1=1",
        );
        if filter.after > 0 {
            query.push(" AND id > ").push_bind(filter.after);
        }
        for (column, value) in [
            ("project", filter.project.as_deref()),
            ("agent", filter.agent.as_deref()),
            ("source", filter.source.as_deref()),
            ("runid", filter.runid.as_deref()),
        ] {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                query
                    .push(" AND ")
                    .push(column)
                    .push(" = ")
                    .push_bind(value);
            }
        }
        if let Some(level) = filter.level {
            query.push(" AND level = ").push_bind(level);
        }
        if let Some(value) = filter.query.as_deref().filter(|value| !value.is_empty()) {
            query
                .push(" AND (message LIKE ")
                .push_bind(format!("%{value}%"))
                .push(" OR kind LIKE ")
                .push_bind(format!("%{value}%"))
                .push(")");
        }
        if filter.after > 0 {
            query.push(" ORDER BY id ASC LIMIT ").push_bind(limit);
        } else {
            query.push(" ORDER BY id DESC LIMIT ").push_bind(limit);
        }
        let mut events = query
            .build_query_as::<Event>()
            .fetch_all(&self.pool)
            .await?;
        if filter.after <= 0 {
            events.reverse();
        }
        Ok(events)
    }

    pub async fn summary(&self) -> Result<Summary> {
        let (running, failed, queued): (i64, i64, i64) = sqlx::query_as(
      "SELECT COALESCE(SUM(status IN ('running','waiting')),0), COALESCE(SUM(status = 'failed'),0), COALESCE(SUM(status = 'queued'),0) FROM run",
    )
    .fetch_one(&self.pool)
    .await?;
        let errors: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM event WHERE level = 'error' AND created >= ?")
                .bind(now() - 86400)
                .fetch_one(&self.pool)
                .await?;
        let lastevent: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id),0) FROM event")
            .fetch_one(&self.pool)
            .await?;
        let liveagents: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM meshagent WHERE registered = 1 AND seen >= ?")
                .bind(now() - 90)
                .fetch_one(&self.pool)
                .await?;
        Ok(Summary {
            liveagents: liveagents as usize,
            running: running as usize,
            failed: failed as usize,
            queued: queued as usize,
            errors: errors as usize,
            lastevent,
        })
    }

    pub async fn createrun(
        &self,
        project: &str,
        harness: &str,
        input: serde_json::Value,
    ) -> Result<Run> {
        let id = identifier("run");
        self.insertrun(&id, project, harness, cleanjson(input)?, "")
            .await
    }

    async fn insertrun(
        &self,
        id: &str,
        project: &str,
        harness: &str,
        input: String,
        replayof: &str,
    ) -> Result<Run> {
        let time = now();
        let project = crate::brain::redact(project).body;
        let harness = crate::brain::redact(harness).body;
        sqlx::query(
      "INSERT INTO run(id,project,harness,status,input,replayof,created,updated) VALUES(?,?,?,'queued',?,?,?,?)",
    )
    .bind(id)
    .bind(&project)
    .bind(&harness)
    .bind(&input)
    .bind(replayof)
    .bind(time)
    .bind(time)
    .execute(&self.pool)
    .await?;
        self.pruneruns(time).await?;
        self.run(id).await?.context("created run is missing")
    }

    pub async fn run(&self, id: &str) -> Result<Option<Run>> {
        sqlx::query_as("SELECT id,project,harness,status,input,output,error,replayof,leaseowner,leaseuntil,attempts,maxattempts,created,updated FROM run WHERE id = ?")
      .bind(id)
      .fetch_optional(&self.pool)
      .await
      .map_err(Into::into)
    }

    pub async fn runs(&self, limit: i64) -> Result<Vec<Run>> {
        sqlx::query_as(
      "SELECT id,project,harness,status,input,output,error,replayof,leaseowner,leaseuntil,attempts,maxattempts,created,updated FROM run ORDER BY updated DESC,id DESC LIMIT ?",
    )
    .bind(limit.clamp(1, 500))
    .fetch_all(&self.pool)
    .await
    .map_err(Into::into)
    }

    pub async fn updaterun(
        &self,
        id: &str,
        status: RunStatus,
        output: &str,
        error: &str,
    ) -> Result<Run> {
        let output = crate::brain::redact(output).body;
        let error = crate::brain::redact(error).body;
        let changed = sqlx::query(
            "UPDATE run SET status = ?, output = ?, error = ?, updated = ? WHERE id = ?",
        )
        .bind(status)
        .bind(output)
        .bind(error)
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        anyhow::ensure!(changed == 1, "run `{id}` does not exist");
        self.run(id).await?.context("updated run is missing")
    }

    pub async fn replay(&self, id: &str) -> Result<Run> {
        let run = self
            .run(id)
            .await?
            .with_context(|| format!("run `{id}` does not exist"))?;
        self.insertrun(
            &identifier("run"),
            &run.project,
            &run.harness,
            run.input,
            &run.id,
        )
        .await
    }

    pub async fn claim(&self, worker: &str, lease: i64) -> Result<Option<Run>> {
        validateword(worker, "worker")?;
        let time = now();
        let until = time + lease.clamp(10, 3600);
        let mut transaction = self.pool.begin().await?;
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM run WHERE (status='queued' OR (status='running' AND leaseuntil<?)) \
             AND attempts<maxattempts ORDER BY created,id LIMIT 1",
        )
        .bind(time)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(id) = id else {
            transaction.commit().await?;
            return Ok(None);
        };
        let changed = sqlx::query(
            "UPDATE run SET status='running',leaseowner=?,leaseuntil=?,attempts=attempts+1,updated=? \
             WHERE id=? AND (status='queued' OR leaseuntil<?)",
        )
        .bind(worker)
        .bind(until)
        .bind(time)
        .bind(&id)
        .bind(time)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        transaction.commit().await?;
        if changed == 0 {
            return Ok(None);
        }
        self.run(&id).await
    }

    pub async fn renew(&self, id: &str, worker: &str, lease: i64) -> Result<bool> {
        let changed = sqlx::query(
            "UPDATE run SET leaseuntil=?,updated=? WHERE id=? AND status='running' AND leaseowner=?",
        )
        .bind(now() + lease.clamp(10, 3600))
        .bind(now())
        .bind(id)
        .bind(worker)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    pub async fn cancel(&self, id: &str) -> Result<Run> {
        self.updaterun(id, RunStatus::Cancelled, "", "cancelled")
            .await
    }

    pub async fn retry(&self, id: &str) -> Result<Run> {
        let changed = sqlx::query(
            "UPDATE run SET status='queued',leaseowner='',leaseuntil=0,error='',updated=? \
             WHERE id=? AND status IN ('failed','waiting') AND attempts<maxattempts",
        )
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        anyhow::ensure!(changed == 1, "run `{id}` cannot be retried");
        self.run(id).await?.context("retried run is missing")
    }

    pub async fn addstep(&self, runid: &str, name: &str, input: serde_json::Value) -> Result<Step> {
        anyhow::ensure!(
            self.run(runid).await?.is_some(),
            "run `{runid}` does not exist"
        );
        validateword(name, "step name")?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runstep WHERE runid=?")
            .bind(runid)
            .fetch_one(&self.pool)
            .await?;
        anyhow::ensure!(
            count < STEPLIMIT,
            "a run may contain at most {STEPLIMIT} steps"
        );
        let id = sqlx::query("INSERT INTO runstep(runid,name,status,input) VALUES(?,?,'queued',?)")
            .bind(runid)
            .bind(name)
            .bind(cleanjson(input)?)
            .execute(&self.pool)
            .await?
            .last_insert_rowid();
        self.step(id).await?.context("created step is missing")
    }

    pub async fn updatestep(
        &self,
        id: i64,
        status: StepStatus,
        output: &str,
        error: &str,
    ) -> Result<Step> {
        let time = now();
        let started = i64::from(status == StepStatus::Running) * time;
        let ended = i64::from(matches!(
            status,
            StepStatus::Succeeded | StepStatus::Failed | StepStatus::Cancelled
        )) * time;
        let changed = sqlx::query(
            "UPDATE runstep SET status=?, output=?, error=?, \
       started=CASE WHEN ? > 0 AND started=0 THEN ? ELSE started END, \
       ended=CASE WHEN ? > 0 THEN ? ELSE ended END WHERE id=?",
        )
        .bind(status)
        .bind(crate::brain::redact(output).body)
        .bind(crate::brain::redact(error).body)
        .bind(started)
        .bind(started)
        .bind(ended)
        .bind(ended)
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        anyhow::ensure!(changed == 1, "step `{id}` does not exist");
        self.step(id).await?.context("updated step is missing")
    }

    pub async fn steps(&self, runid: &str) -> Result<Vec<Step>> {
        sqlx::query_as(
      "SELECT id,runid,name,status,input,output,error,started,ended FROM runstep WHERE runid=? ORDER BY id",
    )
    .bind(runid)
    .fetch_all(&self.pool)
    .await
    .map_err(Into::into)
    }

    async fn step(&self, id: i64) -> Result<Option<Step>> {
        sqlx::query_as(
            "SELECT id,runid,name,status,input,output,error,started,ended FROM runstep WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn putartifact(
        &self,
        name: &str,
        kind: &str,
        media: &str,
        content: &[u8],
    ) -> Result<Artifact> {
        anyhow::ensure!(
            content.len() <= ARTIFACTLIMIT,
            "artifact is larger than 128 MiB"
        );
        validateword(kind, "artifact kind")?;
        let name = crate::brain::redact(name).body;
        let media = crate::brain::redact(media).body;
        let digest = hex(&Sha256::digest(content));
        let folder = self.artifacts.join(&digest[..2]);
        std::fs::create_dir_all(&folder)
            .with_context(|| format!("could not create {}", folder.display()))?;
        crate::database::securedirectory(&folder)?;
        let path = folder.join(&digest);
        if !path.exists() {
            crate::files::atomicreplace(&path, content, None)?;
            crate::database::securefile(&path)?;
        }
        sqlx::query(
      "INSERT OR IGNORE INTO artifact(digest,name,kind,media,path,size,created) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(&digest)
    .bind(&name)
    .bind(kind)
    .bind(&media)
    .bind(path.display().to_string())
    .bind(content.len() as i64)
    .bind(now())
    .execute(&self.pool)
    .await?;
        self.artifact(&digest)
            .await?
            .context("stored artifact is missing")
    }

    pub async fn artifact(&self, digest: &str) -> Result<Option<Artifact>> {
        sqlx::query_as(
            "SELECT digest,name,kind,media,path,size,created FROM artifact WHERE digest=?",
        )
        .bind(digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn attach(&self, runid: &str, digest: &str, role: &str) -> Result<()> {
        validateword(role, "artifact role")?;
        sqlx::query("INSERT OR IGNORE INTO runartifact(runid,digest,role) VALUES(?,?,?)")
            .bind(runid)
            .bind(digest)
            .bind(role)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn artifacts(&self, runid: &str) -> Result<Vec<Artifact>> {
        sqlx::query_as(
            "SELECT a.digest,a.name,a.kind,a.media,a.path,a.size,a.created FROM artifact a \
             JOIN runartifact r ON r.digest=a.digest WHERE r.runid=? ORDER BY a.created,a.digest",
        )
        .bind(runid)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn readartifact(&self, digest: &str) -> Result<Vec<u8>> {
        let artifact = self
            .artifact(digest)
            .await?
            .with_context(|| format!("artifact `{digest}` does not exist"))?;
        anyhow::ensure!(
            artifact.size <= ARTIFACTLIMIT as i64,
            "artifact is too large"
        );
        let file = File::open(&artifact.path)
            .with_context(|| format!("could not open artifact `{digest}`"))?;
        anyhow::ensure!(
            file.metadata()?.len() <= ARTIFACTLIMIT as u64,
            "artifact is too large"
        );
        let mut content = Vec::with_capacity(artifact.size as usize);
        file.take(ARTIFACTLIMIT as u64 + 1)
            .read_to_end(&mut content)
            .with_context(|| format!("could not read artifact `{digest}`"))?;
        anyhow::ensure!(content.len() <= ARTIFACTLIMIT, "artifact is too large");
        anyhow::ensure!(
            hex(&Sha256::digest(&content)) == digest,
            "artifact digest mismatch"
        );
        Ok(content)
    }

    pub async fn decide(
        &self,
        subject: &str,
        kind: &str,
        resource: &str,
        decision: Decision,
        digest: &str,
    ) -> Result<Capability> {
        validateword(subject, "capability subject")?;
        validateword(kind, "capability kind")?;
        let resource = crate::brain::redact(resource).body;
        self.ensureconfig(
            "capability",
            "subject=? AND kind=? AND resource=?",
            &[subject, kind, &resource],
        )
        .await?;
        sqlx::query(
      "INSERT INTO capability(subject,kind,resource,decision,digest,updated) VALUES(?,?,?,?,?,?) \
       ON CONFLICT(subject,kind,resource) DO UPDATE SET decision=excluded.decision,digest=excluded.digest,updated=excluded.updated",
    )
    .bind(subject)
    .bind(kind)
    .bind(&resource)
    .bind(decision)
    .bind(digest)
    .bind(now())
    .execute(&self.pool)
    .await?;
        self.capabilities()
            .await?
            .into_iter()
            .find(|item| item.subject == subject && item.kind == kind && item.resource == resource)
            .context("stored capability is missing")
    }

    pub async fn allowed(
        &self,
        subject: &str,
        kind: &str,
        resource: &str,
        digest: &str,
    ) -> Result<bool> {
        let resource = crate::brain::redact(resource).body;
        let row: Option<(Decision, String)> = sqlx::query_as(
            "SELECT decision,digest FROM capability WHERE subject=? AND kind=? AND resource=?",
        )
        .bind(subject)
        .bind(kind)
        .bind(&resource)
        .fetch_optional(&self.pool)
        .await?;
        Ok(matches!(row, Some((Decision::Allow, saved)) if saved.is_empty() || saved == digest))
    }

    pub async fn capabilities(&self) -> Result<Vec<Capability>> {
        sqlx::query_as(
      "SELECT subject,kind,resource,decision,digest,updated FROM capability ORDER BY subject,kind,resource",
    )
    .fetch_all(&self.pool)
    .await
    .map_err(Into::into)
    }

    pub async fn puttrigger(
        &self,
        name: &str,
        kind: &str,
        spec: serde_json::Value,
        next: i64,
    ) -> Result<Trigger> {
        validateword(name, "trigger name")?;
        self.ensureconfig("trigger", "name=?", &[name]).await?;
        anyhow::ensure!(
            matches!(kind, "schedule" | "event" | "dependency"),
            "invalid trigger kind"
        );
        let time = now();
        sqlx::query(
      "INSERT INTO trigger(name,kind,spec,next,created,updated) VALUES(?,?,?,?,?,?) \
       ON CONFLICT(name) DO UPDATE SET kind=excluded.kind,spec=excluded.spec,next=excluded.next,enabled=1,updated=excluded.updated",
    )
    .bind(name)
    .bind(kind)
    .bind(cleanjson(spec)?)
    .bind(next)
    .bind(time)
    .bind(time)
    .execute(&self.pool)
    .await?;
        self.trigger(name)
            .await?
            .context("stored trigger is missing")
    }

    pub async fn trigger(&self, name: &str) -> Result<Option<Trigger>> {
        sqlx::query_as(
      "SELECT id,name,kind,spec,enabled,next,lastrun,claimed,created,updated FROM trigger WHERE name=?",
    )
    .bind(name)
    .fetch_optional(&self.pool)
    .await
    .map_err(Into::into)
    }

    pub async fn triggers(&self) -> Result<Vec<Trigger>> {
        sqlx::query_as(
      "SELECT id,name,kind,spec,enabled,next,lastrun,claimed,created,updated FROM trigger ORDER BY name",
    )
    .fetch_all(&self.pool)
    .await
    .map_err(Into::into)
    }

    pub async fn due(&self, time: i64) -> Result<Vec<Trigger>> {
        sqlx::query_as(
      "SELECT id,name,kind,spec,enabled,next,lastrun,claimed,created,updated FROM trigger WHERE enabled=1 AND next>0 AND next<=? ORDER BY next,id LIMIT 100",
    )
    .bind(time)
    .fetch_all(&self.pool)
    .await
    .map_err(Into::into)
    }

    pub async fn marktrigger(&self, name: &str, next: i64) -> Result<()> {
        sqlx::query("UPDATE trigger SET lastrun=?,next=?,claimed=0,updated=? WHERE name=?")
            .bind(now())
            .bind(next)
            .bind(now())
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn tick(&self, time: i64) -> Result<Vec<Run>> {
        let mut runs = Vec::new();
        let claimstamp = time.max(1);
        for trigger in self
            .triggers()
            .await?
            .into_iter()
            .filter(|item| item.enabled)
        {
            let spec: serde_json::Value = serde_json::from_str(&trigger.spec)
                .with_context(|| format!("trigger `{}` has invalid spec", trigger.name))?;
            let Some(marker) = self.triggerready(&trigger, &spec, time).await? else {
                continue;
            };
            let claimed = sqlx::query(
                "UPDATE trigger SET claimed=?,updated=? WHERE id=? AND enabled=1 \
                 AND (claimed=0 OR claimed<?) AND lastrun=? AND next=?",
            )
            .bind(claimstamp)
            .bind(time)
            .bind(trigger.id)
            .bind(claimstamp - 300)
            .bind(trigger.lastrun)
            .bind(trigger.next)
            .execute(&self.pool)
            .await?
            .rows_affected();
            if claimed == 0 {
                continue;
            }
            let project = spec
                .get("project")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let harness = spec
                .get("harness")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let input = spec
                .get("input")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let interval = spec
                .get("interval")
                .or_else(|| spec.get("every"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let run = match self.createrun(project, harness, input).await {
                Ok(run) => run,
                Err(error) => {
                    sqlx::query("UPDATE trigger SET claimed=0 WHERE id=?")
                        .bind(trigger.id)
                        .execute(&self.pool)
                        .await?;
                    return Err(error);
                }
            };
            let next = if trigger.kind == "schedule" && interval > 0 {
                time + interval
            } else {
                0
            };
            sqlx::query(
                "UPDATE trigger SET lastrun=?,next=?,claimed=0,updated=? WHERE id=? AND claimed=?",
            )
            .bind(marker)
            .bind(next)
            .bind(time)
            .bind(trigger.id)
            .bind(claimstamp)
            .execute(&self.pool)
            .await?;
            runs.push(run);
        }
        Ok(runs)
    }

    async fn triggerready(
        &self,
        trigger: &Trigger,
        spec: &serde_json::Value,
        time: i64,
    ) -> Result<Option<i64>> {
        match trigger.kind.as_str() {
            "schedule" => Ok((trigger.next > 0 && trigger.next <= time).then_some(time)),
            "event" => {
                let mut query = QueryBuilder::<Sqlite>::new("SELECT id FROM event WHERE id>");
                query.push_bind(trigger.lastrun);
                for (column, key) in [
                    ("kind", "eventkind"),
                    ("source", "source"),
                    ("agent", "agent"),
                    ("project", "project"),
                ] {
                    if let Some(value) = spec.get(key).and_then(|value| value.as_str()) {
                        query.push(" AND ").push(column).push("=").push_bind(value);
                    }
                }
                query
                    .push(" ORDER BY id LIMIT 1")
                    .build_query_scalar::<i64>()
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(Into::into)
            }
            "dependency" => {
                if trigger.lastrun != 0 {
                    return Ok(None);
                }
                let run = spec
                    .get("run")
                    .and_then(|value| value.as_str())
                    .context("a dependency trigger needs a run")?;
                let expected = spec
                    .get("status")
                    .and_then(|value| value.as_str())
                    .unwrap_or("succeeded");
                let status: Option<String> =
                    sqlx::query_scalar("SELECT status FROM run WHERE id=?")
                        .bind(run)
                        .fetch_optional(&self.pool)
                        .await?;
                Ok((status.as_deref() == Some(expected)).then_some(time))
            }
            _ => anyhow::bail!("trigger `{}` has an invalid kind", trigger.name),
        }
    }

    pub async fn putroute(
        &self,
        name: &str,
        endpoint: &str,
        model: &str,
        priority: i64,
        local: bool,
    ) -> Result<Route> {
        validateword(name, "route name")?;
        self.ensureconfig("route", "name=?", &[name]).await?;
        validendpoint(endpoint, local)?;
        let model = crate::brain::redact(model).body;
        let time = now();
        sqlx::query(
      "INSERT INTO route(name,endpoint,model,priority,local,created,updated) VALUES(?,?,?,?,?,?,?) \
       ON CONFLICT(name) DO UPDATE SET endpoint=excluded.endpoint,model=excluded.model,priority=excluded.priority,local=excluded.local,enabled=1,updated=excluded.updated",
    )
    .bind(name)
    .bind(endpoint)
    .bind(&model)
    .bind(priority)
    .bind(local)
    .bind(time)
    .bind(time)
    .execute(&self.pool)
    .await?;
        self.route(name).await?.context("stored route is missing")
    }

    pub async fn route(&self, name: &str) -> Result<Option<Route>> {
        sqlx::query_as(
      "SELECT name,endpoint,model,priority,local,enabled,created,updated FROM route WHERE name=?",
    )
    .bind(name)
    .fetch_optional(&self.pool)
    .await
    .map_err(Into::into)
    }

    pub async fn routes(&self) -> Result<Vec<Route>> {
        sqlx::query_as(
      "SELECT name,endpoint,model,priority,local,enabled,created,updated FROM route ORDER BY priority,name",
    )
    .fetch_all(&self.pool)
    .await
    .map_err(Into::into)
    }

    pub async fn selectroute(&self, local: Option<bool>) -> Result<Option<Route>> {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT name,endpoint,model,priority,local,enabled,created,updated FROM route WHERE enabled=1",
        );
        if let Some(local) = local {
            query.push(" AND local=").push_bind(local);
        }
        query
            .push(" ORDER BY priority,name LIMIT 1")
            .build_query_as::<Route>()
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn callroute(
        &self,
        local: Option<bool>,
        mut body: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let route = self
            .selectroute(local)
            .await?
            .context("no route is available")?;
        anyhow::ensure!(
            !route.endpoint.starts_with("unix://"),
            "Unix routes require a harness adapter"
        );
        let object = body
            .as_object_mut()
            .context("route request must be a JSON object")?;
        if !route.model.is_empty() && !object.contains_key("model") {
            object.insert("model".to_owned(), route.model.clone().into());
        }
        let mut response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .post(&route.endpoint)
            .json(&body)
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "route returned {}",
            response.status()
        );
        let length = response.content_length().unwrap_or(0);
        anyhow::ensure!(length <= 16 * 1024 * 1024, "route response is too large");
        let mut bytes = Vec::with_capacity(length as usize);
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len() + chunk.len() <= 16 * 1024 * 1024,
                "route response is too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).context("route response is not JSON")
    }

    pub async fn evaluate(
        &self,
        runid: &str,
        expected: &str,
        actual: &str,
        score: f64,
        note: &str,
    ) -> Result<Evaluation> {
        anyhow::ensure!(
            (0.0..=1.0).contains(&score),
            "score must be between 0 and 1"
        );
        let id = sqlx::query(
            "INSERT INTO evaluation(runid,expected,actual,score,note,created) VALUES(?,?,?,?,?,?)",
        )
        .bind(runid)
        .bind(crate::brain::redact(expected).body)
        .bind(crate::brain::redact(actual).body)
        .bind(score)
        .bind(crate::brain::redact(note).body)
        .bind(now())
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        sqlx::query_as(
            "SELECT id,runid,expected,actual,score,note,created FROM evaluation WHERE id=?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn evaluations(&self, runid: &str) -> Result<Vec<Evaluation>> {
        sqlx::query_as(
      "SELECT id,runid,expected,actual,score,note,created FROM evaluation WHERE runid=? ORDER BY id",
    )
    .bind(runid)
    .fetch_all(&self.pool)
    .await
    .map_err(Into::into)
    }

    async fn ensureconfig(&self, table: &str, predicate: &str, values: &[&str]) -> Result<()> {
        anyhow::ensure!(
            matches!(table, "capability" | "trigger" | "route"),
            "invalid table"
        );
        let existing = format!("SELECT COUNT(*) FROM {table} WHERE {predicate}");
        let mut query = sqlx::query_scalar::<_, i64>(&existing);
        for value in values {
            query = query.bind(value);
        }
        if query.fetch_one(&self.pool).await? > 0 {
            return Ok(());
        }
        let query = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = sqlx::query_scalar(&query).fetch_one(&self.pool).await?;
        anyhow::ensure!(count < CONFIGLIMIT, "{table} limit reached");
        Ok(())
    }

    async fn pruneruns(&self, time: i64) -> Result<()> {
        sqlx::query(
            "DELETE FROM run WHERE status IN ('succeeded','failed','cancelled') AND updated<?",
        )
        .bind(time - RUNRETENTION)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "DELETE FROM run WHERE id IN (SELECT id FROM run WHERE status IN \
             ('succeeded','failed','cancelled') ORDER BY updated DESC,id DESC LIMIT -1 OFFSET ?)",
        )
        .bind(RUNLIMIT)
        .execute(&self.pool)
        .await?;
        let orphaned: Vec<(String, String)> = sqlx::query_as(
            "SELECT digest,path FROM artifact WHERE NOT EXISTS \
             (SELECT 1 FROM runartifact WHERE runartifact.digest=artifact.digest) AND created<?",
        )
        .bind(time - 86400)
        .fetch_all(&self.pool)
        .await?;
        for (digest, path) in orphaned {
            if std::fs::remove_file(&path).is_ok() {
                sqlx::query("DELETE FROM artifact WHERE digest=?")
                    .bind(digest)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }
}

pub(crate) async fn recordpool(pool: &SqlitePool, mut draft: EventDraft) -> Result<Event> {
    validateword(&draft.source, "event source")?;
    validateword(&draft.kind, "event kind")?;
    anyhow::ensure!(
        draft.message.len() <= MESSAGELIMIT,
        "event message is too large"
    );
    let message = crate::brain::redact(&draft.message).body;
    for value in [
        &mut draft.project,
        &mut draft.agent,
        &mut draft.session,
        &mut draft.runid,
        &mut draft.traceid,
    ] {
        *value = crate::brain::redact(value).body;
    }
    let fields = cleanjson(draft.fields)?;
    anyhow::ensure!(fields.len() <= FIELDSLIMIT, "event fields are too large");
    let created = now();
    let id = sqlx::query(
        "INSERT INTO event(created,source,project,agent,session,level,kind,message,fields,runid,traceid) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(created)
    .bind(&draft.source)
    .bind(&draft.project)
    .bind(&draft.agent)
    .bind(&draft.session)
    .bind(draft.level)
    .bind(&draft.kind)
    .bind(&message)
    .bind(&fields)
    .bind(&draft.runid)
    .bind(&draft.traceid)
    .execute(pool)
    .await?
    .last_insert_rowid();
    pruneevents(pool, created).await?;
    Ok(Event {
        id,
        created,
        source: draft.source,
        project: draft.project,
        agent: draft.agent,
        session: draft.session,
        level: draft.level,
        kind: draft.kind,
        message,
        fields,
        runid: draft.runid,
        traceid: draft.traceid,
    })
}

async fn pruneevents(pool: &SqlitePool, time: i64) -> Result<()> {
    sqlx::query("DELETE FROM event WHERE created < ?")
        .bind(time - EVENTRETENTION)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM event WHERE id <= (SELECT COALESCE(MAX(id),0) - ? FROM event)")
        .bind(EVENTLIMIT)
        .execute(pool)
        .await?;
    Ok(())
}

fn validateword(value: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty() && value.len() <= 128,
        "{label} must be 1 to 128 characters"
    );
    anyhow::ensure!(
        value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".:_-/".contains(character)),
        "{label} contains unsupported characters"
    );
    Ok(())
}

fn cleanjson(mut value: serde_json::Value) -> Result<String> {
    fn clean(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => *text = crate::brain::redact(text).body,
            serde_json::Value::Array(items) => items.iter_mut().for_each(clean),
            serde_json::Value::Object(items) => items.values_mut().for_each(clean),
            _ => {}
        }
    }
    clean(&mut value);
    serde_json::to_string(&value).map_err(Into::into)
}

fn validendpoint(endpoint: &str, local: bool) -> Result<()> {
    anyhow::ensure!(endpoint.len() <= 2048, "route endpoint is too long");
    if local && endpoint.starts_with("unix://") {
        anyhow::ensure!(endpoint.len() > 7, "a Unix route needs a socket path");
        anyhow::ensure!(
            !endpoint.contains(['?', '#']),
            "a Unix route cannot contain query or fragment data"
        );
        return Ok(());
    }
    let url = reqwest::Url::parse(endpoint).context("route endpoint is not a valid URL")?;
    anyhow::ensure!(
        url.username().is_empty() && url.password().is_none(),
        "route credentials must come from the vault or environment"
    );
    anyhow::ensure!(
        url.fragment().is_none(),
        "route endpoint cannot contain a fragment"
    );
    anyhow::ensure!(
        url.query().is_none(),
        "route endpoint cannot contain a query"
    );
    if local {
        anyhow::ensure!(
            url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
            "a local route must use loopback HTTP or a Unix socket"
        );
    } else {
        anyhow::ensure!(url.scheme() == "https", "a remote route must use HTTPS");
    }
    Ok(())
}

fn identifier(prefix: &str) -> String {
    let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
    let seed = format!("{prefix}:{}:{}:{nonce}", std::process::id(), now());
    format!("{prefix}-{}", &hex(&Sha256::digest(seed))[..24])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
