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
    /// The secrets this file asks for, as `vault.name`. They are the references
    /// written in the file itself, so naming them back says nothing that
    /// reading the file would not — which is what makes them safe to report
    /// when they cannot be reached, and everything else on the machine not.
    pub references: Vec<String>,
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
    /// Which store holds the values on this machine: `keychain` or
    /// `encrypted`. Names the store, never what is in it.
    pub backend: String,
    pub available: Vec<String>,
    /// Secrets *this folder asks for* and cannot reach, as `vault.name`.
    ///
    /// Only ones named by a `.synapse.yaml` on the way down to this folder, so
    /// every name here is already written in a file the caller can read. The
    /// rest of the machine is counted by `elsewhere` and never named: an agent
    /// working in one project has no business being handed the names of another
    /// project's credentials, and a name is most of what somebody would need to
    /// know what to go looking for.
    pub unavailable: Vec<String>,
    /// How many other secrets this machine holds that this folder cannot reach.
    ///
    /// A number rather than a list, and it exists for one question: an empty
    /// `available` means either nothing is stored or nothing is approved here,
    /// and those have different fixes. The count tells them apart without
    /// reporting anything about what they are.
    pub elsewhere: usize,
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
