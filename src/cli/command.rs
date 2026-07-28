use crate::cli::Outcome;
use crate::vault::{ScopeKind, VaultStatusResponse, VaultStore};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

const HELP: &str = "Synaps · local memory and scoped credentials

Usage: synaps [command]

Commands:
  app                              Open the desktop application
  mcp                              Run the MCP stdio server
  run -- <command> [arguments]     Run with resolved vault variables
  hook <zsh|bash|fish>             Print automatic shell activation setup
  allow [folder]                   Approve the closest .synaps.yaml digest
  deny [folder]                    Revoke approval for the closest scope
  status [folder] [--json]         Show names, scope trust, and ambient readiness
  vault list                       List vaults
  vault create <name>              Create a vault
  vault delete <name>              Delete an empty vault
  secret list <vault>              List secret names without values
  secret set <vault> <name> <env> [--global]
                                   Read a value securely and save it
  secret forget <vault.name>       Remove a Keychain value and its label
  secret global <vault.name> <on|off>
                                   Change global availability
  scope init [folder] [--folder]   Create .synaps.yaml
  scope trust [folder]             Approve the current YAML digest
  scope status [folder]            Show resolved scope status
  data check [--json]              Verify database integrity and version
  data export <file>               Export a consistent database backup
  data restore <file>              Restore a backup while Synaps is closed
  memory list [query] [--json]     Search or list recent memories
  memory show <id> [--json]        Inspect one memory
  memory add [source]              Store content read from stdin
  memory edit <id> [source]        Replace content read from stdin
  memory delete <id> --confirm     Delete one memory
  memory wipe --confirm            Delete every memory
  settings show                    Show recall and shell settings
  settings optimize <full|balanced|lean>
                                   Set the MCP recall response budget
  install                          Install the synaps CLI for this user
  path                             Print the local data and CLI paths
  version                          Print the version
  help                             Show this help

Secret values are never accepted as command arguments, written to YAML or SQLite,
or returned by MCP.";

pub fn run(arguments: Vec<OsString>) -> Result<Outcome> {
    let Some(command) = arguments.first().and_then(|value| value.to_str()) else {
        return Ok(Outcome::App);
    };
    let rest = &arguments[1..];
    match command {
        "app" => Ok(Outcome::App),
        "help" | "--help" | "-h" => output(HELP),
        "version" | "--version" | "-V" => output(&format!("synaps {}", env!("CARGO_PKG_VERSION"))),
        "mcp" => {
            runtime()?.block_on(crate::mcp::run())?;
            Ok(Outcome::Exit(0))
        }
        "run" => {
            let status = runtime()?.block_on(crate::vault::run(rest.to_vec()))?;
            Ok(Outcome::Exit(status.code().unwrap_or(1)))
        }
        "hook" => super::shell::hook(rest),
        "export" => super::shell::environment(rest),
        "allow" => super::shell::allow(rest),
        "deny" => super::shell::deny(rest),
        "status" => status(rest),
        "vault" => vault(rest),
        "secret" => secret(rest),
        "scope" => scope(rest),
        "data" => data(rest),
        "memory" => memory(rest),
        "settings" => settings(rest),
        "install" => install(),
        "path" => paths(),
        unknown => anyhow::bail!("unknown command `{unknown}`\n\n{HELP}"),
    }
}

fn status(arguments: &[OsString]) -> Result<Outcome> {
    let json = arguments.iter().any(|value| value == "--json");
    let path = arguments
        .iter()
        .find(|value| *value != "--json")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let response = runtime()?.block_on(statusresponse(&path))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Folder: {}", response.path);
        if response.available.is_empty() {
            println!("Available: none");
        } else {
            println!("Available: {}", response.available.join(", "));
        }
        println!(
            "Ambient: {}{}",
            response.ambient,
            response
                .shell
                .as_deref()
                .map(|shell| format!(" · {shell} hook detected"))
                .unwrap_or_default()
        );
        for scope in response.scopes {
            let state = if scope.trusted {
                "approved"
            } else if scope.changed {
                "changed"
            } else {
                "pending"
            };
            println!("{} [{} · {}]", scope.path, scope.scope, state);
        }
        for warning in response.warnings {
            eprintln!("warning: {warning}");
        }
    }
    Ok(Outcome::Exit(0))
}

