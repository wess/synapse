//! `synapse mux` — one terminal at the head of a team.
//!
//! The mesh already carries everything needed to talk to a running agent. What
//! it has never had is a *person* on it. Without one, the only way to reach a
//! worker is through a lead agent relaying for you, which costs that agent's
//! context on every message and makes it a bottleneck for a job it is not doing.
//!
//! So the multiplexer is not new infrastructure. It registers you on the mesh
//! under your own name and gives you a keyboard: the same `send`, `post`,
//! `broadcast`, and inbox every agent already uses. Two consequences fall out of
//! that. You can address any agent directly, without a proxy. And an agent that
//! gets stuck can address *you* — which, for a headless worker running with its
//! permission prompts turned off, is the difference between asking and guessing.
//!
//! It is deliberately line-oriented rather than a full-screen interface: it adds
//! no dependency, it works over ssh, and it keeps a transcript you can scroll
//! back through in the terminal you already had.

use crate::cli::Outcome;
use crate::cli::relay::{directory, value, values};
use crate::relay::{self, Mesh, MessageKind, Supervisor};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

const USAGE: &str = "usage: synapse mux [--as <name>] [--team <team>] [--channel <name>]... \
     [--directory <folder>]";

/// How often the reader re-checks the inbox and the roster. Short enough that a
/// reply feels immediate, long enough that an idle mux is not a busy loop.
const TICK: Duration = Duration::from_millis(500);

/// How many lines of a worker's log `/log` renders.
const LOGLINES: usize = 40;

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    // Every option here takes a value, so a bare word is a team or a name the
    // person expected to be positional. Saying so beats joining the mesh under
    // a default name and looking like it worked.
    if let Some(stray) = stray(arguments) {
        anyhow::bail!("unexpected argument `{stray}`\n\n{USAGE}");
    }
    let root = directory(arguments)?;
    let name = match value(arguments, "--as") {
        Some(name) => relay::store::validname(&name)?,
        None => relay::console::whoami(),
    };
    let channels = values(arguments, "--channel");
    let team = value(arguments, "--team");
    crate::cli::launch::requiremesh()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(session(&name, &root, &channels, team.as_deref()))
}

async fn session(
    name: &str,
    root: &Path,
    channels: &[String],
    team: Option<&str>,
) -> Result<Outcome> {
    let mesh = Mesh::open(crate::files::database()?).await?;
    let supervisor = Supervisor::new();

    // A name already answering on the mesh is somebody else's inbox. Draining
    // it from here would take their work and leave them parked forever — which
    // `arrive` refuses, along with being the only place `human` is set.
    relay::console::arrive(&mesh, name, root, "synapse mux")
        .await
        .with_context(|| format!("could not join the mesh as `{name}`"))?;
    for channel in channels {
        mesh.subscribe(name, channel).await?;
    }

    if let Some(team) = team {
        start(&mesh, &supervisor, team, root).await?;
    }

    println!("You are `{name}` on the mesh. /help for commands, /quit to leave.");
    roster(&mesh).await?;

    let outcome = drive(&mesh, &supervisor, name).await;

    // Leaving takes the workers this session started with it, and takes the
    // name off the roster so nothing addresses an empty terminal.
    supervisor.stopall(&mesh).await;
    let _ = mesh.forget(name).await;
    println!("Left the mesh.");
    outcome
}

/// Start every member of a team as a supervised worker. Unlike `relay team
/// open`, no member becomes the lead: the person in this terminal is the lead.
async fn start(mesh: &Mesh, supervisor: &Supervisor, team: &str, root: &Path) -> Result<()> {
    let resolved = relay::team::resolve(Some(root), team)?
        .with_context(|| format!("no team named `{team}`"))?;
    for member in &resolved.members {
        let role = member.role.clone().unwrap_or_else(|| "worker".to_owned());
        let built = relay::launch(&relay::Options {
            name: Some(&member.name),
            role: &role,
            root,
            tool: member.tool.as_deref(),
            task: None,
            channels: &[],
            tools: &[],
            model: None,
            lead: false,
            optimize: false,
            headless: true,
            skippermissions: true,
            strict: false,
            command: None,
            extra: &[],
        })?;
        supervisor
            .launch(
                mesh,
                relay::Spec {
                    name: member.name.clone(),
                    role,
                    program: built.program,
                    arguments: built.arguments,
                    environment: built.environment,
                    directory: root.to_path_buf(),
                    keepalive: true,
                    session: built.session,
                },
            )
            .await?;
        println!("Started {} ({})", member.name, built.tool);
    }
    Ok(())
}

