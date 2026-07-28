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
    let store = VaultStore::open(crate::files::database()?).await?;
    let resolved = resolve(&store, Path::new(&folder)).await?;
    anyhow::ensure!(
        resolved.warnings.is_empty(),
        "vault scope is not ready:\n{}",
        resolved.warnings.join("\n")
    );
    let mut command = Command::new(program);
    command.args(arguments);
    for (env, secret) in resolved.env {
        command.env(env, getsecret(&secret.account)?);
    }
    command
        .status()
        .with_context(|| format!("could not launch {}", program.to_string_lossy()))
}
