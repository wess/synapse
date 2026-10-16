//! Giving a tool one of the library's servers, and taking it back.
//!
//! The tool's own CLI does the writing, exactly as it does for Synapse's own
//! connection: `mcp add` with the name and `synapse mcp run <name>`. Whether an
//! entry is Synapse's is read from the entry itself — those three arguments —
//! so there is no receipt to drift from what the file says.

use crate::agent::Agent;
use crate::agent::config::{self, Entry};
use crate::servers::model::Server;
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Holding {
    /// The tool starts it through Synapse, from this binary.
    Installed,
    /// Through Synapse, but from a binary that has moved.
    Stale,
    /// The tool has a server by this name that Synapse did not write.
    Foreign,
    Missing,
}

impl Holding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Stale => "update available",
            Self::Foreign => "not ours",
            Self::Missing => "not installed",
        }
    }
}

/// One server in one tool.
#[derive(Clone, Debug, Serialize)]
pub struct Placement {
    pub server: String,
    pub tool: String,
    pub slug: String,
    pub holding: Holding,
}

/// Where every library server stands in every tool that can hold one.
pub fn survey(agents: &[Agent], servers: &[Server], binary: &Path) -> Vec<Placement> {
    let mut placements = Vec::new();
    for agent in agents.iter().filter(|agent| agent.servers.supported()) {
        let held = config::servers(agent, &agent.servers.at);
        for server in servers {
            placements.push(Placement {
                server: server.name.clone(),
                tool: agent.name.clone(),
                slug: agent.slug.clone(),
                holding: holding(held.iter().find(|(name, _)| *name == server.name), binary),
            });
        }
    }
    placements
}

fn holding(held: Option<&(String, Entry)>, binary: &Path) -> Holding {
    let Some((name, entry)) = held else {
        return Holding::Missing;
    };
    if !synapses(name, entry) {
        return Holding::Foreign;
    }
    match config::samepath(&entry.command, binary) {
        true => Holding::Installed,
        false => Holding::Stale,
    }
}

/// An entry Synapse wrote: `synapse mcp run <its own name>`, from a binary
/// called synapse. Anything else is somebody's, however it is spelled.
fn synapses(name: &str, entry: &Entry) -> bool {
    let stem = Path::new(&entry.command)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    matches!(stem, "synapse" | "synapse-cli") && entry.args == ["mcp", "run", name]
}

/// The names of every entry in this tool that Synapse wrote, library server or
/// not — what `disconnect` and `uninstall` take back.
pub fn ours(agent: &Agent) -> Vec<String> {
    if !agent.servers.supported() {
        return Vec::new();
    }
    config::servers(agent, &agent.servers.at)
        .into_iter()
        .filter(|(name, entry)| synapses(name, entry))
        .map(|(name, _)| name)
        .collect()
}

/// Give `agent` this server. A stale entry of ours is replaced; an entry that
/// is somebody else's is left alone unless `replace` says to swap it, because
/// it is theirs.
pub fn install(agent: &Agent, server: &Server, binary: &Path, replace: bool) -> Result<Holding> {
    anyhow::ensure!(
        agent.servers.supported(),
        "{} has no MCP client Synapse can give a server to",
        agent.name
    );
    let held = config::servers(agent, &agent.servers.at);
    let current = holding(held.iter().find(|(name, _)| *name == server.name), binary);
    if current == Holding::Installed {
        return Ok(current);
    }
    anyhow::ensure!(
        current != Holding::Foreign || replace,
        "{} already has its own `{}` server; replace it to have it read from the library instead",
        agent.name,
        server.name
    );
    let executable = executable(agent)?;
    let snapshot = crate::files::Snapshot::capture(&agent.integration)?;
    let outcome = (|| {
        if current != Holding::Missing {
            runtool(
                &executable,
                &agent.servers.remove,
                agent,
                &server.name,
                binary,
            )
            .with_context(|| format!("could not take out the old `{}`", server.name))?;
        }
        runtool(&executable, &agent.servers.add, agent, &server.name, binary)
    })();
    if let Err(error) = outcome {
        // Half a swap is the worst result: the old entry gone and no new one.
        let _ = snapshot.restore();
        return Err(error);
    }
    Ok(Holding::Installed)
}

