mod keychain;
mod model;
mod run;
mod scope;
mod shell;
mod store;

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

pub fn setsecret(account: &str, value: &str) -> anyhow::Result<()> {
    keychain::set(account, value)
}

pub fn getsecret(account: &str) -> anyhow::Result<String> {
    keychain::get(account)
}

pub fn deletesecret(account: &str) -> anyhow::Result<()> {
    keychain::delete(account)
}
