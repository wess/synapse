use crate::cli::Outcome;
use crate::vault::{ScopeKind, VaultStatusResponse, VaultStore};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

const HELP: &str = "Synapse · local memory and scoped credentials

Usage: synapse [command]

Commands:
  app                              Open the desktop application
  mcp                              Run the MCP stdio server
  launch <tool> [-- <flags>]       Start a tool wired into memory and the vault
  run -- <command> [arguments]     Run with resolved vault variables
  mux [--as <name>] [--team <team>]
                                   Drive a team of agents from this terminal
  hook <zsh|bash|fish>             Print automatic shell activation setup
  allow [folder]                   Approve the closest .synapse.yaml digest
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
  scope init [folder] [--folder]   Create .synapse.yaml
  scope trust [folder]             Approve the current YAML digest
  scope status [folder]            Show resolved scope status
  data check [--json]              Verify database integrity and version
  data export <file>               Export a consistent database backup
  data restore <file>              Restore a backup while Synapse is closed
  memory list [query] [--json] [--explain]
                                   Search or list recent memories
  memory grep <text> [--json]      Find memories containing an exact string
  memory show <id> [--json]        Inspect one memory
  memory add [source] [--global|--project <folder>]
                                   Store content read from stdin
  memory edit <id> [source]        Replace content read from stdin
  memory supersede <old> <new>     Stop recalling one memory in favour of another
  memory restore <id>              Recall a superseded memory again
  memory import <claude|codex|markdown> [path]
                                   Preview an existing memory store
  memory import <source> [path] --confirm
                                   Import safe entries after preview
  memory imports [--json]          Show reversible import batches
  memory undo <batch> --confirm    Undo one import batch
  memory delete <id> --confirm     Delete one memory
  memory wipe --confirm            Delete every memory
  guidance show [--json]           Show SOUL.md and pointer status
  guidance sync                    Create SOUL.md and refresh both pointers
  guidance adopt --confirm         Move global guidance into SOUL.md
  relay status [--json]            Show mesh state, roster, and workers
  relay agents [--json]            List agents on the mesh
  relay channels [--json]          List channels and subscriber counts
  relay feed [--follow] [--since <id>]
                                   Print cross-agent messages
  relay launch <name> [--role <role>] [--tool <tool>] [--task <text>]
                                   Open one agent wired into the mesh
  relay team open <name>           Open a whole roster, lead in this terminal
  relay role <list|show|create|edit|delete> [name] [--user]
                                   Manage reusable agent roles
  relay team <list|show|create|edit|delete> [name] [--user]
                                   Manage team rosters
  relay ps [--json]                List background workers
  relay kill <name>                Stop a background worker
  skill list [--json]              List the skills in your library
  skill show <name>                Print one skill
  skill create <name>              Start a new skill from a template
  skill edit <name>                Edit a skill in $EDITOR
  skill delete <name> --confirm    Remove a skill from the library
  skill install [name] [--tool <tool>] [--replace]
                                   Copy skills into your connected tools
  skill remove <name> [--tool <tool>] [--force]
                                   Take a skill back out of a tool
  skill status [name] [--json]     Show where each skill is installed
  skill adopt <name> [--tool <tool>]
                                   Copy a tool's own skill into the library
  skill proposed [--json]          List skills agents wrote, awaiting review
  skill approve <name> [--tool <tool>]
                                   Install a proposed skill and stop proposing it
  skill reject <name> --confirm    Turn down a proposed skill
  skill history <name> [--json]    Show what a skill used to say
  skill revert <name> [<id>]       Put an earlier version of a skill back
  tool list [--json]               List the tools Synapse can connect to
  tool show <name>                 Print one tool's descriptor
  tool create <name>               Describe a tool Synapse does not ship
  tool edit <name>                 Edit a tool descriptor in $EDITOR
  tool delete <name>               Remove a descriptor you added
  session [--json]                 Report this session's Synapse connection
  statusline                       Print one status line for a connected tool
  compact                          Answer a tool's pre-compaction hook
  doctor [--json]                  Report everything a bug report needs
  settings show                    Show recall, mesh, and shell settings
  settings optimize <full|balanced|lean>
                                   Set the MCP recall response budget
  settings mesh <on|off>           Turn the agent mesh tools on or off
  settings learn <on|off>          Let agents write and correct skills
  install                          Install the synapse CLI for this user
  connect [tool]                   Wire a tool into memory, or every one found
  disconnect [tool]                Undo one tool's connection, or every tool's
  uninstall [--data] [--confirm]   Remove everything Synapse installed
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
        "version" | "--version" | "-V" => output(&format!("synapse {}", env!("CARGO_PKG_VERSION"))),
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
        "memory" => super::memory::run(rest),
        "guidance" => super::guidance::run(rest),
        "launch" => super::wrap::run(rest),
        "mux" => super::mux::run(rest),
        "relay" => super::relay::run(rest),
        "skill" => super::skills::run(rest),
        "tool" => super::layers::tool(rest),
        "session" => super::session::session(rest),
        "statusline" => super::session::statusline(rest),
        "compact" => super::session::compact(rest),
        "doctor" => super::doctor::doctor(rest),
        "settings" => settings(rest),
        "install" => install(),
        "connect" => super::connect::connect(rest),
        "disconnect" => super::remove::disconnect(rest),
        "uninstall" => super::remove::uninstall(rest),
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
        shell: std::env::var("SYNAPSE_SHELL_ACTIVE").ok(),
        note: "Values remain in Keychain.".to_owned(),
    })
}

