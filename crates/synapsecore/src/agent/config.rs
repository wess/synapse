//! Reading a tool's own configuration back to answer two questions: is Synapse
//! registered at all, and does that registration point at *this* binary.
//!
//! Both answers come from the tool's descriptor rather than from a match on
//! which tool it is — the file format, the key path, and the arguments a
//! Synapse-written entry carries are all written down in `[detect]`.

use crate::agent::Agent;
use crate::agent::tool::{Format, Style};
use std::fs;
use std::path::{Path, PathBuf};

type Server = (String, Vec<String>);

/// `(registered, configured)`. Registered is an entry under Synapse's name;
/// configured is one that names the binary asking. A registration that is not
/// configured is a stale entry pointing at a binary that may no longer exist.
pub fn state(agent: &Agent, server: Option<&Path>) -> (bool, bool) {
    let Ok(content) = fs::read_to_string(&agent.integration) else {
        return (false, false);
    };
    match agent.detect.style {
        Style::Package => packagestate(agent, &content),
        Style::Server => serverstate(agent, &content, server),
    }
}

fn serverstate(agent: &Agent, content: &str, server: Option<&Path>) -> (bool, bool) {
    let Some(value) = value(agent.detect.format, content) else {
        return (false, false);
    };
    let Some(entry) = at(&value, &agent.detect.at) else {
        return (false, false);
    };
    let configured = registration(&entry).is_some_and(|(command, args)| {
        server.is_some_and(|server| samepath(&command, server)) && args == agent.detect.args
    });
    (true, configured)
}

/// What a tool's settings say about a package-style connection.
///
/// There is no server command to compare against this binary: a package finds it
/// at runtime, which is what lets one install serve every Synapse on the
/// machine. So *registered* is an entry naming the package however it was
/// installed, and *configured* is one naming the source a connection installs
/// now.
fn packagestate(agent: &Agent, content: &str) -> (bool, bool) {
    let Some(value) = value(agent.detect.format, content) else {
        return (false, false);
    };
    let Some(entries) = at(&value, &agent.detect.at).and_then(|value| value.array()) else {
        return (false, false);
    };
    let wanted = agent.package();
    let versioned = format!("{wanted}@");
    entries
        .iter()
        .filter_map(package)
        .fold((false, false), |(registered, configured), source| {
            let ours = source == wanted || source.starts_with(&versioned);
            (
                registered || ours || source.contains(&agent.detect.package),
                configured || ours,
            )
        })
}

/// One entry in a package list, which a tool may accept as either a source
/// string or an object wrapping one with per-package settings.
fn package(value: &Value) -> Option<String> {
    match value.string() {
        Some(source) => Some(source),
        None => value.get("source")?.string(),
    }
}

/// The two config formats a tool's settings come in, behind one accessor so the
/// detection rules do not have to be written twice.
enum Value {
    Json(serde_json::Value),
    Toml(toml::Value),
}

impl Value {
    fn get(&self, key: &str) -> Option<Value> {
        match self {
            Self::Json(value) => value.get(key).cloned().map(Value::Json),
            Self::Toml(value) => value.get(key).cloned().map(Value::Toml),
        }
    }

    fn string(&self) -> Option<String> {
        match self {
            Self::Json(value) => value.as_str().map(ToOwned::to_owned),
            Self::Toml(value) => value.as_str().map(ToOwned::to_owned),
        }
    }

    fn array(&self) -> Option<Vec<Value>> {
        match self {
            Self::Json(value) => Some(value.as_array()?.iter().cloned().map(Value::Json).collect()),
            Self::Toml(value) => Some(value.as_array()?.iter().cloned().map(Value::Toml).collect()),
        }
    }
}

fn value(format: Format, content: &str) -> Option<Value> {
    match format {
        Format::Json => serde_json::from_str(content).ok().map(Value::Json),
        Format::Toml => toml::from_str(content).ok().map(Value::Toml),
    }
}

/// Walk a key path such as `["mcpServers", "synapse"]`.
fn at(value: &Value, keys: &[String]) -> Option<Value> {
    keys.iter()
        .try_fold(clone(value), |value, key| value.get(key))
}

fn clone(value: &Value) -> Value {
    match value {
        Value::Json(value) => Value::Json(value.clone()),
        Value::Toml(value) => Value::Toml(value.clone()),
    }
}

/// The command and arguments of a registration, whichever format it is in. A
/// malformed entry has none, which leaves it registered but not configured — so
/// it can still be replaced rather than becoming unfixable.
fn registration(value: &Value) -> Option<Server> {
    let command = value.get("command")?.string()?;
    let args = match value.get("args") {
        Some(value) => value
            .array()?
            .iter()
            .map(Value::string)
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
    use crate::agent::tool;

    fn agent(slug: &str, integration: PathBuf) -> Agent {
        let home = Path::new("/users/test");
        let mut agent = tool::resolve(home, None, slug).unwrap().unwrap();
        agent.integration = integration;
        agent
    }

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

        assert_eq!(state(&agent("codex", codex), Some(&server)), (true, true));
        assert_eq!(state(&agent("claude", claude), Some(&server)), (true, true));
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
            state(&agent("codex", path), Some(Path::new("/current/synapse"))),
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

        assert_eq!(state(&agent("pi", plain), None), (true, true));
        assert_eq!(state(&agent("pi", wrapped), None), (true, true));
    }

    #[test]
    fn a_pi_package_from_somewhere_else_is_registered_but_not_configured() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "{\"packages\":[\"git:github.com/wess/synapse-pi\"]}").unwrap();

        assert_eq!(state(&agent("pi", path), None), (true, false));
    }

    #[test]
    fn pi_settings_without_the_package_are_not_a_connection() {
        let directory = tempfile::tempdir().unwrap();
        let empty = directory.path().join("empty.json");
        let other = directory.path().join("other.json");
        fs::write(&empty, "{\"theme\":\"dark\"}").unwrap();
        fs::write(&other, "{\"packages\":[\"npm:pi-web-access\"]}").unwrap();

        assert_eq!(state(&agent("pi", empty), None), (false, false));
        assert_eq!(state(&agent("pi", other), None), (false, false));
    }

    #[test]
    fn malformed_named_entry_can_still_be_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("claude.json");
        fs::write(&path, "{\"mcpServers\":{\"synapse\":{\"args\":false}}}").unwrap();

        assert_eq!(
            state(&agent("claude", path), Some(Path::new("/current/synapse"))),
            (true, false)
        );
    }

    /// A tool nobody has heard of is read by its own rules, with no code here
    /// that knows its name.
    #[test]
    fn a_described_tool_is_detected_by_its_own_rules() {
        let directory = tempfile::tempdir().unwrap();
        let server = directory.path().join("synapse");
        fs::write(&server, "binary").unwrap();
        let path = directory.path().join("hermes.json");
        fs::write(
            &path,
            serde_json::json!({
                "servers": {"synapse": {"command": server, "args": ["mcp", "--stdio"]}}
            })
            .to_string(),
        )
        .unwrap();
        let text = format!(
            "name = \"Hermes\"\ncommand = \"hermes\"\n\
             [paths]\ninstructions = \"a\"\nsettings = \"b\"\n\
             integration = {:?}\nskills = \"d\"\n\
             [detect]\nformat = \"json\"\nat = [\"servers\", \"synapse\"]\n\
             args = [\"mcp\", \"--stdio\"]\n",
            path.display().to_string()
        );
        let hermes = tool::parsefortest("hermes", &text);

        assert_eq!(state(&hermes, Some(&server)), (true, true));
    }
}
