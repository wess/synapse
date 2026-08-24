//! Background workers: headless agents a supervisor grows its team with.
//!
//! There is no daemon here. The session that spawned a worker supervises it, for
//! as long as that session lives — which is exactly the lifetime a supervisor
//! agent needs, since it spawns workers to do the job it is itself working on.
//! Every worker is recorded in the database, so `synapse relay ps` and the
//! dashboard can see one whose supervisor has since gone, and [`reapstrays`]
//! clears those out on the next start.

use crate::relay::store::WorkerRecord;
use crate::relay::{Mesh, WorkerView, process};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Concurrently running workers one session may own, unless the machine says
/// otherwise. Bounds a supervisor that would otherwise start agents without end.
pub const DEFAULTWORKERS: usize = 8;

/// The most the `maxworkers` setting can ever buy.
///
/// A limit that is only a setting is not a limit: an agent that can reach the
/// settings, or a person who mistypes a zero, would be the whole safety story.
/// Each worker is a real session on a real account, so this is deliberately a
/// number somebody would have to mean rather than one nobody would notice.
pub const WORKERCEILING: usize = 32;

/// Consecutive *rapid* restarts tolerated before a worker is declared broken.
/// A run that lasted clears the count, so a long-lived worker is never retired
/// for restarts accumulated across a whole shift.
const MAXRESTARTS: i64 = 20;

/// How long a run must last to count as healthy. Below this the worker is
/// crash-looping: a bad argv, a missing binary, or an agent exiting after one
/// turn.
const HEALTHY: Duration = Duration::from_secs(60);

const BACKOFF: Duration = Duration::from_secs(3);
const BACKOFFCEILING: Duration = Duration::from_secs(60);

/// How much of a worker's output to keep. A worker writes to its log for as
/// long as it runs and a crash-looping one writes on every attempt, so without
/// a bound the file grows until the disk does not want it. What anyone reads a
/// worker log for is the most recent output, so the tail is what survives.
const MAXLOG: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Spec {
    pub name: String,
    pub role: String,
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub directory: PathBuf,
    pub keepalive: bool,
    /// A fixed Claude Code session id: passed as `--session-id` on the first
    /// run and `--resume` on every respawn, so context survives a crash.
    pub session: Option<String>,
}

/// The workers one session owns.
#[derive(Clone, Default)]
pub struct Supervisor {
    workers: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start `spec` as a monitored background process, returning its log path.
    pub async fn launch(&self, mesh: &Mesh, spec: Spec) -> Result<PathBuf> {
        // The same gate `register` applies, because this name is also a file
        // name. A model picks it, and one containing a separator would put the
        // log somewhere other than the workers folder — and would then be
        // refused when the worker it started tried to join the mesh under it.
        let name = crate::relay::store::validname(&spec.name)?;
        let spec = Spec { name, ..spec };
        let directory = crate::relay::directory()?;
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("could not create {}", directory.display()))?;
        let log = directory.join(format!("{}.log", spec.name));
        // Read per launch rather than held, so raising the limit takes effect on
        // the next spawn instead of the next restart of whatever is supervising.
        let limit = mesh.maxworkers().await.unwrap_or(DEFAULTWORKERS);

        // Checking the cap and reserving the name happen under one lock hold:
        // two concurrent spawns of the same name must not both pass, leaving the
        // second to overwrite the first and orphan its monitor.
        let stop = {
            let mut workers = self.workers.lock().await;
            anyhow::ensure!(
                !workers.contains_key(&spec.name),
                "a worker named `{}` is already running; stop it first",
                spec.name
            );
            anyhow::ensure!(
                workers.len() < limit,
                "the worker limit of {limit} is reached; stop one before starting another"
            );
            let stop = Arc::new(AtomicBool::new(false));
            workers.insert(spec.name.clone(), stop.clone());
            stop
        };

