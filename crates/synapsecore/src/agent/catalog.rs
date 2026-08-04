use crate::agent::{Agent, Kind};
use std::path::{Path, PathBuf};

pub fn agents(home: &Path) -> Vec<Agent> {
    let codex = codexhome(home);
    let pi = pihome(home);
    vec![
        Agent {
            kind: Kind::Codex,
            name: "Codex",
            command: "codex",
            instructions: codex.join("AGENTS.md"),
            settings: codex.join("config.toml"),
            integration: codex.join("config.toml"),
            // Codex reads personal skills from the shared Agent Skills folder,
            // not from its own home. `.codex/skills` holds the set it ships.
            skills: home.join(".agents").join("skills"),
        },
        Agent {
            kind: Kind::Claude,
            name: "Claude Code",
            command: "claude",
            instructions: home.join(".claude").join("CLAUDE.md"),
            settings: home.join(".claude").join("settings.json"),
            integration: home.join(".claude.json"),
            skills: home.join(".claude").join("skills"),
        },
        Agent {
            kind: Kind::Pi,
            name: "pi",
            command: "pi",
            // pi has no global instruction file of the kind the other two read.
            // What it has is a system prompt it appends to every session, which
            // is the same job: content the user owns, read every time, and the
            // right place for a managed block pointing at SOUL.md.
            instructions: pi.join("APPEND_SYSTEM.md"),
            settings: pi.join("settings.json"),
            // One file for both, because pi records an installed package in the
            // settings it reads everything else from.
            integration: pi.join("settings.json"),
            skills: pi.join("skills"),
        },
    ]
}

fn codexhome(home: &Path) -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"))
}

/// The package a connection to pi installs.
///
/// pi has no MCP client, so the connection is a package that carries one, and
/// the source string is what both setup and detection compare against. The
/// override exists for the same reason every other real path has one: without
/// it the integration tests would have to reach npm, and somebody working on
/// the package could not point a connection at their checkout.
pub(super) fn pipackage() -> String {
    std::env::var("SYNAPSE_PI_PACKAGE")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("npm:{PIPACKAGE}"))
}

/// The npm name, which is also how a package installed some other way — from a
/// checkout, from git — is recognised as this one.
pub(super) const PIPACKAGE: &str = "synapse-pi";

fn pihome(home: &Path) -> PathBuf {
    std::env::var_os("PI_CODING_AGENT_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".pi").join("agent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_mcp_stores_match_the_tool_clis() {
        let home = Path::new("/users/test");
        let agents = agents(home);

        assert_eq!(agents[0].integration, home.join(".codex/config.toml"));
        assert_eq!(agents[1].integration, home.join(".claude.json"));
        assert_eq!(agents[1].settings, home.join(".claude/settings.json"));
        // pi keeps its installed packages in the settings file it reads
        // everything else from, so the two are one path.
        assert_eq!(agents[2].integration, home.join(".pi/agent/settings.json"));
        assert_eq!(agents[2].settings, agents[2].integration);
    }

    #[test]
    fn each_tool_reads_personal_skills_from_its_own_folder() {
        let home = Path::new("/users/test");
        let agents = agents(home);

        assert_eq!(agents[0].skills, home.join(".agents/skills"));
        assert_eq!(agents[1].skills, home.join(".claude/skills"));
        assert_eq!(agents[2].skills, home.join(".pi/agent/skills"));
    }

    #[test]
    fn pi_appends_its_guidance_to_the_system_prompt_it_already_reads() {
        let home = Path::new("/users/test");
        let agents = agents(home);

        assert_eq!(
            agents[2].instructions,
            home.join(".pi/agent/APPEND_SYSTEM.md")
        );
    }
}