/// The loop: incoming messages print as they land, typed lines go out.
///
/// stdin blocks, so it is read on its own thread and delivered over a channel.
/// That keeps one `select` able to serve both halves without either starving the
/// other, and without a terminal library.
async fn drive(mesh: &Mesh, supervisor: &Supervisor, me: &str) -> Result<Outcome> {
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });

    let mut focus: Option<String> = None;
    prompt(&focus);
    loop {
        // Anything addressed to us, printed above the prompt line.
        let pending = mesh.pending(me).await?;
        if let Some(last) = pending.last() {
            println!();
            for message in &pending {
                println!("{}", incoming(message));
            }
            mesh.ack(me, last.id).await?;
            prompt(&focus);
        }
        let _ = mesh.touch(me).await;

        match lines.try_recv() {
            Ok(line) => {
                if !handle(mesh, supervisor, me, &mut focus, line.trim()).await? {
                    return Ok(Outcome::Exit(0));
                }
                prompt(&focus);
            }
            // stdin closed: a piped script that ran out, or a closed terminal.
            Err(mpsc::TryRecvError::Disconnected) => return Ok(Outcome::Exit(0)),
            Err(mpsc::TryRecvError::Empty) => tokio::time::sleep(TICK).await,
        }
    }
}

/// Act on one typed line. Returns whether the loop should keep running.
async fn handle(
    mesh: &Mesh,
    supervisor: &Supervisor,
    me: &str,
    focus: &mut Option<String>,
    line: &str,
) -> Result<bool> {
    // Addressing is `relay::console`'s, not this file's. The desktop Console
    // reads the same grammar, and two copies of it would mean two grammars.
    let (kind, target, body) = match relay::console::read(line, focus.as_deref()) {
        relay::console::Line::Blank => return Ok(true),
        relay::console::Line::Command(rest) => {
            return slash(mesh, supervisor, me, focus, &rest).await;
        }
        relay::console::Line::Empty => {
            eprintln!("nothing to send");
            return Ok(true);
        }
        relay::console::Line::Undirected => {
            eprintln!("{}", relay::console::UNDIRECTED);
            return Ok(true);
        }
        relay::console::Line::Message { kind, target, body } => (kind, target, body),
    };
    let body = body.as_str();
    // A direct message to a name nobody answers to is held for whoever
    // registers under it later, which is what lets a supervisor brief a worker
    // it is still starting. It is also what a typo looks like, so say so.
    if kind == MessageKind::Direct
        && let Some(name) = target.as_deref()
        && !mesh
            .agents()
            .await?
            .iter()
            .any(|agent| agent.name == name && agent.registered)
    {
        eprintln!("note: nobody is registered as `{name}` yet — holding it for them");
    }
    match relay::deliver(mesh, me, kind, target.as_deref(), body).await {
        Ok(_) => println!("  → {}", addressed(kind, target.as_deref())),
        Err(error) => eprintln!("could not send: {error:#}"),
    }
    Ok(true)
}

async fn slash(
    mesh: &Mesh,
    supervisor: &Supervisor,
    me: &str,
    focus: &mut Option<String>,
    line: &str,
) -> Result<bool> {
    let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    match verb {
        "quit" | "q" | "exit" => return Ok(false),
        "help" | "?" => println!("{HELP}"),
        "agents" | "who" => roster(mesh).await?,
        "channels" => {
            for channel in mesh.channels().await? {
                println!(
                    "  #{}  {} subscriber(s)",
                    channel.channel, channel.subscribers
                );
            }
        }
        "workers" | "ps" => {
            for worker in mesh.workers().await? {
                println!(
                    "  {}  {}  pid {}",
                    worker.name, worker.status, worker.process
                );
            }
        }
        "focus" => {
            if rest.is_empty() {
                *focus = None;
                println!("  focus cleared");
            } else {
                *focus = Some(rest.to_owned());
                println!("  focused on {rest}");
            }
        }
        "join" => {
            mesh.subscribe(me, rest).await?;
            println!("  joined #{rest}");
        }
        "leave" => {
            mesh.unsubscribe(me, rest).await?;
            println!("  left #{rest}");
        }
        "log" => match mesh.worker(rest).await? {
            Some(worker) => tail(Path::new(&worker.log)),
            None => eprintln!("no worker named `{rest}`"),
        },
        "kill" => match supervisor.stop(mesh, rest).await {
            Ok(_) => println!("  stopped {rest}"),
            Err(error) => eprintln!("could not stop {rest}: {error:#}"),
        },
        unknown => eprintln!("unknown command `/{unknown}` — /help lists them"),
    }
    Ok(true)
}

const HELP: &str = "  @name text     send to one agent
  #channel text  post to a channel
  !text          send to everyone
  text           send to the focused agent
  /focus [name]  set or clear the focus
  /agents        who is on the mesh
  /channels      channels and subscriber counts
  /workers       background workers and their state
  /log <name>    recent activity from a worker
  /join <name>   subscribe to a channel
  /leave <name>  unsubscribe
  /kill <name>   stop a worker
  /quit          leave the mesh";

