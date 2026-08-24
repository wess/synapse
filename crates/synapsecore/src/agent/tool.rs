//! What a connectable tool is, as data.
//!
//! Synapse connects to coding tools, and there will be more of them than any
//! release can name. A tool that only exists as a match arm is a tool that
//! cannot be added without editing five files and shipping a build, so what a
//! connection needs is written down instead: where the tool keeps its files,
//! what to run to register the server, how to read its config back to tell
//! whether that worked, and how to start it.
//!
//! Descriptors resolve project → user → built-in through the same layered store
//! roles and teams use, so a repository can carry the tool its team works in and
//! a person can override a built-in without waiting for a release. The three
//! Synapse ships are ordinary descriptors in `assets/tools/`, which is what
//! stops a custom tool from being a second-class path through the same code.
//!
//! Two things are deliberately *not* data, because they are behaviour rather
//! than configuration: Claude Code's session hook and status line, and pi's
//! extension write-out. Those stay keyed on [`Kind`].

use crate::agent::{Agent, Kind};
use crate::relay::layer::{self, Source};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const KIND: &str = "tools";

const BUILTINS: &[(&str, &str)] = &[
    ("claude", include_str!("../../assets/tools/claude.toml")),
    ("codex", include_str!("../../assets/tools/codex.toml")),
    ("pi", include_str!("../../assets/tools/pi.toml")),
];

/// The template a new descriptor starts from. Everything a tool that follows the
/// `mcp add` convention needs is uncommented; the rest documents itself.
pub const TEMPLATE: &str = r#"name = "My Tool"          # what it is called in the dashboard
command = "mytool"        # the binary, found on PATH

[home]
# env = "MYTOOL_HOME"     # the tool's own override, if it has one
default = ".mytool"       # under the user's home directory

# `{home}` is the directory above. A path without it is relative to the user's
# home, and an absolute path is used as written.
[paths]
instructions = "{home}/AGENTS.md"   # the file a managed guidance block goes in
settings = "{home}/settings.json"
integration = "{home}/settings.json" # where a registered MCP server is recorded
skills = "{home}/skills"
# projectskills = ".mytool/skills"  # relative to a project root, for skills
                                    # that belong to one repository

# Run against the tool's own CLI. `{server}` is the Synapse binary.
[connect]
add = ["mcp", "add", "synapse", "--", "{server}", "mcp"]
remove = ["mcp", "remove", "synapse"]
# replace = true          # remove a stale registration before adding

# How to read `integration` back and tell a connection from a stale entry.
[detect]
format = "json"           # json or toml
at = ["mcpServers", "synapse"]
args = ["mcp"]

# Optional. Each key is a slot Synapse fills when it starts the tool; leave one
# out and that flag is simply never passed.
[launch]
prompt = ["{prompt}"]
# headless = ["-p", "{prompt}"]
# config = ["--mcp-config", "{config}"]
# model = ["--model", "{model}"]
# skippermissions = ["--yes"]
"#;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connect {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
    #[serde(default)]
    pub replace: bool,
}

/// How a tool's config file answers "is Synapse connected?".
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detect {
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub format: Format,
    /// The key path to the registration, or to the package list.
    #[serde(default)]
    pub at: Vec<String>,
    /// The arguments a registration Synapse wrote would carry.
    #[serde(default)]
    pub args: Vec<String>,
    /// Package style only: the package name, however it was installed.
    #[serde(default)]
    pub package: String,
    /// Package style only: the source a connection installs now.
    #[serde(default)]
    pub source: String,
    /// Package style only: what overrides that source, for tests and checkouts.
    #[serde(default)]
    pub env: String,
}

/// Whether a connection is an MCP server entry or an installed package.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    #[default]
    Server,
    Package,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Json,
    Toml,
}

