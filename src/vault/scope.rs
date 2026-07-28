use crate::vault::{Resolved, ScopeConfig, ScopeKind, ScopeState, VaultStore};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG: &str = ".synapse.yaml";

pub fn template() -> &'static str {
    templatefor(ScopeKind::Project)
}

pub fn templatefor(kind: ScopeKind) -> &'static str {
    match kind {
        ScopeKind::Project => "version: 1\nscope: project\nenv: {}\ndeny: []\n",
        ScopeKind::Folder => "version: 1\nscope: folder\nenv: {}\ndeny: []\n",
    }
}

pub fn discover(folder: &Path) -> Result<Vec<PathBuf>> {
    let folder = folder
        .canonicalize()
        .with_context(|| format!("could not resolve {}", folder.display()))?;
    let mut ancestors = folder.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    Ok(ancestors
        .into_iter()
        .map(|path| path.join(CONFIG))
        .filter(|path| path.is_file())
        .collect())
}

pub fn read(path: &Path) -> Result<(ScopeConfig, String)> {
    let content =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let config: ScopeConfig = serde_saphyr::from_str(&content)
        .with_context(|| format!("{} is not valid Synapse YAML", path.display()))?;
    anyhow::ensure!(config.version == 1, "unsupported Synapse scope version");
    let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
    Ok((config, digest))
}

pub async fn resolve(store: &VaultStore, folder: &Path) -> Result<Resolved> {
    let mut resolved = Resolved::default();
    let mut denied = BTreeSet::new();
    for secret in store.globalsecrets().await? {
        resolved.env.insert(secret.env.clone(), secret);
    }

    for path in discover(folder)? {
        let state = match read(&path) {
            Ok((config, digest)) => {
                let approved = store.digest(&path).await?;
                let trusted = approved.as_deref() == Some(digest.as_str());
                let changed = approved.is_some() && !trusted;
                let mut state = ScopeState {
                    path: path.clone(),
                    kind: config.scope,
                    trusted,
                    changed,
                    env: config.env.keys().cloned().collect(),
                    denied: config.deny.iter().cloned().collect(),
                    error: None,
                };
                if trusted {
                    apply(store, &config.env, &config.deny, &mut denied, &mut resolved).await;
                } else {
                    state.error = Some(if changed {
                        "Scope changed since approval".to_owned()
                    } else {
                        "Scope has not been approved".to_owned()
                    });
                }
                state
            }
            Err(error) => ScopeState {
                path: path.clone(),
                kind: Default::default(),
                trusted: false,
                changed: false,
                env: Vec::new(),
                denied: Vec::new(),
                error: Some(error.to_string()),
            },
        };
        if let Some(error) = state.error.as_ref() {
            resolved
                .warnings
                .push(format!("{}: {error}", path.display()));
        }
        resolved.scopes.push(state);
    }
    Ok(resolved)
}

async fn apply(
    store: &VaultStore,
    references: &BTreeMap<String, String>,
    block: &BTreeSet<String>,
    denied: &mut BTreeSet<String>,
    resolved: &mut Resolved,
) {
    for env in block {
        denied.insert(env.clone());
        resolved.env.remove(env);
    }
    for (env, reference) in references {
        if denied.contains(env) {
            continue;
        }
        match store.findsecret(reference).await {
            Ok(Some(secret)) => {
                resolved.env.insert(env.clone(), secret);
            }
            Ok(None) => resolved
                .warnings
                .push(format!("{env} references unknown secret {reference}")),
            Err(error) => resolved.warnings.push(format!("{env}: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn yaml_scopes_override_global_and_require_approval() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("project");
        let nested = project.join("src");
        fs::create_dir_all(&nested).unwrap();
        let database = directory.path().join("brain.db");
        let store = VaultStore::open(&database).await.unwrap();
        let global = store.createvault("global").await.unwrap();
        store
            .createsecret(global.id, "token", "TOKEN", true)
            .await
            .unwrap();
        let work = store.createvault("work").await.unwrap();
        store
            .createsecret(work.id, "token", "TOKEN", false)
            .await
            .unwrap();
        let path = project.join(CONFIG);
        fs::write(
            &path,
            "version: 1\nscope: project\nenv:\n  TOKEN: work.token\ndeny: []\n",
        )
        .unwrap();

        let pending = resolve(&store, &nested).await.unwrap();
        assert_eq!(pending.env["TOKEN"].vault, "global");
        assert!(!pending.scopes[0].trusted);

        let (_, digest) = read(&path).unwrap();
        store.trust(&path, &digest).await.unwrap();
        let active = resolve(&store, &nested).await.unwrap();
        assert_eq!(active.env["TOKEN"].vault, "work");
        assert!(active.scopes[0].trusted);

        fs::write(
            &path,
            "version: 1\nscope: project\nenv:\n  TOKEN: work.token\ndeny: []\n\n",
        )
        .unwrap();
        let changed = resolve(&store, &nested).await.unwrap();
        assert_eq!(changed.env["TOKEN"].vault, "global");
        assert!(changed.scopes[0].changed);
    }

    #[test]
    fn template_is_valid_yaml() {
        let config: ScopeConfig = serde_saphyr::from_str(template()).unwrap();
        assert_eq!(config.version, 1);
    }
}
