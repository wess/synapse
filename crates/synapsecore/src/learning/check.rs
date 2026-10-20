use super::{Record, Verification, digest};
use crate::brain::Brain;
use anyhow::{Context, Result};
use serde::Serialize;
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct Check {
    pub feedback: Record,
    pub exit: Option<i32>,
    pub timedout: bool,
}

fn version(record: &Record) -> Result<String> {
    digest(&(
        &record.digest,
        &record.lesson,
        record.outcome,
        &record.evidence,
    ))
}

impl Brain {
    /// An explicit check observes the exit status itself. Output and arguments
    /// never enter the ledger, and no check is launched by recall or a timer.
    pub async fn checkfeedback(
        &self,
        id: i64,
        folder: &Path,
        command: &[OsString],
        timeout: Duration,
    ) -> Result<Check> {
        anyhow::ensure!(self.learn().await?, "self-improvement is off");
        anyhow::ensure!(!command.is_empty(), "a check needs a program");
        anyhow::ensure!(
            !timeout.is_zero() && timeout <= Duration::from_secs(300),
            "checks must finish within 300 seconds"
        );
        let record = self.feedbackrecord(id, folder).await?;
        anyhow::ensure!(
            record.target()?.fingerprint(self, folder).await? == record.digest,
            "the target changed; collect fresh evidence"
        );
        let mut process = tokio::process::Command::new(&command[0]);
        process
            .args(&command[1..])
            .current_dir(folder)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        process.process_group(0);
        let mut child = process
            .spawn()
            .context("could not start the verification check")?;
        let pid = child.id();
        let status = tokio::time::timeout(timeout, child.wait()).await;
        let (exit, timedout, passed) = match status {
            Ok(status) => {
                let status = status.context("could not wait for the verification check")?;
                (status.code(), false, status.success())
            }
            Err(_) => {
                stop(pid).await;
                let _ = child.kill().await;
                let _ = child.wait().await;
                (None, true, false)
            }
        };
        anyhow::ensure!(
            version(&record)? == version(&self.feedbackrecord(id, folder).await?)?,
            "feedback changed while the check ran; collect fresh evidence"
        );
        let feedback = self
            .verifyfeedback(
                id,
                folder,
                if passed {
                    Verification::Checkpassed
                } else if timedout {
                    Verification::Checktimedout
                } else {
                    Verification::Checkfailed
                },
                Some(&record),
            )
            .await?;
        Ok(Check {
            feedback,
            exit,
            timedout,
        })
    }
}

#[cfg(unix)]
async fn stop(pid: Option<u32>) {
    if let Some(pid) = pid {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
async fn stop(pid: Option<u32>) {
    if let Some(pid) = pid {
        let mut command = tokio::process::Command::new("taskkill.exe");
        command
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), command.status()).await;
    }
}