        let recorded = mesh
            .saveworker(&WorkerRecord {
                name: spec.name.clone(),
                role: spec.role.clone(),
                program: spec.program.display().to_string(),
                arguments: spec.arguments.clone(),
                directory: spec.directory.display().to_string(),
                keepalive: spec.keepalive,
                session: spec.session.clone(),
                supervisor: std::process::id(),
                log: log.display().to_string(),
            })
            .await;
        if let Err(error) = recorded {
            // Give the reservation back, or the name stays claimed by a worker
            // that never ran.
            self.workers.lock().await.remove(&spec.name);
            return Err(error);
        }

        tokio::spawn(monitor(mesh.clone(), self.clone(), spec, log.clone(), stop));
        Ok(log)
    }

    /// Stop a worker this session owns and forget it, so no later run brings it
    /// back. Returns whether a worker by that name was running here.
    pub async fn stop(&self, mesh: &Mesh, name: &str) -> Result<bool> {
        let stop = self.workers.lock().await.get(name).cloned();
        match stop {
            Some(stop) => {
                stop.store(true, Ordering::SeqCst);
                if let Some(worker) = mesh.worker(name).await? {
                    process::terminate(worker.process);
                }
                mesh.deleteworker(name).await?;
                Ok(true)
            }
            None => {
                // Not ours. A row left by a session that is gone is still safe
                // to clear; one owned by a live session is left alone.
                let Some(worker) = mesh.worker(name).await? else {
                    return Ok(false);
                };
                anyhow::ensure!(
                    !process::alive(worker.supervisor),
                    "`{name}` belongs to another Synapse session; stop it from there"
                );
                process::terminate(worker.process);
                mesh.deleteworker(name).await?;
                Ok(false)
            }
        }
    }

    /// Stop every worker this session owns. Runs when the session ends, so a
    /// closed supervisor does not leave headless agents behind.
    pub async fn stopall(&self, mesh: &Mesh) {
        let names: Vec<String> = self.workers.lock().await.keys().cloned().collect();
        for name in names {
            let _ = self.stop(mesh, &name).await;
        }
    }

    #[cfg(test)]
    pub async fn owns(&self, name: &str) -> bool {
        self.workers.lock().await.contains_key(name)
    }
}

async fn monitor(
    mesh: Mesh,
    supervisor: Supervisor,
    spec: Spec,
    log: PathBuf,
    stop: Arc<AtomicBool>,
) {
    let mut restarts = 0_i64;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        trim(&log);
        let Ok(output) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
        else {
            let _ = mesh
                .workerstate(&spec.name, "could not open the worker log", 0, restarts)
                .await;
            break;
        };
        let Ok(errors) = output.try_clone() else {
            let _ = mesh
                .workerstate(&spec.name, "could not open the worker log", 0, restarts)
                .await;
            break;
        };

        let mut command = tokio::process::Command::new(&spec.program);
        command.args(&spec.arguments);
        for (key, value) in &spec.environment {
            command.env(key, value);
        }
        // A resumable worker fixes its session on the first attempt, then
        // resumes it on every respawn so its context survives the crash.
        if let Some(session) = &spec.session {
            if restarts > 0 {
                command.arg("--resume").arg(session);
            } else {
                command.arg("--session-id").arg(session);
            }
        }
        command
            .current_dir(&spec.directory)
            .stdin(Stdio::null())
            .stdout(Stdio::from(output))
            .stderr(Stdio::from(errors));

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = mesh
                    .workerstate(
                        &spec.name,
                        &format!("could not start {}: {error}", spec.program.display()),
                        0,
                        restarts,
                    )
                    .await;
                break;
            }
        };
        let id = child.id().unwrap_or(0);
        let _ = mesh.workerstate(&spec.name, "running", id, restarts).await;

        let started = Instant::now();
        let status = child.wait().await;
        let uptime = started.elapsed();

        if stop.load(Ordering::SeqCst) {
            break;
        }
        let code = status.ok().and_then(|status| status.code()).unwrap_or(-1);
        let _ = mesh
            .workerstate(&spec.name, &format!("exited ({code})"), 0, restarts)
            .await;

        if !spec.keepalive {
            break;
        }
        // A run that lasted proves the worker basically works, so its budget
        // resets. Only consecutive rapid exits retire one.
        restarts = if uptime >= HEALTHY { 0 } else { restarts + 1 };
        if restarts > MAXRESTARTS {
            let _ = mesh
                .workerstate(
                    &spec.name,
                    &format!("gave up after {MAXRESTARTS} restarts without a healthy run"),
                    0,
                    restarts,
                )
                .await;
            break;
        }
        tokio::time::sleep(backoff(restarts)).await;
    }
    supervisor.workers.lock().await.remove(&spec.name);
    let _ = mesh.deleteworker(&spec.name).await;
}

