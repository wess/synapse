//! Everything you would otherwise have to ask someone for.
//!
//! Synapse reports nothing home, so a problem on somebody else's machine is
//! only ever as clear as what they can tell you. This gathers it in one place:
//! what is installed, what is connected, what state the store is in, and what
//! has crashed. `--json` makes it something to paste into an issue.
//!
//! Every check reports rather than fails. A doctor that stops at the first
//! problem is a doctor that cannot describe the machine it was run on, and the
//! first problem is rarely the interesting one.

use crate::cli::Outcome;
use anyhow::Result;
use serde::Serialize;
use std::ffi::OsString;
use std::path::Path;

#[derive(Serialize)]
pub struct Report {
    pub version: &'static str,
    pub paths: Paths,
    pub store: Store,
    pub tools: Vec<Tool>,
    pub skills: Skills,
    pub mesh: Mesh,
    pub shell: String,
    pub cli: String,
    pub crashes: Vec<String>,
}

#[derive(Serialize)]
pub struct Paths {
    pub data: String,
    pub database: String,
    pub guidance: String,
    pub crashlog: String,
}

#[derive(Serialize)]
pub struct Store {
    pub state: String,
    pub version: Option<i64>,
    pub memories: Option<i64>,
    pub megabytes: Option<u64>,
    pub backups: usize,
    pub optimization: Option<String>,
}

#[derive(Serialize)]
pub struct Tool {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub connected: bool,
    /// Connected, and this release would connect it differently — the
    /// descriptor moved after the connection was made. See
    /// [`crate::agent::receipt`].
    pub outdated: bool,
    pub guidance: bool,
    pub notice: bool,
    pub compact: bool,
    pub statusline: String,
}

#[derive(Serialize)]
pub struct Skills {
    pub library: usize,
    pub installed: usize,
    pub stale: usize,
    /// Whether agents may write skills.
    pub learning: bool,
    /// Written by an agent and waiting for the user.
    pub proposed: usize,
    pub problems: Vec<String>,
}

#[derive(Serialize)]
pub struct Mesh {
    pub enabled: bool,
    pub agents: usize,
    pub workers: usize,
}

pub fn doctor(arguments: &[OsString]) -> Result<Outcome> {
    let report = collect();
    if arguments.iter().any(|value| value == "--json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print(&report);
    }
    // A report is a report even when it describes something broken; the exit
    // code says whether the report was produced, not whether it was good news.
    Ok(Outcome::Exit(0))
}

fn print(report: &Report) {
    println!("Synapse {}", report.version);
    println!();
    println!("Store");
    println!("  State          {}", report.store.state);
    if let Some(version) = report.store.version {
        println!("  Schema         v{version}");
    }
    if let Some(memories) = report.store.memories {
        println!("  Memories       {memories}");
    }
    if let Some(megabytes) = report.store.megabytes {
        println!("  Size           {megabytes} MB");
    }
    println!("  Backups        {}", report.store.backups);
    if let Some(optimization) = &report.store.optimization {
        println!("  Recall budget  {optimization}");
    }

    println!();
    println!("Connected tools");
    if report.tools.is_empty() {
        println!("  none detected");
    }
    for tool in &report.tools {
        let state = match (tool.installed, tool.connected, tool.outdated) {
            (false, ..) => "not installed".to_owned(),
            (true, false, _) => "installed, not connected".to_owned(),
            // Worth a line of its own in a bug report: a tool connected under an
            // older descriptor behaves like the release that connected it, not
            // like the one being reported on.
            (true, true, true) => "connected · update available".to_owned(),
            (true, true, false) => "connected".to_owned(),
        };
        println!("  {:<14} {state}", tool.name);
        if let Some(version) = &tool.version {
            println!("    version      {version}");
        }
        if tool.installed {
            println!("    guidance     {}", yesno(tool.guidance));
            println!("    notice       {}", yesno(tool.notice));
            println!("    compaction   {}", yesno(tool.compact));
            println!("    status line  {}", tool.statusline);
        }
    }

    println!();
    println!("Skills");
    println!("  In the library {}", report.skills.library);
    println!("  Installed      {}", report.skills.installed);
    println!("  Out of date    {}", report.skills.stale);
    println!("  Agents write   {}", yesno(report.skills.learning));
    if report.skills.proposed > 0 {
        println!("  Awaiting you   {}", report.skills.proposed);
    }
    for problem in &report.skills.problems {
        println!("  skipped        {problem}");
    }

    println!();
    println!("Mesh");
    println!("  Enabled        {}", yesno(report.mesh.enabled));
    if report.mesh.enabled {
        println!("  Agents         {}", report.mesh.agents);
        println!("  Workers        {}", report.mesh.workers);
    }

    println!();
    println!("Integration");
    println!("  Command line   {}", report.cli);
    println!("  Shell hook     {}", report.shell);

    println!();
    println!("Paths");
    println!("  Data           {}", report.paths.data);
    println!("  Database       {}", report.paths.database);
    println!("  Guidance       {}", report.paths.guidance);
    println!("  Crash log      {}", report.paths.crashlog);

    println!();
    if report.crashes.is_empty() {
        println!("No crashes recorded.");
    } else {
        println!("Recent crashes ({})", report.crashes.len());
        for crash in &report.crashes {
            for line in crash.lines().take(4) {
                println!("  {line}");
            }
            println!();
        }
    }
}

