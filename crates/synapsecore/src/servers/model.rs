use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One MCP server as the library holds it.
///
/// `env` is plain configuration and `secrets` maps a variable to a vault
/// reference (`vault.name`). The two are separate fields so a secret has no
/// way into this file: anything under `env` is written in the clear, and that
/// is the only place a value can be written at all.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Server {
    /// The file name, which is also what every tool calls it.
    #[serde(skip)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Where it came from — a registry entry, or the tool it was adopted from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Variable to vault reference. Empty is a secret the server needs and
    /// nobody has given it yet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<String, String>,
}

impl Server {
    /// What `run` would start, for a person to read. Never carries a value.
    pub fn commandline(&self) -> String {
        std::iter::once(self.command.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(|part| match part.contains(char::is_whitespace) {
                true => format!("{part:?}"),
                false => part.to_owned(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Everything that has to be filled in before the server can start: a
    /// secret with no reference, a required setting with no value, or an
    /// argument the registry left as `<placeholder>`.
    pub fn missing(&self) -> Vec<String> {
        let mut missing: Vec<String> = self
            .secrets
            .iter()
            .filter(|(_, reference)| reference.trim().is_empty())
            .map(|(variable, _)| variable.clone())
            .collect();
        missing.extend(
            self.env
                .iter()
                .filter(|(_, value)| value.is_empty())
                .map(|(variable, _)| variable.clone()),
        );
        missing.extend(self.args.iter().filter(|arg| placeholder(arg)).cloned());
        missing
    }

    pub fn parse(name: &str, text: &str) -> Result<Self> {
        validname(name)?;
        let mut server: Server = toml::from_str(text)
            .with_context(|| format!("could not read the MCP server `{name}`"))?;
        anyhow::ensure!(
            !server.command.trim().is_empty(),
            "`{name}` has no command to start"
        );
        for variable in server.env.keys().chain(server.secrets.keys()) {
            validvariable(variable)?;
        }
        if let Some(variable) = server
            .env
            .keys()
            .find(|key| server.secrets.contains_key(*key))
        {
            anyhow::bail!("`{name}` sets {variable} both as a value and as a secret");
        }
        server.name = name.to_owned();
        Ok(server)
    }

    pub fn text(&self) -> Result<String> {
        toml::to_string_pretty(self).context("could not write the server description")
    }
}

/// A server name: what every tool's config will call it, so it has to be a key
/// all of them accept and a word a person can type.
pub fn validname(name: &str) -> Result<()> {
    anyhow::ensure!(
        !name.is_empty() && name.len() <= 64,
        "a server name needs 1 to 64 characters"
    );
    anyhow::ensure!(
        name.chars().all(|character| character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'),
        "`{name}` is not a server name; use lowercase letters, digits, and hyphens"
    );
    anyhow::ensure!(
        !name.starts_with('-') && !name.ends_with('-'),
        "`{name}` cannot start or end with a hyphen"
    );
    // Every tool already holds an entry called `synapse`, and it is this one.
    anyhow::ensure!(name != "synapse", "`synapse` is Synapse's own entry");
    Ok(())
}

/// The closest valid name to something a tool or the registry called a server.
pub fn slug(raw: &str) -> String {
    let lowered: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| match character.is_ascii_alphanumeric() {
            true => character,
            false => '-',
        })
        .collect();
    let mut slug = String::new();
    for character in lowered.chars() {
        if character == '-' && (slug.is_empty() || slug.ends_with('-')) {
            continue;
        }
        slug.push(character);
    }
    slug.trim_end_matches('-')
        .chars()
        .take(64)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned()
}

fn validvariable(variable: &str) -> Result<()> {
    let mut characters = variable.chars();
    anyhow::ensure!(
        characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            && characters.all(|character| character.is_ascii_alphanumeric() || character == '_'),
        "`{variable}` is not an environment variable name"
    );
    Ok(())
}

pub(super) fn placeholder(arg: &str) -> bool {
    arg.len() > 2 && arg.starts_with('<') && arg.ends_with('>')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_reads_back_what_it_wrote() {
        let mut server = Server {
            name: "github".to_owned(),
            description: "GitHub's API".to_owned(),
            command: "npx".to_owned(),
            args: vec![
                "-y".to_owned(),
                "@modelcontextprotocol/server-github".to_owned(),
            ],
            ..Server::default()
        };
        server.env.insert("LOG_LEVEL".to_owned(), "info".to_owned());
        server.secrets.insert(
            "GITHUB_TOKEN".to_owned(),
            "mcp-github.github-token".to_owned(),
        );
        let text = server.text().unwrap();
        assert_eq!(Server::parse("github", &text).unwrap(), server);
        assert!(server.missing().is_empty());
        assert_eq!(
            server.commandline(),
            "npx -y @modelcontextprotocol/server-github"
        );
    }

    #[test]
    fn what_is_still_needed_is_listed() {
        let text = "command = \"npx\"\nargs = [\"pkg\", \"<allowed-directories>\"]\n[env]\nROOT = \"\"\n[secrets]\nTOKEN = \"\"\n";
        let server = Server::parse("files", text).unwrap();
        assert_eq!(
            server.missing(),
            vec!["TOKEN", "ROOT", "<allowed-directories>"]
        );
    }

    #[test]
    fn a_variable_cannot_be_both_a_value_and_a_secret() {
        let text = "command = \"x\"\n[env]\nTOKEN = \"plain\"\n[secrets]\nTOKEN = \"v.t\"\n";
        assert!(Server::parse("x", text).is_err());
    }

    #[test]
    fn names_are_ones_every_tool_accepts() {
        assert!(validname("github").is_ok());
        assert!(validname("context7").is_ok());
        for bad in ["", "GitHub", "my_server", "-x", "x-", "synapse", "a/b"] {
            assert!(validname(bad).is_err(), "{bad} should be refused");
        }
        assert_eq!(
            slug("io.github.j0hanz/Filesystem_MCP"),
            "io-github-j0hanz-filesystem-mcp"
        );
        assert_eq!(slug("My Server!"), "my-server");
    }
}
