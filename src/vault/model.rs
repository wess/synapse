use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct Vault {
    pub id: i64,
    pub name: String,
    pub created: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct Secret {
    pub id: i64,
    pub vaultid: i64,
    pub vault: String,
    pub name: String,
    pub env: String,
    pub account: String,
    pub global: bool,
    pub created: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeConfig {
    pub version: u32,
    #[serde(default)]
    pub scope: ScopeKind,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub deny: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScopeKind {
    #[default]
    Project,
    Folder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeState {
    pub path: PathBuf,
    pub kind: ScopeKind,
    pub trusted: bool,
    pub changed: bool,
    pub env: Vec<String>,
    pub denied: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    pub env: BTreeMap<String, Secret>,
    pub scopes: Vec<ScopeState>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultStatusRequest {
    /// Folder whose global, project, and nested folder scopes should be resolved.
    pub path: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct VaultStatusResponse {
    pub path: String,
    pub available: Vec<String>,
    pub scopes: Vec<VaultScopeResponse>,
    pub warnings: Vec<String>,
    pub ambient: String,
    pub shell: Option<String>,
    pub note: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct VaultScopeResponse {
    pub path: String,
    pub scope: String,
    pub trusted: bool,
    pub changed: bool,
    pub env: Vec<String>,
    pub denied: Vec<String>,
    pub error: Option<String>,
}

impl From<ScopeState> for VaultScopeResponse {
    fn from(value: ScopeState) -> Self {
        Self {
            path: value.path.display().to_string(),
            scope: match value.kind {
                ScopeKind::Project => "project",
                ScopeKind::Folder => "folder",
            }
            .to_owned(),
            trusted: value.trusted,
            changed: value.changed,
            env: value.env,
            denied: value.denied,
            error: value.error,
        }
    }
}
