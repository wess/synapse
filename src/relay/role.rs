//! A role is a reusable identity an agent launches with: a brief describing what
//! it owns and how it coordinates, plus optional defaults. It is durable, unlike
//! the one-off `--task` focus. Resolved project → user → built-in.

use crate::relay::layer::{self, Source};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

const KIND: &str = "roles";

const BUILTINS: &[(&str, &str)] = &[
    (
        "supervisor",
        include_str!("../../assets/roles/supervisor.toml"),
    ),
    ("worker", include_str!("../../assets/roles/worker.toml")),
    ("frontend", include_str!("../../assets/roles/frontend.toml")),
    ("backend", include_str!("../../assets/roles/backend.toml")),
    ("reviewer", include_str!("../../assets/roles/reviewer.toml")),
    ("devops", include_str!("../../assets/roles/devops.toml")),
    ("qa", include_str!("../../assets/roles/qa.toml")),
];

/// The template a new role starts from, so an empty file is never saved.
pub const TEMPLATE: &str = "# channels = [\"frontend\"]  # joined automatically at launch\n\
     # tool = \"claude\"           # Claude Code or Codex\n\
     # model = \"\"                # model override\n\
     # driver = true             # stay interactive instead of parking on wait\n\
     # tools = [\"Read\", \"Edit\", \"Bash(git:*)\"]  # pre-granted tool rules\n\
     description = \"\"\"\n\
     Describe what this agent owns and how it should coordinate with the rest of\n\
     the mesh.\n\
     \"\"\"\n";

/// The file schema. `name` is taken from the file name, so it is ignored here.
#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    channels: Vec<String>,
    /// Which connected tool to launch: `claude` or `codex`.
    tool: Option<String>,
    model: Option<String>,
    #[serde(default)]
    description: String,
    /// A human-driven lead. It launches interactively rather than parking on the
    /// wait loop, so the person in the terminal can steer it.
    #[serde(default)]
    driver: bool,
    /// Tool rules pre-granted to the agent, passed through to its own CLI.
    #[serde(default)]
    tools: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Role {
    pub channels: Vec<String>,
    pub tool: Option<String>,
    pub model: Option<String>,
    pub description: String,
    pub driver: bool,
    pub tools: Vec<String>,
}

/// Resolve a role by name against the project layer rooted at `root`.
pub fn resolve(root: Option<&Path>, name: &str) -> Result<Option<Role>> {
    let Some((text, source)) = layer::read(KIND, BUILTINS, root, name) else {
        return Ok(None);
    };
    parse(name, &text, source).map(Some)
}

/// Every role name with the layer it resolves from.
pub fn roles(root: Option<&Path>) -> Vec<(String, Source)> {
    layer::scan(KIND, BUILTINS, root).into_iter().collect()
}

/// Read a role's file text and the layer it came from, so it can be shown or
/// used as the seed for a copy in a layer the user owns.
pub fn text(root: Option<&Path>, name: &str) -> Result<(String, Source)> {
    layer::read(KIND, BUILTINS, root, name).with_context(|| format!("no role named `{name}`"))
}

pub fn save(name: &str, user: bool, root: &Path, text: &str) -> Result<std::path::PathBuf> {
    parse(name, text, Source::Project)?;
    layer::save(KIND, name, user, root, text)
}

pub fn delete(name: &str, user: bool, root: &Path) -> Result<std::path::PathBuf> {
    layer::remove(KIND, name, user, root)
}

fn parse(name: &str, text: &str, _source: Source) -> Result<Role> {
    let file: File =
        toml::from_str(text).with_context(|| format!("could not read the role `{name}`"))?;
    Ok(Role {
        channels: file.channels,
        tool: file.tool,
        model: file.model,
        description: file.description.trim().to_owned(),
        driver: file.driver,
        tools: file.tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_role_parses_and_carries_a_brief() {
        for (name, text) in BUILTINS {
            let role = parse(name, text, Source::Builtin).unwrap();
            assert!(
                !role.description.is_empty(),
                "the built-in role `{name}` has no brief"
            );
        }
    }

    #[test]
    fn the_supervisor_is_the_only_builtin_that_drives() {
        for (name, text) in BUILTINS {
            let role = parse(name, text, Source::Builtin).unwrap();
            assert_eq!(
                role.driver,
                *name == "supervisor",
                "`{name}` has the wrong driver setting"
            );
        }
    }

    #[test]
    fn a_project_role_overrides_the_builtin_of_the_same_name() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        crate::files::write(
            &layer::file(&layer::projectdirectory(root, KIND), "worker"),
            "channels = [\"mine\"]\ndescription = \"local\"\n",
        )
        .unwrap();

        let role = resolve(Some(root), "worker").unwrap().unwrap();
        let (_, source) = text(Some(root), "worker").unwrap();

        assert_eq!(source, Source::Project);
        assert_eq!(role.channels, ["mine"]);
        assert_eq!(role.description, "local");
    }

    #[test]
    fn an_unknown_role_resolves_to_nothing_rather_than_failing() {
        let directory = tempfile::tempdir().unwrap();
        assert!(resolve(Some(directory.path()), "nobody").unwrap().is_none());
    }

    #[test]
    fn a_malformed_role_file_is_refused_before_it_is_saved() {
        let directory = tempfile::tempdir().unwrap();
        assert!(save("broken", false, directory.path(), "channels = ").is_err());
        assert!(
            !layer::file(&layer::projectdirectory(directory.path(), KIND), "broken").exists(),
            "a draft that does not parse must not land on disk"
        );
    }

    #[test]
    fn the_new_role_template_parses() {
        assert!(parse("draft", TEMPLATE, Source::Project).is_ok());
    }
}
