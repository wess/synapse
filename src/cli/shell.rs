use crate::cli::Outcome;
use crate::vault::{Shell, VaultStore};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub fn hook(arguments: &[OsString]) -> Result<Outcome> {
    let shell = shellarg(arguments, "usage: synapse hook <zsh|bash|fish>")?;
    let script = std::env::var("SYNAPSE_SHELL_COMMAND")
        .ok()
        .filter(|command| !command.is_empty())
        .map(|command| crate::vault::shellhookcommand(shell, &command))
        .unwrap_or_else(|| crate::vault::shellhook(shell));
    println!("{script}");
    Ok(Outcome::Exit(0))
}

pub fn environment(arguments: &[OsString]) -> Result<Outcome> {
    let shell = shellarg(arguments, "usage: synapse export <zsh|bash|fish>")?;
    let previous = std::env::var("SYNAPSE_SHELL_KEYS").unwrap_or_default();
    let script = prepare(shell, &previous).unwrap_or_else(|error| {
        eprintln!("synapse: ambient activation paused: {error:#}");
        crate::vault::shellclear(shell, &previous, "error")
    });
    print!("{script}");
    Ok(Outcome::Exit(0))
}

pub fn allow(arguments: &[OsString]) -> Result<Outcome> {
    approval(arguments, true)
}

pub fn deny(arguments: &[OsString]) -> Result<Outcome> {
    approval(arguments, false)
}

fn prepare(shell: Shell, previous: &str) -> Result<String> {
    let folder = std::env::current_dir().context("could not read the current folder")?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let store = VaultStore::open(crate::files::database()?).await?;
        crate::vault::shellchanges(shell, &store, &folder, previous).await
    })
}

fn approval(arguments: &[OsString], allowed: bool) -> Result<Outcome> {
    anyhow::ensure!(
        arguments.len() <= 1,
        "usage: synapse {} [folder]",
        if allowed { "allow" } else { "deny" }
    );
    let folder = arguments
        .first()
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let path = scopepath(&folder)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let store = runtime.block_on(VaultStore::open(crate::files::database()?))?;
    if allowed {
        let (_, digest) = crate::vault::readscope(&path)?;
        runtime.block_on(store.trust(&path, &digest))?;
        println!("Allowed {}", path.display());
    } else {
        runtime.block_on(store.untrust(&path))?;
        println!("Denied {}", path.display());
    }
    Ok(Outcome::Exit(0))
}

fn scopepath(path: &Path) -> Result<PathBuf> {
    if path.file_name().and_then(|name| name.to_str()) == Some(crate::vault::CONFIG) {
        anyhow::ensure!(path.is_file(), "{} does not exist", path.display());
        return Ok(path.to_path_buf());
    }
    crate::vault::discover(path)?
        .pop()
        .context("no .synapse.yaml found here or in a parent folder")
}

fn shellarg(arguments: &[OsString], usage: &str) -> Result<Shell> {
    anyhow::ensure!(arguments.len() == 1, "{usage}");
    arguments[0]
        .to_str()
        .context(usage.to_owned())
        .and_then(Shell::from_str)
}