/// Take one of Synapse's entries out of a tool. Somebody else's entry is never
/// removed from here, and one that is not there is not an error.
pub fn uninstall(agent: &Agent, name: &str, binary: &Path) -> Result<bool> {
    if !agent.servers.supported() {
        return Ok(false);
    }
    let held = config::servers(agent, &agent.servers.at);
    match holding(held.iter().find(|(entry, _)| entry == name), binary) {
        Holding::Missing => return Ok(false),
        Holding::Foreign => anyhow::bail!(
            "{}'s `{name}` server is its own, not one Synapse installed; it is left alone",
            agent.name
        ),
        Holding::Installed | Holding::Stale => {}
    }
    let executable = executable(agent)?;
    runtool(&executable, &agent.servers.remove, agent, name, binary)?;
    Ok(true)
}

fn executable(agent: &Agent) -> Result<std::path::PathBuf> {
    crate::agent::detect(agent, None)
        .executable
        .with_context(|| format!("{} is not installed or is not on PATH", agent.name))
}

fn runtool(
    executable: &Path,
    template: &[String],
    agent: &Agent,
    name: &str,
    binary: &Path,
) -> Result<()> {
    let binary = binary.display().to_string();
    let argv: Vec<String> = template
        .iter()
        .map(|token| token.replace("{name}", name).replace("{server}", &binary))
        .collect();
    let output = crate::agent::command(executable)
        .args(&argv)
        .output()
        .with_context(|| format!("could not run {}", agent.name))?;
    anyhow::ensure!(
        output.status.success(),
        "{} said: {}",
        agent.name,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(command: &str, args: &[&str]) -> Entry {
        Entry {
            command: command.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            ..Entry::default()
        }
    }

    #[test]
    fn an_entry_is_ours_only_when_it_runs_this_server_through_synapse() {
        let binary = Path::new("/bin/synapse");
        let held = |name: &str, entry: Entry| holding(Some(&(name.to_owned(), entry)), binary);
        assert_eq!(
            held("github", entry("/bin/synapse", &["mcp", "run", "github"])),
            Holding::Installed
        );
        assert_eq!(
            held("github", entry("/old/synapse", &["mcp", "run", "github"])),
            Holding::Stale
        );
        // Somebody's own entry, and one of ours filed under another name.
        assert_eq!(
            held("github", entry("npx", &["-y", "server-github"])),
            Holding::Foreign
        );
        assert_eq!(
            held("github", entry("/bin/synapse", &["mcp", "run", "gitlab"])),
            Holding::Foreign
        );
        assert_eq!(holding(None, binary), Holding::Missing);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_swap_puts_the_tool_back_the_way_it_was() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("claude.json");
        let original = r#"{"mcpServers":{"github":{"command":"npx","args":["server-github"],"env":{"TOKEN":"kept"}}}}"#;
        std::fs::write(&config, original).unwrap();
        // Removes the entry, then fails to add one.
        let fake = directory.path().join("fake");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nif [ \"$2\" = remove ]; then printf '{{}}' > '{}'; exit 0; fi\nexit 1\n",
                config.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();

        let mut agent = crate::agent::tool::parsefortest(
            "fake",
            &format!(
                "name = \"Fake\"\ncommand = {:?}\n[paths]\ninstructions = \"a\"\nsettings = \"b\"\nintegration = {:?}\nskills = \"c\"\n[servers]\nadd = [\"x\", \"add\", \"{{name}}\"]\nremove = [\"x\", \"remove\", \"{{name}}\"]\nat = [\"mcpServers\", \"{{name}}\"]\n",
                fake.display().to_string(),
                config.display().to_string()
            ),
        );
        agent.integration = config.clone();
        let server = Server {
            name: "github".to_owned(),
            command: "npx".to_owned(),
            ..Server::default()
        };

        assert!(install(&agent, &server, Path::new("/bin/synapse"), false).is_err());
        assert!(install(&agent, &server, Path::new("/bin/synapse"), true).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
    }
}
