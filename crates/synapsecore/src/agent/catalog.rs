//! Which tools Synapse can connect to.
//!
//! Every one of them, including the three that ship, is a descriptor resolved
//! through [`crate::agent::tool`]. Nothing is listed here: a tool a person
//! described themselves appears beside the built-ins because it arrived the
//! same way.

use crate::agent::{Agent, tool};
use std::path::Path;

/// Every connectable tool, built-ins first and then whatever the user or the
/// current project has added.
pub fn agents(home: &Path) -> Vec<Agent> {
    tool::tools(home, projectroot().as_deref())
}

/// The repository the descriptors of the current directory belong to, so a
/// project can carry the tool its team works in. Absent when the process is not
/// in one, which is not an error — the user and built-in layers still resolve.
fn projectroot() -> Option<std::path::PathBuf> {
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
    fn the_three_that_ship_are_listed_first_and_in_a_stable_order() {
        let home = Path::new("/users/test");
        let agents = agents(home);

        let slugs: Vec<_> = agents.iter().map(|agent| agent.slug.as_str()).collect();
        assert_eq!(&slugs[..3], ["claude", "codex", "pi"]);
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
    }
}