fn yesno(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn collect() -> Report {
    let data = crate::files::data();
    let database = crate::files::database();
    let soul = crate::files::soul();
    let home = crate::files::home();

    Report {
        version: env!("CARGO_PKG_VERSION"),
        paths: Paths {
            data: shown(&data),
            database: shown(&database),
            guidance: shown(&soul),
            crashlog: shown(&crate::crashes::path()),
        },
        store: store(database.as_deref().ok()),
        tools: home.as_deref().map(tools).unwrap_or_default(),
        skills: home.as_deref().map(skills).unwrap_or_else(|_| Skills {
            library: 0,
            installed: 0,
            stale: 0,
            learning: false,
            proposed: 0,
            problems: vec!["could not locate the home directory".to_owned()],
        }),
        mesh: mesh(database.as_deref().ok()),
        shell: shell(),
        cli: cli(),
        crashes: crate::crashes::recent(3),
    }
}

fn shown(path: &Result<std::path::PathBuf>) -> String {
    match path {
        Ok(path) => path.display().to_string(),
        Err(error) => format!("unavailable: {error:#}"),
    }
}

fn store(database: Option<&Path>) -> Store {
    let unknown = |state: String| Store {
        state,
        version: None,
        memories: None,
        megabytes: None,
        backups: 0,
        optimization: None,
    };
    let Some(database) = database else {
        return unknown("could not locate the database".to_owned());
    };
    let backups = crate::database::backupfolder(database)
        .map(|folder| crate::database::snapshots(&folder).len())
        .unwrap_or(0);
    if !database.exists() {
        return Store {
            state: "not created yet".to_owned(),
            backups,
            ..unknown(String::new())
        };
    }
    let megabytes = std::fs::metadata(database)
        .ok()
        .map(|item| item.len() / 1_048_576);

    let gathered = runtime().and_then(|runtime| {
        runtime.block_on(async {
            let report = crate::database::check(database).await?;
            let brain = crate::brain::Brain::glance(database).await?;
            Ok::<_, anyhow::Error>((
                report.version,
                brain.stats().await?.entries,
                brain.settings().await?.optimization,
            ))
        })
    });
    match gathered {
        Ok((version, memories, optimization)) => Store {
            state: "ok".to_owned(),
            version: Some(version),
            memories: Some(memories),
            megabytes,
            backups,
            optimization: Some(format!("{optimization:?}").to_lowercase()),
        },
        Err(error) => Store {
            state: format!("{error:#}"),
            megabytes,
            backups,
            ..unknown(String::new())
        },
    }
}

fn tools(home: &Path) -> Vec<Tool> {
    let server = crate::cli::destination().ok();
    let soul = crate::files::soul().unwrap_or_default();
    // Every check here reports rather than fails, so a database that will not
    // open costs the staleness column and nothing else.
    let rows = crate::files::database()
        .ok()
        .zip(runtime().ok())
        .map(|(database, runtime)| {
            runtime.block_on(crate::agent::connections(
                home,
                server.as_deref(),
                &database,
            ))
        })
        .unwrap_or_default();
    let outdated = |slug: &str| {
        rows.iter()
            .any(|row| row.agent.slug == slug && row.outdated)
    };
    crate::agent::agents(home)
        .into_iter()
        .map(|agent| {
            let detection = crate::agent::detect(&agent, server.as_deref());
            Tool {
                name: agent.name.to_owned(),
                installed: detection.executable.is_some(),
                version: detection.version.clone(),
                connected: detection.configured,
                outdated: outdated(&agent.slug),
                guidance: crate::agent::pointermatches(&agent.instructions, &soul),
                notice: detection.hooks.notice,
                compact: detection.hooks.compact,
                statusline: if detection.hooks.statusline {
                    "Synapse".to_owned()
                } else if detection.hooks.borrowed {
                    "another tool's".to_owned()
                } else {
                    "none".to_owned()
                },
            }
        })
        .collect()
}

fn skills(home: &Path) -> Skills {
    let surveyed = runtime().and_then(|runtime| runtime.block_on(crate::skill::survey(home)));
    let library = crate::skill::library::all()
        .map(|(skills, _)| skills.len())
        .unwrap_or(0);
    let (learning, proposed) = runtime()
        .and_then(|runtime| {
            runtime.block_on(async {
                let database = crate::files::database()?;
                let learning = crate::brain::Brain::glance(&database)
                    .await?
                    .learn()
                    .await?;
                let waiting = crate::skill::Receipts::glance(&database)
                    .await?
                    .waiting()
                    .await?;
                Ok((learning, waiting as usize))
            })
        })
        .unwrap_or((false, 0));
    match surveyed {
        Ok((statuses, problems)) => Skills {
            library,
            learning,
            proposed,
            installed: statuses
                .iter()
                .filter(|status| status.state == crate::skill::State::Installed)
                .count(),
            stale: statuses
                .iter()
                .filter(|status| status.state == crate::skill::State::Stale)
                .count(),
            problems,
        },
        Err(error) => Skills {
            library,
            learning,
            proposed,
            installed: 0,
            stale: 0,
            problems: vec![format!("{error:#}")],
        },
    }
}

fn mesh(database: Option<&Path>) -> Mesh {
    let quiet = Mesh {
        enabled: false,
        agents: 0,
        workers: 0,
    };
    let Some(database) = database.filter(|path| path.exists()) else {
        return quiet;
    };
    let gathered = runtime().and_then(|runtime| {
        runtime.block_on(async {
            let brain = crate::brain::Brain::glance(database).await?;
            if !brain.mesh().await? {
                return Ok::<_, anyhow::Error>(None);
            }
            let mesh = crate::relay::Mesh::glance(database).await?;
            Ok(Some((
                mesh.agents().await?.iter().filter(|a| a.online).count(),
                mesh.workers().await?.len(),
            )))
        })
    });
    match gathered {
        Ok(Some((agents, workers))) => Mesh {
            enabled: true,
            agents,
            workers,
        },
        _ => quiet,
    }
}

fn shell() -> String {
    let Ok(command) = crate::cli::destination() else {
        return "unknown".to_owned();
    };
    match crate::shellsetup::status(&command) {
        Ok(integration) => match integration.state {
            crate::shellsetup::IntegrationState::Installed => {
                format!("{} · installed", integration.shell)
            }
            crate::shellsetup::IntegrationState::Modified => {
                format!("{} · edited since Synapse wrote it", integration.shell)
            }
            crate::shellsetup::IntegrationState::Missing => {
                format!("{} · not installed", integration.shell)
            }
        },
        Err(error) => format!("unknown: {error:#}"),
    }
}

fn cli() -> String {
    match crate::cli::status() {
        Ok(crate::cli::InstallStatus::Installed(path)) => path.display().to_string(),
        Ok(crate::cli::InstallStatus::Missing) => "not installed".to_owned(),
        Ok(crate::cli::InstallStatus::Conflict(path)) => {
            format!("another program at {}", path.display())
        }
        Err(error) => format!("unknown: {error:#}"),
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new().map_err(Into::into)
}
