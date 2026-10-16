//! Bringing a server a tool already has into the library.
//!
//! This is where the plaintext token comes out of the tool's config: every
//! variable the entry carries goes into the vault, and the library keeps only
//! the reference. Adopting does not touch the tool — its own entry is still
//! there, still holding the value, until a person installs over it with
//! `replace`. The tool is theirs, so swapping its entry is their call.

use crate::agent::Agent;
use crate::agent::config;
use crate::servers::library;
use crate::servers::model::{Server, slug, validname};
use crate::vault::VaultStore;
use anyhow::Result;

/// Servers a tool has that are not Synapse's, by (tool, name). Synapse's own
/// entry is not offered; neither is one the library already has a server for,
/// because that row is already on the page with a "not ours" badge.
pub fn foreign(agents: &[Agent], known: &[String]) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for agent in agents.iter().filter(|agent| agent.servers.supported()) {
        let ours = crate::servers::ours(agent);
        for (name, _) in config::servers(agent, &agent.servers.at) {
            if name == "synapse" || ours.contains(&name) || known.contains(&name) {
                continue;
            }
            found.push((agent.name.clone(), name));
        }
    }
    found.sort();
    found
}

/// Copy a tool's server into the library, its variables into the vault.
/// Returns the library name, which is the tool's name made valid.
pub async fn adopt(agent: &Agent, name: &str) -> Result<String> {
    let entry = config::servers(agent, &agent.servers.at)
        .into_iter()
        .find(|(entry, _)| entry == name)
        .map(|(_, entry)| entry)
        .ok_or_else(|| anyhow::anyhow!("{} has no MCP server named `{name}`", agent.name))?;
    anyhow::ensure!(
        entry.url.is_empty(),
        "`{name}` is a remote server; Synapse can only keep servers it starts itself"
    );
    anyhow::ensure!(
        !entry.command.is_empty(),
        "{}'s `{name}` entry has no command",
        agent.name
    );
    let library_name = match validname(name) {
        Ok(()) => name.to_owned(),
        Err(_) => slug(name),
    };
    validname(&library_name)?;
    anyhow::ensure!(
        !library::exists(&library_name),
        "the library already has a server named `{library_name}`"
    );

    let mut server = Server {
        name: library_name.clone(),
        command: entry.command,
        args: entry.args,
        source: format!("adopted from {}", agent.name),
        ..Server::default()
    };
    if !entry.env.is_empty() {
        // Every variable, not only the ones whose names look secret: a value
        // misjudged as harmless would sit in plaintext in this folder, and one
        // misjudged as secret costs nothing but a vault entry.
        let store = VaultStore::open(crate::files::database()?).await?;
        for (variable, value) in &entry.env {
            // Nothing to keep, and kept as an empty setting it would read as one
            // the server still needs.
            if value.trim().is_empty() {
                continue;
            }
            let reference =
                crate::servers::secrets::store_value(&store, &library_name, variable, value)
                    .await?;
            server.secrets.insert(variable.clone(), reference);
        }
    }
    library::save(&server)?;
    Ok(library_name)
}
