mod cipher;
mod clipboard;
mod key;
mod keychain;
mod model;
mod run;
mod scope;
mod sealed;
mod shell;
mod store;
mod values;

pub use model::{
    Resolved, ScopeConfig, ScopeKind, ScopeState, Secret, Vault, VaultStatusRequest,
    VaultStatusResponse,
};
pub use run::{environment, names, run};
pub use scope::{CONFIG, discover, read as readscope, resolve, template, templatefor};
pub use shell::{
    Shell, changes as shellchanges, clear as shellclear, hook as shellhook,
    hookcommand as shellhookcommand,
};
pub use store::VaultStore;
pub use values::{Backend, Migration, Moved, Values, backend, migrate, setbackend};

pub async fn setsecret(account: &str, value: &str) -> anyhow::Result<()> {
    Values::open().await?.set(account, value).await
}

pub async fn getsecret(account: &str) -> anyhow::Result<String> {
    Values::open().await?.get(account).await
}

/// Forget a value in both stores rather than only the one in use.
///
/// A value left behind in the store this machine stopped reading is a value
/// nobody can see and nobody deleted, which is the opposite of what `forget`
/// promises. The second removal is best effort: the other backend may not
/// exist on this platform at all.
pub async fn deletesecret(account: &str) -> anyhow::Result<()> {
    let values = Values::open().await?;
    values.delete(account).await?;
    if let Ok(other) = Values::of(values.backend().other()).await {
        let _ = other.delete(account).await;
    }
    Ok(())
}

/// The one way a person gets their own secret back. It goes to the pasteboard
/// and is never returned, so it stays off every display, log, and response the
/// way it does everywhere else.
pub async fn copysecret(account: &str) -> anyhow::Result<()> {
    clipboard::copy(&getsecret(account).await?)
}
