use anyhow::{Context, Result};
use std::path::Path;
use tokio::sync::Semaphore;

static WORK: Semaphore = Semaphore::const_new(2);

pub(crate) async fn run<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let permit = WORK
        .try_acquire()
        .context("project knowledge is busy; retry shortly")?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation()
    })
    .await
    .context("project knowledge worker failed")?
}

pub(crate) async fn catalog(project: Option<&Path>) -> Option<super::Catalog> {
    let project = project?.to_owned();
    run(move || super::catalog(&project)).await.ok()
}

#[cfg(test)]
mod tests;
