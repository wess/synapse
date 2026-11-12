use super::store::{identifier, now};
use super::{Backend, Checkpoint, Run, Step, Workspace};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};

const FILELIMIT: usize = 1_000;
const FILESIZELIMIT: u64 = 32 * 1024 * 1024;
const CHECKPOINTLIMIT: u64 = 128 * 1024 * 1024;
const STATELIMIT: usize = 2 * 1024 * 1024;
const PATCHLIMIT: usize = 64 * 1024 * 1024;
const GITTIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Deserialize, Serialize)]
struct Manifest {
    files: Vec<ManifestFile>,
}

#[derive(Deserialize, Serialize)]
struct ManifestFile {
    path: String,
    digest: String,
    absent: bool,
}

#[derive(Deserialize, Serialize)]
struct State {
    run: Run,
    steps: Vec<Step>,
    client: serde_json::Value,
}

impl Backend {
    pub async fn checkpoint(
        &self,
        runid: &str,
        root: &Path,
        paths: &[PathBuf],
        label: &str,
        client: serde_json::Value,
    ) -> Result<Checkpoint> {
        anyhow::ensure!(!paths.is_empty(), "a checkpoint needs at least one path");
        anyhow::ensure!(
            paths.len() <= FILELIMIT,
            "a checkpoint is limited to {FILELIMIT} files"
        );
        let root = canonicalroot(root)?;
        let client = cleanvalue(client);
        let mut seen = std::collections::HashSet::new();
        let mut total = 0u64;
        let run = self
            .run(runid)
            .await?
            .with_context(|| format!("run `{runid}` does not exist"))?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runcheckpoint WHERE runid=?")
            .bind(runid)
            .fetch_one(&self.pool)
            .await?;
        anyhow::ensure!(count < 1_000, "checkpoint limit reached for this run");
        let state = serde_json::to_string(&State {
            run,
            steps: self.steps(runid).await?,
            client,
        })?;
        anyhow::ensure!(state.len() <= STATELIMIT, "checkpoint state is too large");
        let mut files = Vec::with_capacity(paths.len());
        for relative in paths {
            validrelative(relative)?;
            sensitive(relative)?;
            anyhow::ensure!(
                seen.insert(relative.clone()),
                "checkpoint path is repeated: {}",
                relative.display()
            );
            let path = root.join(relative);
            if !path.exists() {
                files.push(ManifestFile {
                    path: relative.display().to_string(),
                    digest: String::new(),
                    absent: true,
                });
                continue;
            }
            anyhow::ensure!(
                path.is_file(),
                "{} is not a regular file",
                relative.display()
            );
            let real = path
                .canonicalize()
                .with_context(|| format!("could not resolve {}", path.display()))?;
            anyhow::ensure!(real.starts_with(&root), "checkpoint path leaves its root");
            let size = real.metadata()?.len();
            anyhow::ensure!(
                size <= FILESIZELIMIT,
                "{} is larger than 32 MiB",
                relative.display()
            );
            total = total.saturating_add(size);
            anyhow::ensure!(
                total <= CHECKPOINTLIMIT,
                "checkpoint content is larger than 128 MiB"
            );
            let content = std::fs::read(&real)
                .with_context(|| format!("could not read {}", relative.display()))?;
            if let Ok(text) = std::str::from_utf8(&content) {
                anyhow::ensure!(
                    crate::brain::redact(text).body == text,
                    "{} appears to contain a credential or private block",
                    relative.display()
                );
            }
            let artifact = self
                .putartifact(
                    &relative.display().to_string(),
                    "checkpointfile",
                    "application/octet-stream",
                    &content,
                )
                .await?;
            self.attach(runid, &artifact.digest, "checkpointfile")
                .await?;
            files.push(ManifestFile {
                path: relative.display().to_string(),
                digest: artifact.digest,
                absent: false,
            });
        }
        let manifest = self
            .putartifact(
                "checkpoint.json",
                "checkpoint",
                "application/json",
                &serde_json::to_vec(&Manifest { files })?,
            )
            .await?;
        self.attach(runid, &manifest.digest, "checkpoint").await?;
        let parent: String = sqlx::query_scalar(
            "SELECT id FROM runcheckpoint WHERE runid=? ORDER BY created DESC,id DESC LIMIT 1",
        )
        .bind(runid)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or_default();
        let id = identifier("checkpoint");
        sqlx::query(
            "INSERT INTO runcheckpoint(id,runid,parent,label,root,manifest,state,created) \
             VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(&id)
        .bind(runid)
        .bind(parent)
        .bind(crate::brain::redact(label).body)
        .bind(root.display().to_string())
        .bind(&manifest.digest)
        .bind(state)
        .bind(now())
        .execute(&self.pool)
        .await?;
        self.checkpointbyid(&id)
            .await?
            .context("created checkpoint is missing")
    }

    pub async fn restorecheckpoint(&self, id: &str) -> Result<Checkpoint> {
        let checkpoint = self
            .checkpointbyid(id)
            .await?
            .with_context(|| format!("checkpoint `{id}` does not exist"))?;
        let root = canonicalroot(Path::new(&checkpoint.root))?;
        let manifest: Manifest =
            serde_json::from_slice(&self.readartifact(&checkpoint.manifest).await?)
                .context("checkpoint manifest is invalid")?;
        anyhow::ensure!(
            manifest.files.len() <= FILELIMIT,
            "checkpoint manifest has too many files"
        );
        for file in manifest.files {
            let relative = PathBuf::from(&file.path);
            validrelative(&relative)?;
            sensitive(&relative)?;
            let path = safeoutputpath(&root, &relative)?;
            if file.absent {
                if path.is_file() {
                    std::fs::remove_file(&path)
                        .with_context(|| format!("could not remove {}", relative.display()))?;
                }
                continue;
            }
            let content = self.readartifact(&file.digest).await?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::files::atomicreplace(&path, &content, None)
                .with_context(|| format!("could not restore {}", relative.display()))?;
        }
        let state: State =
            serde_json::from_str(&checkpoint.state).context("checkpoint state is invalid")?;
        anyhow::ensure!(
            state.run.id == checkpoint.runid
                && state
                    .steps
                    .iter()
                    .all(|step| step.runid == checkpoint.runid),
            "checkpoint state belongs to another run"
        );
        self.restorestate(&state).await?;
        sqlx::query("UPDATE runcheckpoint SET restored=? WHERE id=?")
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        self.checkpointbyid(id)
            .await?
            .context("restored checkpoint is missing")
    }

    async fn restorestate(&self, state: &State) -> Result<()> {
        sqlx::query(
            "UPDATE run SET status=?,output=?,error=?,leaseowner='',leaseuntil=0,updated=? WHERE id=?",
        )
        .bind(state.run.status)
        .bind(&state.run.output)
        .bind(&state.run.error)
        .bind(now())
        .bind(&state.run.id)
        .execute(&self.pool)
        .await?;
        for step in &state.steps {
            sqlx::query(
                "UPDATE runstep SET status=?,output=?,error=?,started=?,ended=? WHERE id=? AND runid=?",
            )
            .bind(step.status)
            .bind(&step.output)
            .bind(&step.error)
            .bind(step.started)
            .bind(step.ended)
            .bind(step.id)
            .bind(&step.runid)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn checkpoints(&self, runid: &str) -> Result<Vec<Checkpoint>> {
        sqlx::query_as(
            "SELECT id,runid,parent,label,root,manifest,state,restored,created FROM runcheckpoint \
             WHERE runid=? ORDER BY created,id",
        )
        .bind(runid)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn recentcheckpoints(&self, runid: &str, limit: i64) -> Result<Vec<Checkpoint>> {
        sqlx::query_as(
            "SELECT id,runid,parent,label,root,manifest,state,restored,created FROM runcheckpoint \
             WHERE runid=? ORDER BY created DESC,id DESC LIMIT ?",
        )
        .bind(runid)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn checkpointbyid(&self, id: &str) -> Result<Option<Checkpoint>> {
        sqlx::query_as(
            "SELECT id,runid,parent,label,root,manifest,state,restored,created FROM runcheckpoint \
             WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn createworkspace(
        &self,
        runid: &str,
        root: &Path,
        base: Option<&str>,
    ) -> Result<Workspace> {
        anyhow::ensure!(
            self.workspace(runid).await?.is_none(),
            "run already has a workspace"
        );
        anyhow::ensure!(
            self.run(runid).await?.is_some(),
            "run `{runid}` does not exist"
        );
        let root = gitroot(root)?;
        let base = base.unwrap_or("HEAD");
        anyhow::ensure!(
            !base.starts_with('-') && base.len() <= 256,
            "workspace base is invalid"
        );
        let resolved = git(&root, &["rev-parse", "--verify", base])?;
        let id = identifier("workspace");
        let path = self
            .artifacts
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("workspaces")
            .join(&id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            crate::database::securedirectory(parent)?;
        }
        let time = now();
        sqlx::query(
            "INSERT INTO runworkspace(id,runid,root,path,base,status,created,updated) \
             VALUES(?,?,?,?,?,'creating',?,?)",
        )
        .bind(&id)
        .bind(runid)
        .bind(root.display().to_string())
        .bind(path.display().to_string())
        .bind(resolved.trim())
        .bind(time)
        .bind(time)
        .execute(&self.pool)
        .await?;
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["worktree", "add", "--detach"])
            .arg(&path)
            .arg(resolved.trim())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("could not start git worktree")?;
        let stderr = child
            .stderr
            .take()
            .context("git worktree stderr is unavailable")?;
        let errors = std::thread::spawn(move || {
            let mut content = Vec::new();
            let _ = stderr.take(64 * 1024 + 1).read_to_end(&mut content);
            content
        });
        let result = waitchild(&mut child, "git worktree")?;
        let errors = errors.join().unwrap_or_default();
        let status = if result.success() { "ready" } else { "failed" };
        sqlx::query("UPDATE runworkspace SET status=?,updated=? WHERE id=?")
            .bind(status)
            .bind(now())
            .bind(&id)
            .execute(&self.pool)
            .await?;
        anyhow::ensure!(
            result.success(),
            "could not create worktree: {}",
            String::from_utf8_lossy(&errors).trim()
        );
        self.workspace(runid)
            .await?
            .context("created workspace is missing")
    }

    pub async fn workspace(&self, runid: &str) -> Result<Option<Workspace>> {
        sqlx::query_as(
            "SELECT id,runid,root,path,branch,base,status,created,updated FROM runworkspace \
             WHERE runid=?",
        )
        .bind(runid)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn mergeworkspace(&self, runid: &str) -> Result<Workspace> {
        let workspace = self
            .workspace(runid)
            .await?
            .with_context(|| format!("run `{runid}` has no workspace"))?;
        anyhow::ensure!(
            workspace.status == "ready",
            "workspace is not ready to merge"
        );
        let root = canonicalroot(Path::new(&workspace.root))?;
        let checkout = canonicalroot(Path::new(&workspace.path))?;
        git(&checkout, &["add", "-N", "."])?;
        let names = gitbytes(
            &checkout,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--name-only",
                "-z",
                "HEAD",
            ],
            1024 * 1024,
        )?;
        let paths = names
            .split(|byte| *byte == 0)
            .filter(|value| !value.is_empty())
            .map(|value| {
                std::str::from_utf8(value)
                    .map(PathBuf::from)
                    .context("git returned a non-UTF-8 path")
            })
            .collect::<Result<Vec<_>>>()?;
        anyhow::ensure!(!paths.is_empty(), "workspace has no changes to merge");
        anyhow::ensure!(paths.len() <= FILELIMIT, "workspace changes too many files");
        let mut total = 0u64;
        for path in &paths {
            validrelative(path)?;
            sensitive(path)?;
            let candidate = checkout.join(path);
            if candidate.is_file() {
                total = total.saturating_add(candidate.metadata()?.len());
                anyhow::ensure!(
                    total <= CHECKPOINTLIMIT,
                    "workspace changes are larger than 128 MiB"
                );
            }
        }
        let existed = paths
            .iter()
            .map(|path| root.join(path).exists())
            .collect::<Vec<_>>();
        let patch = gitbytes(
            &checkout,
            &["diff", "--binary", "--no-ext-diff", "--no-textconv", "HEAD"],
            PATCHLIMIT,
        )?;
        anyhow::ensure!(!patch.is_empty(), "workspace has no patch to merge");
        applypatch(&root, &patch, true)?;
        self.checkpoint(
            runid,
            &root,
            &paths,
            "before workspace merge",
            serde_json::json!({"workspace": workspace.id}),
        )
        .await?;
        sqlx::query("UPDATE runworkspace SET status='merging',updated=? WHERE id=?")
            .bind(now())
            .bind(&workspace.id)
            .execute(&self.pool)
            .await?;
        if let Err(error) = applypatch(&root, &patch, false) {
            sqlx::query("UPDATE runworkspace SET status='ready',updated=? WHERE id=?")
                .bind(now())
                .bind(&workspace.id)
                .execute(&self.pool)
                .await?;
            return Err(error);
        }
        for (path, existed) in paths.iter().zip(existed) {
            let operation = match (existed, root.join(path).exists()) {
                (false, true) => "create",
                (true, false) => "delete",
                _ => "modify",
            };
            self.recordchange(
                runid,
                None,
                &path.display().to_string(),
                operation,
                "",
                "",
                "",
                "Merged from the isolated workspace",
            )
            .await?;
        }
        sqlx::query("UPDATE runworkspace SET status='merged',updated=? WHERE id=?")
            .bind(now())
            .bind(&workspace.id)
            .execute(&self.pool)
            .await?;
        let _ = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["worktree", "remove", "--force"])
            .arg(&checkout)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        self.workspace(runid)
            .await?
            .context("merged workspace is missing")
    }
}

fn canonicalroot(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .with_context(|| format!("could not resolve {}", root.display()))
}

fn gitroot(root: &Path) -> Result<PathBuf> {
    let root = canonicalroot(root)?;
    let output = git(&root, &["rev-parse", "--show-toplevel"])?;
    canonicalroot(Path::new(output.trim()))
}

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    String::from_utf8(gitbytes(root, arguments, 64 * 1024)?).context("git returned invalid text")
}

fn gitbytes(root: &Path, arguments: &[&str], limit: usize) -> Result<Vec<u8>> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start git")?;
    let stdout = child.stdout.take().context("git stdout is unavailable")?;
    let stderr = child.stderr.take().context("git stderr is unavailable")?;
    let output = std::thread::spawn(move || {
        let mut content = Vec::with_capacity(limit.min(1024 * 1024));
        let _ = stdout.take(limit as u64 + 1).read_to_end(&mut content);
        content
    });
    let errors = std::thread::spawn(move || {
        let mut content = Vec::new();
        let _ = stderr.take(64 * 1024 + 1).read_to_end(&mut content);
        content
    });
    let status = waitchild(&mut child, "git")?;
    let output = output.join().unwrap_or_default();
    let errors = errors.join().unwrap_or_default();
    anyhow::ensure!(output.len() <= limit, "git output is too large");
    anyhow::ensure!(
        status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&errors).trim()
    );
    Ok(output)
}

fn applypatch(root: &Path, patch: &[u8], check: bool) -> Result<()> {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(["apply", "--binary"]);
    if check {
        command.arg("--check");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start git apply")?;
    let stderr = child
        .stderr
        .take()
        .context("git apply stderr is unavailable")?;
    let errors = std::thread::spawn(move || {
        let mut content = Vec::new();
        let _ = stderr.take(64 * 1024 + 1).read_to_end(&mut content);
        content
    });
    let mut stdin = child
        .stdin
        .take()
        .context("git apply stdin is unavailable")?;
    let status = std::thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(patch));
        let status = waitchild(&mut child, "git apply");
        let written = writer
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("git apply input writer panicked")));
        status.and_then(|status| {
            written.context("could not write the workspace patch")?;
            Ok(status)
        })
    })?;
    let errors = errors.join().unwrap_or_default();
    anyhow::ensure!(
        status.success(),
        "workspace changes do not apply cleanly: {}",
        String::from_utf8_lossy(&errors).trim()
    );
    Ok(())
}