/// Keep a worker log under [`MAXLOG`] by dropping the oldest half, between runs
/// so nothing is holding the file open. Failing to trim is not worth stopping a
/// worker for; the log simply stays long.
fn trim(log: &std::path::Path) {
    let Ok(metadata) = std::fs::metadata(log) else {
        return;
    };
    if metadata.len() <= MAXLOG {
        return;
    }
    let Ok(content) = std::fs::read(log) else {
        return;
    };
    let keep = content.len() - (MAXLOG / 2) as usize;
    // Resume at a line boundary, so the log does not open mid-sentence.
    let from = content[keep..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(keep, |at| keep + at + 1);
    let _ = std::fs::write(log, &content[from..]);
}

/// Delay before restart `n`: [`BACKOFF`] doubling per consecutive rapid failure,
/// capped at [`BACKOFFCEILING`]. A worker that had been healthy restarts at the
/// base delay.
fn backoff(n: i64) -> Duration {
    let shift = n.saturating_sub(1).clamp(0, 16) as u32;
    BACKOFF.saturating_mul(1_u32 << shift).min(BACKOFFCEILING)
}

/// Clear workers whose supervising session is gone, stopping any child of theirs
/// that is somehow still running. A worker owned by a live session is untouched.
pub async fn reapstrays(mesh: &Mesh) -> Result<Vec<WorkerView>> {
    let mut reaped = Vec::new();
    for worker in mesh.workers().await? {
        if process::alive(worker.supervisor) {
            continue;
        }
        process::terminate(worker.process);
        mesh.deleteworker(&worker.name).await?;
        reaped.push(worker);
    }
    Ok(reaped)
}

#[cfg(test)]
#[allow(
    clippy::await_holding_lock,
    reason = "the guard serialises tests over one process-wide SYNAPSE_DATA; \
              holding it across the await is the point"
)]
mod tests {
    use super::*;

    /// A mesh whose worker logs are written inside `directory`, held for as long
    /// as the returned guard lives.
    async fn mesh() -> (Mesh, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let mesh = Mesh::open(directory.path().join("brain.db")).await.unwrap();
        (mesh, directory)
    }

