use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// skills.sh, or anything answering its `/api/search` and `/api/download`.
    SkillsIndex,
    /// The MCP Registry API: `/v0/servers`.
    McpRegistry,
    /// A repository's `SKILL.md` folders and `server.json` files.
    Github,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::SkillsIndex => "skills index",
            Self::McpRegistry => "MCP registry",
            Self::Github => "GitHub repository",
        }
    }
}

/// Which layer a source came from.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase", tag = "layer")]
pub enum Origin {
    #[default]
    Builtin,
    User,
    /// A repository's own file. `trusted` is whether its exact contents were
    /// approved; an untrusted one is listed and never searched.
    Project {
        file: PathBuf,
        trusted: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub name: String,
    pub kind: Kind,
    /// An index or registry's base address.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// `owner/repo`, for a GitHub source.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repo: String,
    #[serde(default, rename = "ref", skip_serializing_if = "String::is_empty")]
    pub reference: String,
    /// A folder inside the repository to look under.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// A vault reference (`vault.name`) for a private repository's token. The
    /// token itself never appears in a sources file.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(skip)]
    pub origin: Origin,
}

impl Source {
    pub fn skills(&self) -> bool {
        matches!(self.kind, Kind::SkillsIndex | Kind::Github)
    }

    pub fn servers(&self) -> bool {
        matches!(self.kind, Kind::McpRegistry | Kind::Github)
    }

    /// Searched at all: everything but a project source nobody approved.
    pub fn usable(&self) -> bool {
        !matches!(self.origin, Origin::Project { trusted: false, .. })
    }

    /// Where it is, as a person would write it.
    pub fn location(&self) -> String {
        match self.kind {
            Kind::Github => {
                let mut location = self.repo.clone();
                if !self.reference.is_empty() {
                    location.push('@');
                    location.push_str(&self.reference);
                }
                if !self.path.is_empty() {
                    location.push('/');
                    location.push_str(&self.path);
                }
                location
            }
            _ => self.url.clone(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        validname(&self.name)?;
        match self.kind {
            Kind::Github => {
                let parts: Vec<&str> = self.repo.split('/').collect();
                anyhow::ensure!(
                    parts.len() == 2 && parts.iter().all(|part| repopart(part)),
                    "`{}` needs a repository as owner/repo",
                    self.name
                );
                anyhow::ensure!(
                    self.path.split('/').all(|part| part != ".."),
                    "`{}` names a folder outside its repository",
                    self.name
                );
            }
            Kind::SkillsIndex | Kind::McpRegistry => anyhow::ensure!(
                self.url.starts_with("https://")
                    || self.url.starts_with("http://localhost")
                    || self.url.starts_with("http://127.0.0.1"),
                "`{}` needs an https address",
                self.name
            ),
        }
        if !self.token.is_empty() {
            anyhow::ensure!(
                self.token
                    .split_once('.')
                    .is_some_and(|(vault, name)| !vault.is_empty() && !name.is_empty()),
                "`{}` names its token as `{}`, which is not a vault reference (vault.name)",
                self.name,
                self.token
            );
        }
        Ok(())
    }
}

/// A source name: what a person types before the colon in `team:release`.
pub fn validname(name: &str) -> Result<()> {
    anyhow::ensure!(
        !name.is_empty()
            && name.len() <= 40
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character)),
        "`{name}` is not a source name; use letters, digits, `-`, `_`, and `.`"
    );
    Ok(())
}

fn repopart(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}
