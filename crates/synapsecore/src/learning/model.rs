use crate::brain::MemoryScope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Target {
    Memory {
        id: i64,
    },
    Skill {
        name: String,
        #[serde(default = "projectscope")]
        scope: MemoryScope,
    },
}

fn projectscope() -> MemoryScope {
    MemoryScope::Project
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Outcome {
    Helpful,
    Failed,
    Unresolved,
}

impl Outcome {
    pub fn value(self) -> &'static str {
        match self {
            Self::Helpful => "helpful",
            Self::Failed => "failed",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Verification {
    Reported,
    Checkpassed,
    Checkfailed,
    Checktimedout,
    Confirmed,
    Rejected,
}

impl Verification {
    pub fn value(self) -> &'static str {
        match self {
            Self::Reported => "reported",
            Self::Checkpassed => "checkpassed",
            Self::Checkfailed => "checkfailed",
            Self::Checktimedout => "checktimedout",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Feedback {
    pub target: Target,
    pub outcome: Outcome,
    /// The mistake or situation this lesson applies to.
    pub problem: String,
    /// A repeatable correction, or empty while the issue is unresolved.
    #[serde(default)]
    pub correction: String,
    /// A short account of the evidence. Never command output or credentials.
    #[serde(default)]
    pub evidence: String,
    /// Optional failed recall question, for a proposed regression case.
    #[serde(default)]
    pub query: String,
    /// Text the corrected recall should contain, for a proposed regression case.
    #[serde(default)]
    pub expected: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema, FromRow)]
pub struct Record {
    pub id: i64,
    pub project: String,
    /// Serialized target; kept stable alongside the content fingerprint.
    pub target: String,
    pub digest: String,
    pub lesson: String,
    /// A one-way session identifier; retries in one session count once.
    pub session: String,
    pub outcome: Outcome,
    pub verification: Verification,
    pub judgment: String,
    pub checkstate: String,
    pub checked: i64,
    pub reviewed: i64,
    pub unstable: bool,
    pub problem: String,
    pub correction: String,
    pub evidence: String,
    pub query: String,
    pub expected: String,
    pub created: i64,
    pub updated: i64,
}

impl Record {
    pub fn target(&self) -> anyhow::Result<Target> {
        Ok(serde_json::from_str(&self.target)?)
    }

    pub fn qualified(&self) -> bool {
        self.outcome == Outcome::Helpful
            && !self.unstable
            && matches!(
                self.verification,
                Verification::Checkpassed | Verification::Confirmed
            )
            && !self.correction.is_empty()
    }
}
