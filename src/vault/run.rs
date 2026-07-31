use crate::vault::{VaultStore, getsecret, resolve};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, ExitStatus};

pub async fn run(arguments: Vec<OsString>) -> Result<ExitStatus> {
    let arguments = arguments
        .into_iter()
        .skip_while(|value| value == "--")
        .collect::<Vec<_>>();
    let (program, arguments) = arguments
        .split_first()
        .context("usage: synapse run -- <command> [arguments]")?;
    let folder = std::env::current_dir().context("could not read the current folder")?;
    let mut command = Command::new(program);
    command.args(arguments);
    command.envs(environment(&folder).await?);
    command
        .status()
        .with_context(|| format!("could not launch {}", program.to_string_lossy()))
}

/// The variables a child launched in `folder` should carry, with the values read
/// from Keychain.
///
/// Refuses on any scope warning rather than launching with part of an
/// environment: a command that runs with half its credentials fails somewhere
/// less obvious than here. Shared with `synapse launch`, so the two cannot come
/// to disagree about what a ready scope is.
pub async fn environment(folder: &Path) -> Result<Vec<(String, String)>> {
    let store = VaultStore::open(crate::files::database()?).await?;
    let resolved = resolve(&store, folder).await?;
    anyhow::ensure!(
        resolved.warnings.is_empty(),
        "vault scope is not ready:\n{}",
        resolved.warnings.join("\n")
    );
    resolved
        .env
        .into_iter()
        .map(|(env, secret)| Ok((env, getsecret(&secret.account)?)))
        .collect()
}

/// The variable *names* a child launched in `folder` would carry, without
/// reading a single value out of Keychain. This is what a preview prints:
/// secrets never reach a terminal, a log, or a screenshot in a bug report.
pub async fn names(folder: &Path) -> Result<Vec<String>> {
    let store = VaultStore::open(crate::files::database()?).await?;
    let resolved = resolve(&store, folder).await?;
    anyhow::ensure!(
        resolved.warnings.is_empty(),
        "vault scope is not ready:\n{}",
        resolved.warnings.join("\n")
    );
    Ok(resolved.env.into_keys().collect())
}
