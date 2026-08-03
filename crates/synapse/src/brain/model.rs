use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone, FromRow, Serialize, JsonSchema, PartialEq, Eq)]
pub struct Memory {
    pub id: i64,
    pub body: String,
    pub source: String,
    pub scope: MemoryScope,
    pub project: String,
    pub created: i64,
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum MemoryScope {
    #[default]
    Global,
    Project,
}

impl MemoryScope {
    pub fn value(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    pub fn project(self, path: Option<&Path>) -> anyhow::Result<String> {
        match self {
            Self::Global => Ok(String::new()),
            Self::Project => path
                .map(crate::brain::projectroot)
                .transpose()?
                .flatten()
                .map(|path| path.display().to_string())
                .ok_or_else(|| anyhow::anyhow!("project-scoped memory needs a project path")),
        }
    }
}

impl FromStr for MemoryScope {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "global" => Ok(Self::Global),
            "project" => Ok(Self::Project),
            _ => anyhow::bail!("expected global or project"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub entries: i64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Optimization {
    Full,
    #[default]
    Balanced,
    Lean,
}

impl Optimization {
    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Balanced => "Balanced",
            Self::Lean => "Lean",
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Balanced => "balanced",
            Self::Lean => "lean",
        }
    }

    pub fn constrained(self, requested: Option<Self>) -> Self {
        match (self, requested.unwrap_or(self)) {
            (Self::Lean, _) | (_, Self::Lean) => Self::Lean,
            (Self::Balanced, _) | (_, Self::Balanced) => Self::Balanced,
            (Self::Full, Self::Full) => Self::Full,
        }
    }
}

impl FromStr for Optimization {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "full" => Ok(Self::Full),
            "balanced" => Ok(Self::Balanced),
            "lean" => Ok(Self::Lean),
            _ => anyhow::bail!("expected full, balanced, or lean"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub optimization: Optimization,
    pub resultlimit: u32,
    pub characterbudget: Option<usize>,
}

impl From<Optimization> for Settings {
    fn from(optimization: Optimization) -> Self {
        match optimization {
            Optimization::Full => Self {
                optimization,
                resultlimit: 25,
                characterbudget: None,
            },
            Optimization::Balanced => Self {
                optimization,
                resultlimit: 8,
                characterbudget: Some(6_000),
            },
            Optimization::Lean => Self {
                optimization,
                resultlimit: 4,
                characterbudget: Some(2_800),
            },
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RememberRequest {
    /// Durable fact, decision, preference, or correction to store.
    pub content: String,
    /// Optional origin such as a project path or topic.
    pub source: Option<String>,
    /// Store this for the current project by default, or globally when it is useful everywhere.
    pub scope: Option<MemoryScope>,
    /// Absolute project root used for project-scoped memory.
    pub project: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RememberResponse {
    pub id: i64,
    pub stored: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecallRequest {
    /// Words or phrase describing the memory needed.
    pub query: String,
    /// Requested match count. The active Synapse optimization setting may lower it.
    pub limit: Option<u32>,
    /// Optional per-call response budget. This can make the configured budget smaller, never larger.
    pub budget: Option<Optimization>,
    /// Absolute project root. Recall includes global memory plus memory for this project.
    pub project: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RecallResponse {
    pub optimization: Optimization,
    pub memories: Vec<Memory>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_budget_can_only_reduce_the_configured_budget() {
        assert_eq!(
            Optimization::Full.constrained(Some(Optimization::Lean)),
            Optimization::Lean
        );
        assert_eq!(
            Optimization::Balanced.constrained(Some(Optimization::Full)),
            Optimization::Balanced
        );
        assert_eq!(
            Optimization::Lean.constrained(Some(Optimization::Full)),
            Optimization::Lean
        );
    }
}