async fn roster(mesh: &Mesh) -> Result<()> {
    for agent in mesh.agents().await? {
        let state = if agent.human {
            "you"
        } else if agent.online {
            "online"
        } else {
            "offline"
        };
        // The note is what someone reading this actually wants: the state says
        // a worker has not stalled, the note says whether to leave it alone.
        let doing = match (agent.status.is_empty(), agent.note.is_empty()) {
            (true, true) => "-".to_owned(),
            (false, true) => agent.status.clone(),
            (true, false) => agent.note.clone(),
            (false, false) => format!("{} · {}", agent.status, agent.note),
        };
        println!(
            "  {:<16} {:<9} {:<10} {}",
            agent.name,
            state,
            if agent.role.is_empty() {
                "-"
            } else {
                &agent.role
            },
            doing
        );
    }
    Ok(())
}

/// Render the tail of a worker's log.
///
/// Workers run with `--output-format stream-json`, so the log is a structured
/// event stream rather than a terminal recording. That is what makes this
/// readable without a pty: pull the human-meaningful part out of each event and
/// drop the rest.
fn tail(log: &Path) {
    let Ok(content) = std::fs::read_to_string(log) else {
        eprintln!("no log at {}", log.display());
        return;
    };
    let lines: Vec<&str> = content.lines().collect();
    let from = lines.len().saturating_sub(LOGLINES * 4);
    let mut shown = 0;
    for line in &lines[from..] {
        if let Some(rendered) = event(line) {
            println!("  {rendered}");
            shown += 1;
        }
    }
    if shown == 0 {
        println!("  (nothing readable in {} yet)", log.display());
    }
}

/// One stream-json event as a line, or nothing when it carries no news.
fn event(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let content = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_array())?;
    let mut parts = Vec::new();
    for item in content {
        match item.get("type").and_then(serde_json::Value::as_str) {
            Some("text") => {
                let text = item.get("text").and_then(serde_json::Value::as_str)?;
                let text = text.trim();
                if !text.is_empty() {
                    parts.push(text.lines().next().unwrap_or_default().to_owned());
                }
            }
            Some("tool_use") => {
                let name = item
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("tool");
                parts.push(format!("· {name}"));
            }
            _ => {}
        }
    }
    (!parts.is_empty()).then(|| parts.join("  "))
}

fn incoming(message: &relay::Message) -> String {
    let via = match (message.kind, message.target.as_deref()) {
        (MessageKind::Channel, Some(channel)) => format!(" #{channel}"),
        (MessageKind::Broadcast, _) => " (everyone)".to_owned(),
        _ => String::new(),
    };
    format!("{}{via}: {}", message.sender, message.body)
}

fn addressed(kind: MessageKind, target: Option<&str>) -> String {
    match (kind, target) {
        (MessageKind::Direct, Some(name)) => name.to_owned(),
        (MessageKind::Channel, Some(name)) => format!("#{name}"),
        _ => "everyone".to_owned(),
    }
}

fn prompt(focus: &Option<String>) {
    if !std::io::stdin().is_terminal() {
        return;
    }
    match focus {
        Some(name) => print!("{name}> "),
        None => print!("> "),
    }
    let _ = std::io::stdout().flush();
}

/// The first argument that is neither a flag nor the value of one.
fn stray(arguments: &[OsString]) -> Option<String> {
    let mut index = 0;
    while index < arguments.len() {
        let value = arguments[index].to_string_lossy();
        if value.starts_with("--") {
            // Every flag this command takes carries a value.
            index += 2;
            continue;
        }
        return Some(value.into_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_json_event_renders_as_one_readable_line() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"text","text":"Looking at the migration now.\nSecond line ignored."},
            {"type":"tool_use","name":"Read"}]}}"#;
        let rendered = event(&line.replace('\n', "")).unwrap();
        assert!(rendered.contains("Looking at the migration now."));
        assert!(rendered.contains("· Read"));
        assert!(
            !rendered.contains("Second line"),
            "only the first line of a block belongs on a roster line"
        );
    }

    #[test]
    fn events_carrying_no_news_are_dropped_rather_than_printed_empty() {
        assert!(event("not json at all").is_none());
        assert!(event(r#"{"type":"system","subtype":"init"}"#).is_none());
        assert!(event(r#"{"message":{"content":[{"type":"text","text":"  "}]}}"#).is_none());
    }

    #[test]
    fn addressing_reads_back_the_way_it_was_typed() {
        assert_eq!(addressed(MessageKind::Direct, Some("backend")), "backend");
        assert_eq!(addressed(MessageKind::Channel, Some("build")), "#build");
        assert_eq!(addressed(MessageKind::Broadcast, None), "everyone");
    }
}