    /// Worker logs land under the data directory, so a test that starts one
    /// redirects `SYNAPSE_DATA` and holds the shared lock while it does.
    fn logsinto(directory: &tempfile::TempDir) -> std::sync::MutexGuard<'static, ()> {
        crate::files::scopeddata(directory.path())
    }

    fn spec(name: &str) -> Spec {
        Spec {
            name: name.to_owned(),
            role: "worker".to_owned(),
            program: PathBuf::from("/usr/bin/true"),
            arguments: Vec::new(),
            environment: Vec::new(),
            directory: std::env::temp_dir(),
            keepalive: false,
            session: None,
        }
    }

    #[test]
    fn the_restart_delay_grows_and_is_capped() {
        assert_eq!(backoff(0), BACKOFF, "a healthy run restarts promptly");
        assert_eq!(backoff(1), BACKOFF);
        assert_eq!(backoff(2), BACKOFF * 2);
        assert_eq!(backoff(3), BACKOFF * 4);
        assert_eq!(backoff(99), BACKOFFCEILING);
        for n in 0..64 {
            assert!(backoff(n) <= BACKOFFCEILING, "n={n} ran past the ceiling");
        }
    }

    #[test]
    fn a_full_restart_budget_spans_more_than_a_minute() {
        let total: Duration = (1..=MAXRESTARTS).map(backoff).sum();
        assert!(
            total >= Duration::from_secs(600),
            "a crash-looping worker should get many minutes before it is retired, got {total:?}"
        );
    }

    #[tokio::test]
    async fn a_duplicate_name_is_refused_rather_than_overwriting_the_first() {
        let (mesh, directory) = mesh().await;
        let _logs = logsinto(&directory);
        let supervisor = Supervisor::new();
        supervisor.launch(&mesh, spec("backend")).await.unwrap();

        let error = supervisor
            .launch(&mesh, spec("backend"))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("already running"), "got {error}");
    }

    #[tokio::test]
    async fn the_worker_limit_is_enforced() {
        let (mesh, directory) = mesh().await;
        let _logs = logsinto(&directory);
        let supervisor = Supervisor::new();
        {
            let mut workers = supervisor.workers.lock().await;
            for index in 0..DEFAULTWORKERS {
                workers.insert(format!("w{index}"), Arc::new(AtomicBool::new(false)));
            }
        }

        let error = supervisor
            .launch(&mesh, spec("overflow"))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("worker limit"), "got {error}");
        assert!(!supervisor.owns("overflow").await);
    }

    #[tokio::test]
    async fn a_name_that_would_escape_the_workers_folder_is_refused() {
        let (mesh, directory) = mesh().await;
        let _logs = logsinto(&directory);
        let supervisor = Supervisor::new();

        for bad in ["../escape", "with space", "a/b", ""] {
            let error = supervisor
                .launch(
                    &mesh,
                    Spec {
                        name: bad.to_owned(),
                        ..spec("placeholder")
                    },
                )
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains("name"), "`{bad}` gave {error}");
        }
        // And nothing was written outside the workers folder on the way.
        assert!(
            !crate::relay::directory()
                .unwrap()
                .join("../escape.log")
                .exists()
        );
    }

    #[test]
    fn a_long_worker_log_is_trimmed_to_its_most_recent_output() {
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("noisy.log");
        let mut content = "old line\n".repeat((MAXLOG / 9) as usize + 1);
        content.push_str("the newest line\n");
        std::fs::write(&log, &content).unwrap();
        assert!(std::fs::metadata(&log).unwrap().len() > MAXLOG);

        trim(&log);

        let kept = std::fs::read_to_string(&log).unwrap();
        assert!(std::fs::metadata(&log).unwrap().len() <= MAXLOG / 2);
        assert!(kept.ends_with("the newest line\n"), "the tail must survive");
        assert!(
            kept.starts_with("old line\n"),
            "must resume at a line break"
        );
    }

    #[test]
    fn a_log_that_fits_is_left_exactly_as_it_was() {
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("quiet.log");
        std::fs::write(&log, "one line\n").unwrap();

        trim(&log);

        assert_eq!(std::fs::read_to_string(&log).unwrap(), "one line\n");
    }

    #[tokio::test]
    async fn a_worker_left_by_a_dead_session_is_reaped_and_a_live_one_is_left_alone() {
        let (mesh, _directory) = mesh().await;
        let record = |name: &str, supervisor: u32| WorkerRecord {
            name: name.to_owned(),
            role: "worker".to_owned(),
            program: "/usr/bin/true".to_owned(),
            arguments: Vec::new(),
            directory: "/tmp".to_owned(),
            keepalive: true,
            session: None,
            supervisor,
            log: String::new(),
        };
        // Process id 0 is never a live process, so it stands in for a session
        // that is gone.
        mesh.saveworker(&record("orphan", 0)).await.unwrap();
        mesh.saveworker(&record("owned", std::process::id()))
            .await
            .unwrap();

        let reaped = reapstrays(&mesh).await.unwrap();

        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].name, "orphan");
        let remaining = mesh.workers().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "owned");
    }
}
