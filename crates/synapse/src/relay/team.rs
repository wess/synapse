//! A team is a named roster: open it and every member launches at once, the
//! first as the human-driven lead and the rest as workers. Sinclair paired this
//! with a pane layout; Synapse has no panes, so a team is the roster alone.
//! Resolved project → user → built-in, exactly like a role.

use crate::relay::layer::{self, Source};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

const KIND: &str = "teams";

const BUILTINS: &[(&str, &str)] = &[
    ("web", include_str!("../../assets/teams/web.toml")),
    ("pair", include_str!("../../assets/teams/pair.toml")),
];

pub const TEMPLATE: &str = "# The first member is the lead: it stays interactive so you can steer it.\n\
     [[member]]\n\
     name = \"lead\"\n\
     role = \"supervisor\"\n\
     \n\
     [[member]]\n\
     name = \"worker\"\n\
     role = \"worker\"\n\
     # tool = \"claude\"\n";

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    member: Vec<Member>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Member {
    pub name: String,
    pub role: Option<String>,
    /// Which connected tool this member launches with, overriding the role's.
    pub tool: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Team {
    pub name: String,
    pub members: Vec<Member>,
}

pub fn resolve(root: Option<&Path>, name: &str) -> Result<Option<Team>> {
    let Some((text, source)) = layer::read(KIND, BUILTINS, root, name) else {
        return Ok(None);
    };
    parse(name, &text, source).map(Some)
}

pub fn teams(root: Option<&Path>) -> Vec<(String, Source)> {
    layer::scan(KIND, BUILTINS, root).into_iter().collect()
}

pub fn text(root: Option<&Path>, name: &str) -> Result<(String, Source)> {
    layer::read(KIND, BUILTINS, root, name).with_context(|| format!("no team named `{name}`"))
}

pub fn save(name: &str, user: bool, root: &Path, text: &str) -> Result<std::path::PathBuf> {
    parse(name, text, Source::Project)?;
    layer::save(KIND, name, user, root, text)
}

pub fn delete(name: &str, user: bool, root: &Path) -> Result<std::path::PathBuf> {
    layer::remove(KIND, name, user, root)
}

fn parse(name: &str, text: &str, _source: Source) -> Result<Team> {
    let file: File =
        toml::from_str(text).with_context(|| format!("could not read the team `{name}`"))?;
    anyhow::ensure!(!file.member.is_empty(), "the team `{name}` has no members");
    let mut seen = std::collections::HashSet::new();
    for member in &file.member {
        anyhow::ensure!(
            seen.insert(member.name.as_str()),
            "the team `{name}` uses the member name `{}` twice; every agent on the mesh needs its own name",
            member.name
        );
    }
    Ok(Team {
        name: name.to_owned(),
        members: file.member,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_team_parses_with_a_lead_first() {
        for (name, text) in BUILTINS {
            let team = parse(name, text, Source::Builtin).unwrap();
            assert!(!team.members.is_empty(), "the team `{name}` is empty");
        }
        let web = parse("web", BUILTINS[0].1, Source::Builtin).unwrap();
        assert_eq!(web.members[0].role.as_deref(), Some("supervisor"));
    }

    #[test]
    fn a_team_with_no_members_is_refused() {
        assert!(parse("empty", "", Source::Builtin).is_err());
    }

    #[test]
    fn duplicate_member_names_are_refused_before_two_agents_collide() {
        let text = "[[member]]\nname = \"a\"\n\n[[member]]\nname = \"a\"\n";
        let error = parse("clash", text, Source::Project)
            .unwrap_err()
            .to_string();
        assert!(error.contains("twice"), "got {error}");
    }

    #[test]
    fn the_new_team_template_parses() {
        assert!(parse("draft", TEMPLATE, Source::Project).is_ok());
    }
}
