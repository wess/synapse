//! Evidence and corrections, collected only when something happens.
//!
//! Reported success stays distinct from an observed check and a user's verdict.
//! Nothing here changes recall authority or starts a model, timer, or worker.

mod approve;
mod check;
mod eval;
mod feedback;
mod model;
mod promotion;
mod resolve;
mod retention;
mod review;
mod target;

pub use approve::Approval;
pub use check::Check;
pub use eval::Regression;
pub use model::{Feedback, Outcome, Record, Target, Verification};
pub use promotion::{Assessment, Promotion, SUPPORT};
pub use resolve::{Disposition, Resolution};
pub use retention::{FEEDBACKLIMIT, RETENTIONDAYS};
pub use review::{Evidence, Review};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn digest(value: &impl serde::Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

pub(crate) fn now() -> Result<i64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64)
}

pub(crate) fn project(folder: Option<&Path>) -> Result<String> {
    folder
        .map(crate::brain::projectkey)
        .transpose()?
        .flatten()
        .context("learning needs an existing project folder")
}

pub(crate) fn clean(value: &str, limit: usize, field: &str) -> Result<String> {
    anyhow::ensure!(value.len() <= limit, "{field} exceeds {limit} bytes");
    Ok(crate::brain::redact(value).body.trim().to_owned())
}