fn vault(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(arguments, 0, "usage: synapse vault <list|create|delete>")?;
    let runtime = runtime()?;
    let store = runtime.block_on(VaultStore::open(crate::files::database()?))?;
    match action.as_str() {
        "list" => {
            for vault in runtime.block_on(store.vaults())? {
                println!("{}", vault.name);
            }
        }
        "create" => {
            let name = textarg(arguments, 1, "usage: synapse vault create <name>")?;
            let vault = runtime.block_on(store.createvault(&name))?;
            println!("Created vault {}", vault.name);
        }
        "delete" => {
            let name = textarg(arguments, 1, "usage: synapse vault delete <name>")?;
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
        "usage: synapse secret <list|set|forget|global>",
    )?;
    let runtime = runtime()?;
    let store = runtime.block_on(VaultStore::open(crate::files::database()?))?;
    match action.as_str() {
        "list" => {
            let name = textarg(arguments, 1, "usage: synapse secret list <vault>")?;
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
            let reference = textarg(arguments, 1, "usage: synapse secret forget <vault.name>")?;
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
                "usage: synapse secret global <vault.name> <on|off>",
            )?;
            let enabled = match textarg(
                arguments,
                2,
                "usage: synapse secret global <vault.name> <on|off>",
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
        "usage: synapse secret set <vault> <name> <env> [--global]",
    )?;
    let name = textarg(
        arguments,
        2,
        "usage: synapse secret set <vault> <name> <env> [--global]",
    )?;
    let env = textarg(
        arguments,
        3,
        "usage: synapse secret set <vault> <name> <env> [--global]",
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
    let action = textarg(arguments, 0, "usage: synapse scope <init|trust|status>")?;
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
    let folder = target.parent().unwrap_or_else(|| Path::new("."));
    if !pathcontains(folder) {
        println!(
            "Add {} to PATH to use synapse from your shell.",
            folder.display()
        );
    }
    Ok(Outcome::Exit(0))
}

fn data(arguments: &[OsString]) -> Result<Outcome> {
    let action = textarg(arguments, 0, "usage: synapse data <check|export|restore>")?;
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
            let target = PathBuf::from(textarg(arguments, 1, "usage: synapse data export <file>")?);
            runtime.block_on(crate::database::export(&database, &target))?;
            println!("Exported {}", target.display());
        }
        "restore" => {
            let source =
                PathBuf::from(textarg(arguments, 1, "usage: synapse data restore <file>")?);
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
    let action = textarg(
        arguments,
        0,
        "usage: synapse settings <show|optimize|mesh|learn>",
    )?;
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
            println!(
                "mesh\t{}",
                if runtime.block_on(brain.mesh())? {
                    "on"
                } else {
                    "off"
                }
            );
            println!(
                "learn\t{}",
                if runtime.block_on(brain.learn())? {
                    "on"
                } else {
                    "off"
                }
            );
            println!("shell modes\tcommand-scoped, ambient directory");
            println!("zsh hook\teval \"$(synapse hook zsh)\"");
        }
        "mesh" => {
            let enabled =
                match textarg(arguments, 1, "usage: synapse settings mesh <on|off>")?.as_str() {
                    "on" => true,
                    "off" => false,
                    value => anyhow::bail!("expected `on` or `off`, got `{value}`"),
                };
            runtime.block_on(brain.setmesh(enabled))?;
            println!(
                "Agent mesh {}. Connected tools pick this up the next time they start.",
                if enabled { "on" } else { "off" }
            );
        }
        "learn" => {
            let enabled =
                match textarg(arguments, 1, "usage: synapse settings learn <on|off>")?.as_str() {
                    "on" => true,
                    "off" => false,
                    value => anyhow::bail!("expected `on` or `off`, got `{value}`"),
                };
            runtime.block_on(brain.setlearn(enabled))?;
            match enabled {
                true => println!(
                    "Self-improvement on. Connected tools pick this up the next time they start; \
                     a skill an agent writes waits in `synapse skill proposed` until you approve it."
                ),
                false => println!(
                    "Self-improvement off. Skills already in the library stay where they are."
                ),
            }
        }
        "optimize" => {
            let value = textarg(
                arguments,
                1,
                "usage: synapse settings optimize <full|balanced|lean>",
            )?;
            let optimization = value.parse()?;
            runtime.block_on(brain.setoptimization(optimization))?;
            println!("Recall optimization set to {}", optimization.name());
        }
        _ => anyhow::bail!("unknown settings command `{action}`"),
    }
    Ok(Outcome::Exit(0))
}

fn paths() -> Result<Outcome> {
    println!("data\t{}", crate::files::data()?.display());
    println!("soul\t{}", crate::files::soul()?.display());
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