async fn statusresponse(path: &Path) -> Result<VaultStatusResponse> {
    let store = VaultStore::open(crate::files::database()?).await?;
    let resolved = crate::vault::resolve(&store, path).await?;
    let ambient = if resolved.scopes.is_empty() {
        "inactive"
    } else if resolved.warnings.is_empty() {
        "ready"
    } else {
        "blocked"
    };
    Ok(VaultStatusResponse {
        path: path.display().to_string(),
        available: resolved.env.keys().cloned().collect(),
        scopes: resolved.scopes.into_iter().map(Into::into).collect(),
        warnings: resolved.warnings,
        ambient: ambient.to_owned(),
        shell: std::env::var("SYNAPS_SHELL_ACTIVE").ok(),
        note: "Values remain in Keychain.".to_owned(),
    })
}

fn vault(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(arguments, 0, "usage: synaps vault <list|create|delete>")?;
    let runtime = runtime()?;
    let store = runtime.block_on(VaultStore::open(crate::files::database()?))?;
    match action.as_str() {
        "list" => {
            for vault in runtime.block_on(store.vaults())? {
                println!("{}", vault.name);
            }
        }
        "create" => {
            let name = textarg(arguments, 1, "usage: synaps vault create <name>")?;
            let vault = runtime.block_on(store.createvault(&name))?;
            println!("Created vault {}", vault.name);
        }
        "delete" => {
            let name = textarg(arguments, 1, "usage: synaps vault delete <name>")?;
            let vaults = runtime.block_on(store.vaults())?;
            let vault = vaults
                .into_iter()
                .find(|vault| vault.name == name)
                .context("vault not found")?;
            runtime.block_on(store.deletevault(vault.id))?;
            println!("Deleted vault {}", vault.name);
        }
        _ => anyhow::bail!("unknown vault command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn secret(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(
        arguments,
        0,
        "usage: synaps secret <list|set|forget|global>",
    )?;
    let runtime = runtime()?;
    let store = runtime.block_on(VaultStore::open(crate::files::database()?))?;
    match action.as_str() {
        "list" => {
            let name = textarg(arguments, 1, "usage: synaps secret list <vault>")?;
            let vault = runtime
                .block_on(store.vaults())?
                .into_iter()
                .find(|vault| vault.name == name)
                .context("vault not found")?;
            for secret in runtime.block_on(store.secrets(vault.id))? {
                println!(
                    "{}.{}\t{}\t{}",
                    secret.vault,
                    secret.name,
                    secret.env,
                    if secret.global { "global" } else { "scoped" }
                );
            }
        }
        "set" => setsecret(&runtime, &store, arguments)?,
        "forget" => {
            let reference = textarg(arguments, 1, "usage: synaps secret forget <vault.name>")?;
            let secret = runtime
                .block_on(store.findsecret(&reference))?
                .context("secret not found")?;
            crate::vault::deletesecret(&secret.account)?;
            runtime.block_on(store.deletesecret(secret.id))?;
            println!("Forgot {reference}");
        }
        "global" => {
            let reference = textarg(
                arguments,
                1,
                "usage: synaps secret global <vault.name> <on|off>",
            )?;
            let enabled = match textarg(
                arguments,
                2,
                "usage: synaps secret global <vault.name> <on|off>",
            )?
            .as_str()
            {
                "on" => true,
                "off" => false,
                value => anyhow::bail!("expected `on` or `off`, got `{value}`"),
            };
            let secret = runtime
                .block_on(store.findsecret(&reference))?
                .context("secret not found")?;
            runtime.block_on(store.setglobal(secret.id, enabled))?;
            println!(
                "{reference} is now {}",
                if enabled { "global" } else { "scoped" }
            );
        }
        _ => anyhow::bail!("unknown secret command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn setsecret(
    runtime: &tokio::runtime::Runtime,
    store: &VaultStore,
    arguments: &[OsString],
) -> Result<()> {
    let vaultname = textarg(
        arguments,
        1,
        "usage: synaps secret set <vault> <name> <env> [--global]",
    )?;
    let name = textarg(
        arguments,
        2,
        "usage: synaps secret set <vault> <name> <env> [--global]",
    )?;
    let env = textarg(
        arguments,
        3,
        "usage: synaps secret set <vault> <name> <env> [--global]",
    )?;
    let global = arguments.iter().any(|value| value == "--global");
    let vault = runtime
        .block_on(store.vaults())?
        .into_iter()
        .find(|vault| vault.name == vaultname)
        .context("vault not found")?;
    let reference = format!("{vaultname}.{name}");
    let value = readsecret()?;
    if let Some(secret) = runtime.block_on(store.findsecret(&reference))? {
        anyhow::ensure!(
            secret.env == env.to_ascii_uppercase(),
            "existing secret uses environment name {}",
            secret.env
        );
        crate::vault::setsecret(&secret.account, &value)?;
        if global {
            runtime.block_on(store.setglobal(secret.id, true))?;
        }
    } else {
        let secret = runtime.block_on(store.createsecret(vault.id, &name, &env, global))?;
        if let Err(error) = crate::vault::setsecret(&secret.account, &value) {
            let _ = runtime.block_on(store.deletesecret(secret.id));
            return Err(error);
        }
    }
    println!("Saved {reference} in Keychain");
    Ok(())
}

fn scope(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(arguments, 0, "usage: synaps scope <init|trust|status>")?;
    if action == "status" {
        return status(&arguments[1..]);
    }
    let folder = arguments
        .iter()
        .skip(1)
        .find(|value| *value != "--folder")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let path = if folder.file_name().and_then(|name| name.to_str()) == Some(crate::vault::CONFIG) {
        folder
    } else {
        folder.join(crate::vault::CONFIG)
    };
    match action.as_str() {
        "init" => {
            anyhow::ensure!(!path.exists(), "{} already exists", path.display());
            let kind = if arguments.iter().any(|value| value == "--folder") {
                ScopeKind::Folder
            } else {
                ScopeKind::Project
            };
            crate::files::write(&path, crate::vault::templatefor(kind))?;
            println!("Created {}", path.display());
        }
        "trust" => {
            let (_, digest) = crate::vault::readscope(&path)?;
            let runtime = runtime()?;
            let store = runtime.block_on(VaultStore::open(crate::files::database()?))?;
            runtime.block_on(store.trust(&path, &digest))?;
            println!("Approved {}", path.display());
        }
        _ => anyhow::bail!("unknown scope command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn install() -> Result<Outcome> {
    let target = crate::cli::install()?;
    println!("Installed {}", target.display());
    if !pathcontains(target.parent().unwrap_or_else(|| Path::new("."))) {
        println!(
            "Add {} to PATH to use synaps from your shell.",
            target.parent().unwrap().display()
        );
    }
    Ok(Outcome::Exit(0))
}

fn data(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(arguments, 0, "usage: synaps data <check|export|restore>")?;
    let database = crate::files::database()?;
    let runtime = runtime()?;
    match action.as_str() {
        "check" => {
            let report = runtime.block_on(crate::database::check(&database))?;
            if arguments.iter().any(|value| value == "--json") {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Database: {}", report.path);
                println!("Version: {}", report.version);
                println!("Integrity: {}", report.integrity);
            }
        }
        "export" => {
            let target = PathBuf::from(textarg(arguments, 1, "usage: synaps data export <file>")?);
            runtime.block_on(crate::database::export(&database, &target))?;
            println!("Exported {}", target.display());
        }
        "restore" => {
            let source = PathBuf::from(textarg(arguments, 1, "usage: synaps data restore <file>")?);
            let backup = runtime.block_on(crate::database::restore(&database, &source))?;
            println!("Restored {}", source.display());
            if let Some(backup) = backup {
                println!("Previous database: {}", backup.display());
            }
        }
        _ => anyhow::bail!("unknown data command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn settings(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(arguments, 0, "usage: synaps settings <show|optimize>")?;
    let runtime = runtime()?;
    let brain = runtime.block_on(crate::brain::Brain::open(crate::files::database()?))?;
    match action.as_str() {
        "show" => {
            let settings = runtime.block_on(brain.settings())?;
            println!("optimization\t{}", settings.optimization.value());
            println!("result limit\t{}", settings.resultlimit);
            println!(
                "character budget\t{}",
                settings
                    .characterbudget
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unlimited".to_owned())
            );
            println!("shell modes\tcommand-scoped, ambient directory");
            println!("zsh hook\teval \"$(synaps hook zsh)\"");
        }
        "optimize" => {
            let value = textarg(
                arguments,
                1,
                "usage: synaps settings optimize <full|balanced|lean>",
            )?;
            let optimization = value.parse()?;
            runtime.block_on(brain.setoptimization(optimization))?;
            println!("Recall optimization set to {}", optimization.name());
        }
        _ => anyhow::bail!("unknown settings command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn memory(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(
        arguments,
        0,
        "usage: synaps memory <list|show|add|edit|delete|wipe>",
    )?;
    let runtime = runtime()?;
    let brain = runtime.block_on(crate::brain::Brain::open(crate::files::database()?))?;
    let json = arguments.iter().any(|value| value == "--json");
    match action.as_str() {
        "list" => {
            let query = arguments
                .iter()
                .skip(1)
                .filter(|value| *value != "--json")
                .map(|value| value.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            let memories = runtime.block_on(brain.search(&query, 100))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&memories)?);
            } else {
                for memory in memories {
                    println!(
                        "{}\t{}\t{}",
                        memory.id,
                        if memory.source.is_empty() {
                            "-"
                        } else {
                            &memory.source
                        },
                        memorypreview(&memory.body)
                    );
                }
            }
        }
        "show" => {
            let id = memoryid(arguments, 1, "usage: synaps memory show <id> [--json]")?;
            let memory = runtime
                .block_on(brain.memory(id))?
                .context("memory not found")?;
            if json {
                println!("{}", serde_json::to_string_pretty(&memory)?);
            } else {
                println!("Memory #{}", memory.id);
                println!("Source: {}", memory.source);
                println!("Created: {}", memory.created);
                println!();
                println!("{}", memory.body);
            }
        }
        "add" => {
            let source = arguments.get(1).and_then(|value| value.to_str());
            let body = readmemory()?;
            let id = runtime.block_on(brain.remember(&body, source))?;
            println!("Stored memory #{id}");
        }
        "edit" => {
            let id = memoryid(arguments, 1, "usage: synaps memory edit <id> [source]")?;
            let source = arguments.get(2).and_then(|value| value.to_str());
            let body = readmemory()?;
            runtime
                .block_on(brain.updatememory(id, &body, source))?
                .context("memory not found")?;
            println!("Updated memory #{id}");
        }
        "delete" => {
            let id = memoryid(arguments, 1, "usage: synaps memory delete <id> --confirm")?;
            anyhow::ensure!(
                arguments.iter().any(|value| value == "--confirm"),
                "add --confirm to delete memory #{id}"
            );
            runtime
                .block_on(brain.deletememory(id))?
                .context("memory not found")?;
            println!("Deleted memory #{id}");
        }
        "wipe" => {
            anyhow::ensure!(
                arguments.iter().any(|value| value == "--confirm"),
                "add --confirm to delete every memory"
            );
            let count = runtime.block_on(brain.wipememories())?;
            println!("Deleted {count} memories");
        }
        _ => anyhow::bail!("unknown memory command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn paths() -> Result<Outcome> {
    println!("data\t{}", crate::files::data()?.display());
    println!("cli\t{}", crate::cli::destination()?.display());
    Ok(Outcome::Exit(0))
}

fn readsecret() -> Result<String> {
    if std::io::stdin().is_terminal() {
        let value = rpassword::prompt_password("Secret value: ")?;
        anyhow::ensure!(!value.is_empty(), "secret value cannot be empty");
        return Ok(value);
    }
    let mut value = String::new();
    std::io::stdin().read_to_string(&mut value)?;
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    anyhow::ensure!(!value.is_empty(), "secret value cannot be empty");
    Ok(value)
}

fn readmemory() -> Result<String> {
    anyhow::ensure!(
        !std::io::stdin().is_terminal(),
        "pipe the replacement memory content to stdin"
    );
    let mut value = String::new();
    std::io::stdin().read_to_string(&mut value)?;
    anyhow::ensure!(!value.trim().is_empty(), "memory content cannot be empty");
    Ok(value)
}

fn memoryid(arguments: &[OsString], index: usize, usage: &str) -> Result<i64> {
    textarg(arguments, index, usage)?
        .parse()
        .with_context(|| usage.to_owned())
}

fn memorypreview(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = compact.chars();
    let preview = characters.by_ref().take(80).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn textarg(arguments: &[OsString], index: usize, usage: &str) -> Result<String> {
    arguments
        .get(index)
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .with_context(|| usage.to_owned())
}

fn pathcontains(folder: &Path) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|item| item == folder))
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new().map_err(Into::into)
}

fn output(value: &str) -> Result<Outcome> {
    println!("{value}");
    Ok(Outcome::Exit(0))
}
