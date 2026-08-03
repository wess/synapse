mod claude;
mod codex;
mod markdown;
mod model;
mod secret;

pub use model::{
    ImportBatch, ImportCandidate, ImportItem, ImportPreview, ImportProvider, ImportReport,
    ImportStatus, ImportSummary,
};

use anyhow::Result;
use std::path::Path;

pub async fn scan(home: &Path, provider: ImportProvider) -> Result<Vec<ImportCandidate>> {
    match provider {
        ImportProvider::Claude => claude::scan(home),
        ImportProvider::Codex => codex::scan(home).await,
        ImportProvider::Markdown => anyhow::bail!("Markdown imports need an explicit path"),
    }
}

pub fn scanmarkdown(path: &Path) -> Result<Vec<ImportCandidate>> {
    markdown::scan(path)
}

pub(crate) use secret::warning;
