use crate::agent::{Agent, Kind};
use std::path::{Path, PathBuf};

pub fn agents(home: &Path) -> Vec<Agent> {
    let codex = codexhome(home);
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
    ]
}

fn codexhome(home: &Path) -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"))
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
    }

    #[test]
    fn each_tool_reads_personal_skills_from_its_own_folder() {
        let home = Path::new("/users/test");
        let agents = agents(home);

        assert_eq!(agents[0].skills, home.join(".agents/skills"));
        assert_eq!(agents[1].skills, home.join(".claude/skills"));
    }
}
