use crate::brain::MemoryScope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportProvider {
    Claude,
    Codex,
    Markdown,
}

impl ImportProvider {
    pub fn value(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Markdown => "markdown",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Markdown => "Markdown",
        }
    }
}

impl Display for ImportProvider {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.value())
    }
}

impl FromStr for ImportProvider {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "markdown" => Ok(Self::Markdown),
            _ => anyhow::bail!("expected claude, codex, or markdown"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImportCandidate {
    pub provider: ImportProvider,
    pub externalid: String,
    pub body: String,
    pub source: String,
    pub scope: MemoryScope,
    pub project: String,
    pub path: PathBuf,
    pub created: i64,
    pub warning: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportStatus {
    Ready,
    Existing,
    Flagged,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ImportItem {
    pub provider: ImportProvider,
    pub externalid: String,
    pub preview: String,
    pub source: String,
    pub scope: MemoryScope,
    pub project: String,
    pub path: String,
    pub status: ImportStatus,
    pub warning: Option<String>,
    #[serde(skip)]
    pub(crate) candidate: ImportCandidate,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ImportPreview {
    pub provider: ImportProvider,
    pub found: usize,
    pub ready: usize,
    pub existing: usize,
    pub flagged: usize,
    pub items: Vec<ImportItem>,
}

impl ImportPreview {
    pub fn summary(&self) -> ImportSummary {
        ImportSummary {
            provider: self.provider,
            found: self.found,
            ready: self.ready,
            existing: self.existing,
            flagged: self.flagged,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ImportSummary {
    pub provider: ImportProvider,
    pub found: usize,
    pub ready: usize,
    pub existing: usize,
    pub flagged: usize,
    pub error: Option<String>,
}

impl ImportSummary {
    pub fn error(provider: ImportProvider, error: impl ToString) -> Self {
        Self {
            provider,
            found: 0,
            ready: 0,
            existing: 0,
            flagged: 0,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Clone, Debug, FromRow, Serialize, JsonSchema)]
pub struct ImportBatch {
    pub id: i64,
    pub provider: String,
    pub created: i64,
    pub imported: i64,
    pub linked: i64,
    pub skipped: i64,
    pub flagged: i64,
    pub undone: bool,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ImportReport {
    pub batch: ImportBatch,
    pub stored: i64,
    pub linked: i64,
    pub skipped: i64,
    pub flagged: i64,
}