/// The argv slots Synapse fills when it starts the tool. Every one is optional:
/// an absent slot is a flag this tool does not have, not an error.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    #[serde(default)]
    pub prompt: Vec<String>,
    #[serde(default)]
    pub headless: Vec<String>,
    #[serde(default)]
    pub config: Vec<String>,
    #[serde(default)]
    pub model: Vec<String>,
    #[serde(default)]
    pub skippermissions: Vec<String>,
    #[serde(default)]
    pub strict: Vec<String>,
    #[serde(default)]
    pub allowedtools: Vec<String>,
}

/// The file schema. `slug` comes from the file name, so it is not read here.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: String,
    command: String,
    #[serde(default)]
    home: Home,
    paths: Paths,
    #[serde(default)]
    connect: Connect,
    #[serde(default)]
    detect: Detect,
    #[serde(default)]
    launch: Launch,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Home {
    /// The tool's own override, honoured before the default.
    #[serde(default)]
    env: String,
    #[serde(default)]
    default: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Paths {
    instructions: String,
    settings: String,
    integration: String,
    skills: String,
    /// Relative to a project root rather than to the user's home, because that
    /// is the only thing it can be relative to.
    #[serde(default)]
    projectskills: String,
}

/// Every descriptor across the layers, built-ins first and in the order they are
/// listed so the dashboard's rows stay put between runs.
pub fn tools(home: &Path, root: Option<&Path>) -> Vec<Agent> {
    let mut resolved = Vec::new();
    let scanned = layer::scan(KIND, BUILTINS, root);
    for (slug, _) in BUILTINS {
        if scanned.contains_key(*slug)
            && let Some(tool) = resolve(home, root, slug).ok().flatten()
        {
            resolved.push(tool);
        }
    }
    for slug in scanned.keys() {
        if BUILTINS.iter().any(|(item, _)| item == slug) {
            continue;
        }
        if let Some(tool) = resolve(home, root, slug).ok().flatten() {
            resolved.push(tool);
        }
    }
    resolved
}

/// Every descriptor name with the layer it resolves from, including ones that do
/// not parse — a broken file is listed so it can be found and fixed, rather than
/// silently vanishing from the dashboard.
pub fn names(root: Option<&Path>) -> Vec<(String, Source)> {
    layer::scan(KIND, BUILTINS, root).into_iter().collect()
}

/// Resolve one descriptor by name. `Ok(None)` is "no such tool"; `Err` is a file
/// that exists and does not parse, which the user needs told about.
pub fn resolve(home: &Path, root: Option<&Path>, slug: &str) -> Result<Option<Agent>> {
    let Some((text, source)) = layer::read(KIND, BUILTINS, root, slug) else {
        return Ok(None);
    };
    parse(home, slug, &text, source).map(Some)
}

/// A descriptor's file text and the layer it came from, so it can be shown or
/// used as the seed for a copy in a layer the user owns.
pub fn text(root: Option<&Path>, slug: &str) -> Result<(String, Source)> {
    layer::read(KIND, BUILTINS, root, slug).with_context(|| format!("no tool named `{slug}`"))
}

pub fn save(slug: &str, user: bool, root: &Path, text: &str) -> Result<PathBuf> {
    let home = crate::files::home()?;
    parse(&home, slug, text, Source::User).context("that descriptor is not usable")?;
    layer::save(KIND, slug, user, root, text)
}

/// Remove a descriptor from a layer the user owns. A built-in has no file, so
/// deleting one is reported rather than silently accepted — the way back to the
/// shipped version is deleting the copy that overrides it.
pub fn delete(slug: &str, user: bool, root: &Path) -> Result<PathBuf> {
    layer::remove(KIND, slug, user, root)
}

/// The descriptor file a person owns for `slug`, created from the layer it
/// currently resolves from — or from the template when there is none yet.
///
/// This is what a dashboard opens when somebody wants to describe a tool: it
/// returns a path that exists and parses, so the editor on the other side has a
/// real file rather than a promise of one. Editing a built-in copies it down
/// first, which is the same rule the CLI follows.
pub fn draft(slug: &str) -> Result<PathBuf> {
    layer::valid(slug)?;
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let path = layer::file(&layer::userdirectory(KIND)?, slug);
    if path.is_file() {
        return Ok(path);
    }
    let seed = layer::read(KIND, BUILTINS, Some(&root), slug)
        .map(|(text, _)| text)
        .unwrap_or_else(|| TEMPLATE.to_owned());
    save(slug, true, &root, &seed)
}

/// Whether a slug names one of the three Synapse ships, which have behaviour
/// beyond their descriptor and so cannot be reduced to one.
pub fn kind(slug: &str) -> Kind {
    match slug {
        "codex" => Kind::Codex,
        "claude" => Kind::Claude,
        "pi" => Kind::Pi,
        _ => Kind::Custom,
    }
}

fn parse(home: &Path, slug: &str, text: &str, source: Source) -> Result<Agent> {
    layer::valid(slug)?;
    let file: File =
        toml::from_str(text).with_context(|| format!("could not read the `{slug}` tool"))?;
    anyhow::ensure!(!file.name.trim().is_empty(), "`{slug}` needs a name");
    anyhow::ensure!(!file.command.trim().is_empty(), "`{slug}` needs a command");
    let toolhome = toolhome(home, &file.home);
    Ok(Agent {
        kind: kind(slug),
        slug: slug.to_owned(),
        name: file.name,
        command: file.command,
        instructions: resolvepath(home, &toolhome, &file.paths.instructions),
        settings: resolvepath(home, &toolhome, &file.paths.settings),
        integration: resolvepath(home, &toolhome, &file.paths.integration),
        skills: resolvepath(home, &toolhome, &file.paths.skills),
        projectskills: file
            .paths
            .projectskills
            .trim()
            .trim_start_matches('/')
            .to_owned(),
        connect: file.connect,
        detect: file.detect,
        launch: file.launch,
        source,
    })
}

/// The tool's own directory: its environment override when it has one and that
/// variable is set, and the descriptor's default under the user's home otherwise.
fn toolhome(home: &Path, declared: &Home) -> PathBuf {
    if !declared.env.is_empty()
        && let Some(value) = std::env::var_os(&declared.env).filter(|value| !value.is_empty())
    {
        return PathBuf::from(value);
    }
    home.join(&declared.default)
}

/// `{home}` is the tool's directory, a bare relative path is under the user's
/// home, and an absolute path is used as written.
fn resolvepath(home: &Path, toolhome: &Path, declared: &str) -> PathBuf {
    if let Some(rest) = declared.strip_prefix("{home}") {
        return toolhome.join(rest.trim_start_matches('/'));
    }
    let path = Path::new(declared);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    home.join(path)
}

/// Build a descriptor straight from text, for tests elsewhere in the crate that
/// need a tool nobody ships.
#[cfg(test)]
pub(crate) fn parsefortest(slug: &str, text: &str) -> Agent {
    parse(Path::new("/users/test"), slug, text, Source::User).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin(slug: &str, home: &Path) -> Agent {
        resolve(home, None, slug).unwrap().unwrap()
    }

    /// The three Synapse ships have to survive the move into data unchanged, or
    /// every existing connection on every machine stops being recognised.
    #[test]
    fn the_built_ins_resolve_to_the_paths_they_have_always_used() {
        let home = Path::new("/users/test");

        let codex = builtin("codex", home);
        assert_eq!(codex.name, "Codex");
        assert_eq!(codex.instructions, home.join(".codex/AGENTS.md"));
        assert_eq!(codex.integration, home.join(".codex/config.toml"));
        assert_eq!(codex.settings, home.join(".codex/config.toml"));
        // The shared Agent Skills folder, not Codex's own home.
        assert_eq!(codex.skills, home.join(".agents/skills"));

        let claude = builtin("claude", home);
        assert_eq!(claude.name, "Claude Code");
        assert_eq!(claude.instructions, home.join(".claude/CLAUDE.md"));
        assert_eq!(claude.settings, home.join(".claude/settings.json"));
        // Beside the home directory rather than inside it.
        assert_eq!(claude.integration, home.join(".claude.json"));
        assert_eq!(claude.skills, home.join(".claude/skills"));

        let pi = builtin("pi", home);
        assert_eq!(pi.instructions, home.join(".pi/agent/APPEND_SYSTEM.md"));
        assert_eq!(pi.settings, home.join(".pi/agent/settings.json"));
        // pi records an installed package in the settings it reads everything
        // else from, so the two are one file.
        assert_eq!(pi.integration, pi.settings);
        assert_eq!(pi.skills, home.join(".pi/agent/skills"));
    }

    #[test]
    fn a_tool_that_declares_an_environment_override_honours_it() {
        let home = Path::new("/users/test");
        // Unset: the descriptor's default applies.
        assert_eq!(
            builtin("codex", home).integration,
            home.join(".codex/config.toml")
        );

        let text = "name = \"T\"\ncommand = \"t\"\n\
             [home]\nenv = \"SYNAPSE_TOOLHOME_TEST\"\ndefault = \".t\"\n\
             [paths]\ninstructions = \"{home}/A.md\"\nsettings = \"{home}/s\"\n\
             integration = \"{home}/s\"\nskills = \"{home}/skills\"\n";
        // SAFETY: a variable this test owns, read only by the parse below.
        unsafe { std::env::set_var("SYNAPSE_TOOLHOME_TEST", "/elsewhere") };
        let tool = parse(home, "t", text, Source::User).unwrap();
        unsafe { std::env::remove_var("SYNAPSE_TOOLHOME_TEST") };

        assert_eq!(tool.instructions, Path::new("/elsewhere/A.md"));
    }

    #[test]
    fn detection_styles_come_through_as_written() {
        let home = Path::new("/users/test");

        let codex = builtin("codex", home);
        assert_eq!(codex.detect.style, Style::Server);
        assert_eq!(codex.detect.format, Format::Toml);
        assert_eq!(codex.detect.at, ["mcp_servers", "synapse"]);

        let claude = builtin("claude", home);
        assert_eq!(claude.detect.format, Format::Json);
        assert!(claude.connect.replace, "a stale entry is replaced");

        let pi = builtin("pi", home);
        assert_eq!(pi.detect.style, Style::Package);
        assert_eq!(pi.detect.package, "synapse-pi");
        assert_eq!(pi.detect.env, "SYNAPSE_PI_PACKAGE");
    }

    #[test]
    fn a_descriptor_that_does_not_parse_is_reported_rather_than_dropped() {
        let home = Path::new("/users/test");
        assert!(parse(home, "t", "name = \"T\"\n", Source::User).is_err());
        assert!(parse(home, "t", "not toml at all {", Source::User).is_err());
        // A name a file cannot be called is refused before anything is written.
        let usable = "name = \"T\"\ncommand = \"t\"\n[paths]\ninstructions = \"a\"\n\
             settings = \"b\"\nintegration = \"c\"\nskills = \"d\"\n";
        assert!(parse(home, "Not Valid", usable, Source::User).is_err());
        assert!(parse(home, "hermes", usable, Source::User).is_ok());
    }

    #[test]
    fn the_template_is_a_usable_descriptor() {
        let home = Path::new("/users/test");
        let tool = parse(home, "mytool", TEMPLATE, Source::User).unwrap();

        assert_eq!(tool.command, "mytool");
        assert_eq!(tool.integration, home.join(".mytool/settings.json"));
        assert_eq!(tool.connect.add[0], "mcp");
    }

    #[test]
    fn only_the_three_that_carry_extra_behaviour_are_built_in_kinds() {
        assert_eq!(kind("codex"), Kind::Codex);
        assert_eq!(kind("claude"), Kind::Claude);
        assert_eq!(kind("pi"), Kind::Pi);
        assert_eq!(kind("hermes"), Kind::Custom);
    }
}
