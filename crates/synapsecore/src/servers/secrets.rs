//! A server's secrets live in the vault, one vault per server.
//!
//! One vault each rather than one for all of them because a vault holds one
//! secret per environment variable, and two servers wanting `GITHUB_TOKEN` is
//! ordinary. The library file holds only the reference.

use crate::servers::library;
use crate::vault::VaultStore;
use anyhow::{Context, Result};

fn vaultname(server: &str) -> String {
    format!("mcp-{server}")
}

fn secretname(variable: &str) -> String {
    variable.to_ascii_lowercase().replace('_', "-")
}

/// Store a value for one of a server's variables and point the server at it.
/// The value comes from a prompt or a pipe, never an argument, the same as
/// every other secret.
pub async fn setsecret(server: &str, variable: &str, value: &str) -> Result<String> {
    let mut entry = library::read(server)?;
    anyhow::ensure!(
        !entry.env.contains_key(variable),
        "`{server}` sets {variable} as a plain value; remove it from [env] with `synapse mcp edit {server}` first"
    );
    let store = VaultStore::open(crate::files::database()?).await?;
    let reference = store_value(&store, server, variable, value).await?;
    entry.secrets.insert(variable.to_owned(), reference.clone());
    library::save(&entry)?;
    Ok(reference)
}

/// Write a value into the server's own vault, creating the vault and the
/// secret as needed, and return its reference.
pub(super) async fn store_value(
    store: &VaultStore,
    server: &str,
    variable: &str,
    value: &str,
) -> Result<String> {
    let entered = crate::vault::entered(value, &[server, variable])?;
    let vault = vaultname(server);
    let reference = format!("{vault}.{}", secretname(variable));
    let secret = match store.findsecret(&reference).await? {
        Some(secret) => secret,
        None => {
            let existing = store
                .vaults()
                .await?
                .into_iter()
                .find(|item| item.name == vault);
            let id = match existing {
                Some(vault) => vault.id,
                None => store.createvault(&vault).await?.id,
            };
            store
                .createsecret(id, &secretname(variable), variable, false)
                .await
                .with_context(|| format!("could not make a place for {variable} in {vault}"))?
        }
    };
    crate::vault::setsecret(&secret.account, &entered.value).await?;
    Ok(reference)
}

/// Forget every secret a server holds, along with its vault. Used when the
/// server itself is deleted: a secret nothing can reach is one nobody deleted.
pub async fn forgetsecret(server: &str) -> Result<usize> {
    let store = VaultStore::open(crate::files::database()?).await?;
    let Some(vault) = store
        .vaults()
        .await?
        .into_iter()
        .find(|item| item.name == vaultname(server))
    else {
        return Ok(0);
    };
    let secrets = store.secrets(vault.id).await?;
    for secret in &secrets {
        crate::vault::deletesecret(&secret.account).await?;
    }
    store.deletevault(vault.id).await?;
    Ok(secrets.len())
}
