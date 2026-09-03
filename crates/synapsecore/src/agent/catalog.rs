//! Which tools Synapse can connect to.
//!
//! Every one of them, including the built-ins, is a descriptor resolved
//! through [`crate::agent::tool`]. Nothing is listed here: a tool a person
//! described themselves appears beside the built-ins because it arrived the
//! same way.

use crate::agent::{Agent, Connection, tool};
use std::path::Path;

/// Every connectable tool, built-ins first and then whatever the user or the
/// current project has added.
pub fn agents(home: &Path) -> Vec<Agent> {
    tool::tools(home, projectroot().as_deref())
}

/// Every connectable tool with what this machine found and whether its
/// descriptor has moved, **connected ones first**.
///
/// The order is the whole point of the split the dashboard draws. What is wired
/// in is a shorter list than what could be, it is the list somebody acts on,
/// and burying it among tools they do not have is what the flat list used to
/// do. Within each half the descriptor order is kept, so rows do not shuffle
/// between runs for any reason other than connecting something.
///
/// A read of the connection records that fails leaves every row current rather
/// than every row stale: not knowing is not the same as knowing it is out of
/// date, and the second one nags.
pub async fn connections(home: &Path, server: Option<&Path>, database: &Path) -> Vec<Connection> {
    let root = projectroot();
    let recorded = super::receipt::recorded(database).await.unwrap_or_default();
    let mut rows: Vec<Connection> = agents(home)
        .into_iter()
        .map(|agent| {
            let detection = super::detect(&agent, server);
            let outdated = detection.registered
                && detection.configured
                && recorded
                    .get(&agent.slug)
                    .zip(tool::text(root.as_deref(), &agent.slug).ok())
                    .is_some_and(|(held, (text, _))| *held != super::receipt::digest(&text));
            Connection {
                agent,
                detection,
                outdated,
            }
        })
        .collect();
    rows.sort_by_key(|row| !row.connected());
    rows
}

/// The repository the descriptors of the current directory belong to, so a
/// project can carry the tool its team works in. Absent when the process is not
/// in one, which is not an error — the user and built-in layers still resolve.
pub(crate) fn projectroot() -> Option<std::path::PathBuf> {
    let current = std::env::var_os("SYNAPSE_PROJECT_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    crate::brain::projectroot(&current).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_tools_are_listed_first_and_in_a_stable_order() {
        let home = Path::new("/users/test");
        let agents = agents(home);

        let slugs: Vec<_> = agents.iter().map(|agent| agent.slug.as_str()).collect();
        assert_eq!(&slugs[..4], ["claude", "codex", "pi", "ainz"]);
    }

    /// The dashboard draws two lists out of one vector, so the partition has to
    /// be in the order rather than in the drawing — or the terminal and the
    /// window would each decide it for themselves.
    #[tokio::test]
    async fn connected_tools_sort_ahead_of_the_rest_without_reshuffling_either_half() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let rows = connections(home, None, &directory.path().join("brain.db")).await;

        // Nothing is connected under a home with no tool configuration in it,
        // so the descriptor order survives untouched.
        assert!(rows.iter().all(|row| !row.connected()));
        let slugs: Vec<_> = rows.iter().map(|row| row.agent.slug.as_str()).collect();
        assert_eq!(&slugs[..4], ["claude", "codex", "pi", "ainz"]);
        // And nothing is stale, because nothing was ever recorded.
        assert!(rows.iter().all(|row| !row.outdated));
    }

    #[test]
    fn user_mcp_stores_match_the_tool_clis() {
        let home = Path::new("/users/test");
        let agents = agents(home);
        let find = |slug: &str| {
            agents
                .iter()
                .find(|agent| agent.slug == slug)
                .unwrap()
                .clone()
        };

        assert_eq!(find("codex").integration, home.join(".codex/config.toml"));
        assert_eq!(find("claude").integration, home.join(".claude.json"));
        assert_eq!(find("claude").settings, home.join(".claude/settings.json"));
        // pi keeps its installed packages in the settings file it reads
        // everything else from, so the two are one path.
        assert_eq!(find("pi").integration, home.join(".pi/agent/settings.json"));
        assert_eq!(find("pi").settings, find("pi").integration);
        // Ainz keeps its servers in a file of their own, so unlike pi's the
        // two are not one path. Where that directory is depends on the
        // platform, which is [`tool`]'s business rather than this test's.
        let ainz = find("ainz");
        assert_eq!(ainz.integration.file_name().unwrap(), "mcp.toml");
        assert_eq!(ainz.integration.parent(), ainz.settings.parent());
        assert_ne!(ainz.integration, ainz.settings);
    }
}
