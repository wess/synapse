use crate::agent::{Agent, Kind};
use std::fs;
use std::path::{Path, PathBuf};

type Server = (String, Vec<String>);
type Entry = (bool, Option<Server>);

pub fn state(agent: &Agent, server: Option<&Path>) -> (bool, bool) {
    let Ok(content) = fs::read_to_string(&agent.integration) else {
        return (false, false);
    };
    if agent.kind == Kind::Pi {
        return pistate(&content);
    }
    let Some((registered, serverentry)) = entry(agent.kind, &content) else {
        return (false, false);
    };
    let configured = serverentry.is_some_and(|(command, args)| {
        server.is_some_and(|server| samepath(&command, server)) && args == ["mcp"]
    });
    (registered, configured)
}

/// What pi's settings say about the Synapse package.
///
/// There is no server command to compare against this binary here — the
/// extension finds the binary itself at runtime, which is what lets one
/// installed package serve every Synapse on the machine. So *registered* is a
/// package entry that names Synapse's, whichever way it was installed, and
/// *configured* is one that names the source a connection would install now.
fn pistate(content: &str) -> (bool, bool) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return (false, false);
    };
    let Some(packages) = value.get("packages").and_then(|value| value.as_array()) else {
        return (false, false);
    };
    let wanted = super::catalog::pipackage();
    let versioned = format!("{wanted}@");
    packages
        .iter()
        .filter_map(package)
        .fold((false, false), |(registered, configured), source| {
            let ours = source == wanted || source.starts_with(&versioned);
            (
                registered || ours || source.contains(super::catalog::PIPACKAGE),
                configured || ours,
            )
        })
}

/// One `packages` entry, which pi accepts as either a source string or an
/// object wrapping one with per-package filters.
fn package(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(source) => Some(source.clone()),
        value => value
            .get("source")?
            .as_str()
            .map(std::borrow::ToOwned::to_owned),
    }
}

fn entry(kind: Kind, content: &str) -> Option<Entry> {
    match kind {
        Kind::Codex => {
            let value = toml::from_str::<toml::Value>(content).ok()?;
            let value = value
                .get("mcp_servers")
                .and_then(|value| value.get("synapse"));
            Some((value.is_some(), value.and_then(tomlserver)))
        }
        Kind::Claude => {
            let value = serde_json::from_str::<serde_json::Value>(content).ok()?;
            let value = value
                .get("mcpServers")
                .and_then(|value| value.get("synapse"));
            Some((value.is_some(), value.and_then(jsonserver)))
        }
        // Answered by `pistate` before this is reached: a package is not a
        // server entry, and there is no command to compare.
        Kind::Pi => None,
    }
}

fn tomlserver(value: &toml::Value) -> Option<Server> {
    let command = value.get("command")?.as_str()?.to_owned();
    let args = match value.get("args") {
        Some(value) => value
            .as_array()?
            .iter()
            .map(|value| value.as_str().map(ToOwned::to_owned))
            .collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    Some((command, args))
}

fn jsonserver(value: &serde_json::Value) -> Option<Server> {
    let command = value.get("command")?.as_str()?.to_owned();
    let args = match value.get("args") {
        Some(value) => value
            .as_array()?
            .iter()
            .map(|value| value.as_str().map(ToOwned::to_owned))
            .collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    Some((command, args))
}

fn samepath(stored: &str, expected: &Path) -> bool {
    let stored = PathBuf::from(stored);
    stored == expected
        || stored
            .canonicalize()
            .ok()
            .zip(expected.canonicalize().ok())
            .is_some_and(|(stored, expected)| stored == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_codex_and_claude_user_entries() {
        let directory = tempfile::tempdir().unwrap();
        let server = directory.path().join("synapse");
        fs::write(&server, "binary").unwrap();
        let codex = directory.path().join("config.toml");
        fs::write(
            &codex,
            format!(
                "[mcp_servers.synapse]\ncommand = {:?}\nargs = [\"mcp\"]\n",
                server.display().to_string()
            ),
        )
        .unwrap();
        let claude = directory.path().join("claude.json");
        fs::write(
            &claude,
            serde_json::json!({
                "mcpServers": {"synapse": {"command": server, "args": ["mcp"]}}
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(
            state(&agent(Kind::Codex, codex), Some(&server)),
            (true, true)
        );
        assert_eq!(
            state(&agent(Kind::Claude, claude), Some(&server)),
            (true, true)
        );
    }

    #[test]
    fn existing_stale_entry_is_registered_but_not_configured() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "[mcp_servers.synapse]\ncommand = \"/deleted/synapse\"\nargs = [\"mcp\"]\n",
        )
        .unwrap();

        assert_eq!(
            state(
                &agent(Kind::Codex, path),
                Some(Path::new("/current/synapse"))
            ),
            (true, false)
        );
    }

    #[test]
    fn reads_the_pi_package_however_it_was_written() {
        let directory = tempfile::tempdir().unwrap();
        let plain = directory.path().join("plain.json");
        let wrapped = directory.path().join("wrapped.json");
        fs::write(&plain, "{\"packages\":[\"npm:other\",\"npm:synapse-pi\"]}").unwrap();
        fs::write(
            &wrapped,
            "{\"packages\":[{\"source\":\"npm:synapse-pi@0.1.0\",\"skills\":[]}]}",
        )
        .unwrap();

        assert_eq!(state(&agent(Kind::Pi, plain), None), (true, true));
        assert_eq!(state(&agent(Kind::Pi, wrapped), None), (true, true));
    }

    #[test]
    fn a_pi_package_from_somewhere_else_is_registered_but_not_configured() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "{\"packages\":[\"git:github.com/wess/synapse-pi\"]}").unwrap();

        assert_eq!(state(&agent(Kind::Pi, path), None), (true, false));
    }

    #[test]
    fn pi_settings_without_the_package_are_not_a_connection() {
        let directory = tempfile::tempdir().unwrap();
        let empty = directory.path().join("empty.json");
        let other = directory.path().join("other.json");
        fs::write(&empty, "{\"theme\":\"dark\"}").unwrap();
        fs::write(&other, "{\"packages\":[\"npm:pi-web-access\"]}").unwrap();

        assert_eq!(state(&agent(Kind::Pi, empty), None), (false, false));
        assert_eq!(state(&agent(Kind::Pi, other), None), (false, false));
    }

    #[test]
    fn malformed_named_entry_can_still_be_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("claude.json");
        fs::write(&path, "{\"mcpServers\":{\"synapse\":{\"args\":false}}}").unwrap();

        assert_eq!(
            state(
                &agent(Kind::Claude, path),
                Some(Path::new("/current/synapse"))
            ),
            (true, false)
        );
    }

    fn agent(kind: Kind, integration: PathBuf) -> Agent {
        Agent {
            kind,
            name: "Test",
            command: "test",
            instructions: PathBuf::new(),
            settings: PathBuf::new(),
            skills: PathBuf::new(),
            integration,
        }
    }
}