fn waitchild(child: &mut Child, label: &str) -> Result<ExitStatus> {
    let deadline = std::time::Instant::now() + GITTIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("{label} timed out after 120 seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn validrelative(path: &Path) -> Result<()> {
    anyhow::ensure!(!path.as_os_str().is_empty(), "checkpoint path is empty");
    anyhow::ensure!(!path.is_absolute(), "checkpoint paths must be relative");
    anyhow::ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "checkpoint path contains unsupported components"
    );
    let text = path.display().to_string();
    anyhow::ensure!(
        crate::brain::redact(&text).body == text,
        "checkpoint path appears to contain a credential"
    );
    Ok(())
}

fn cleanvalue(mut value: serde_json::Value) -> serde_json::Value {
    fn clean(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => *text = crate::brain::redact(text).body,
            serde_json::Value::Array(items) => items.iter_mut().for_each(clean),
            serde_json::Value::Object(items) => {
                let original = std::mem::take(items);
                for (key, mut value) in original {
                    clean(&mut value);
                    items.insert(crate::brain::redact(&key).body, value);
                }
            }
            _ => {}
        }
    }
    clean(&mut value);
    value
}

fn safeoutputpath(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut parent = root.to_path_buf();
    if let Some(components) = relative.parent() {
        for component in components.components() {
            let Component::Normal(component) = component else {
                anyhow::bail!("checkpoint path contains unsupported components");
            };
            parent.push(component);
            if parent.exists() {
                let metadata = std::fs::symlink_metadata(&parent)?;
                anyhow::ensure!(
                    !metadata.file_type().is_symlink() && metadata.is_dir(),
                    "checkpoint output passes through a link or non-directory"
                );
            }
        }
    }
    Ok(root.join(relative))
}

fn sensitive(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let blocked = name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name.as_str(),
            "credentials" | "credentials.json" | "id_rsa" | "id_ed25519"
        )
        || matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("pem" | "key" | "p12")
        );
    anyhow::ensure!(!blocked, "{} is credential-shaped", path.display());
    Ok(())
}
