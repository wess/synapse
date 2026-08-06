//! Managing the layered TOML files a person edits — roles, teams, and tool
//! descriptors — the way `git config` manages settings: list what resolves, show
//! one, and round-trip a file through `$EDITOR`. Editing a built-in copies it
//! down into a layer you own first, so the shipped templates stay intact.
//!
//! All three differ only in which functions read and write them, so they share
//! one implementation: a draft that does not parse never replaces a working
//! file, whichever kind it is.

use crate::cli::Outcome;
use crate::cli::editor::editor;
use crate::cli::relay::{directory, text};
use crate::relay::{self, Source};
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const ROLEUSAGE: &str = "usage: synapse relay role <list|show|create|edit|delete> [name] [--user]";
const TEAMUSAGE: &str =
    "usage: synapse relay team <list|show|create|edit|delete|open> [name] [--user]";
const TOOLUSAGE: &str = "usage: synapse tool <list|show|create|edit|delete> [name] [--user]";

pub fn role(arguments: &[OsString]) -> Result<Outcome> {
    let kind = Kind {
        label: "role",
        usage: ROLEUSAGE,
        template: relay::role::TEMPLATE,
        names: |root| relay::role::roles(Some(root)),
        text: |root, name| relay::role::text(Some(root), name),
        save: relay::role::save,
        delete: relay::role::delete,
    };
    run(&kind, arguments)
}

pub fn team(arguments: &[OsString]) -> Result<Outcome> {
    let kind = Kind {
        label: "team",
        usage: TEAMUSAGE,
        template: relay::team::TEMPLATE,
        names: |root| relay::team::teams(Some(root)),
        text: |root, name| relay::team::text(Some(root), name),
        save: relay::team::save,
        delete: relay::team::delete,
    };
    run(&kind, arguments)
}

/// Describing a tool Synapse does not ship: what it is called, where it keeps
/// its files, and what to run to connect it.
pub fn tool(arguments: &[OsString]) -> Result<Outcome> {
    run(&toolkind(), arguments)
}

/// Round-trip one tool descriptor through `$EDITOR`, seeded from the existing
/// one or from the template when there is none yet.
///
/// The dashboards call this rather than growing a form of their own: a
/// descriptor has four sections and a person editing one wants their own editor,
/// not a field at a time. It saves into the user layer, which is the one that
/// travels with the person rather than the checkout.
pub(crate) fn describetool(slug: &str) -> Result<PathBuf> {
    let kind = toolkind();
    let root = std::env::current_dir()?;
    let seed = (kind.text)(&root, slug)
        .map(|(body, _)| body)
        .unwrap_or_else(|_| kind.template.to_owned());
    edit(&kind, slug, true, &root, seed)
}

fn toolkind() -> Kind {
    Kind {
        label: "tool",
        usage: TOOLUSAGE,
        template: crate::agent::tool::TEMPLATE,
        names: |root| crate::agent::tool::names(Some(root)),
        text: |root, name| crate::agent::tool::text(Some(root), name),
        save: crate::agent::tool::save,
        delete: crate::agent::tool::delete,
    }
}

/// The three file kinds differ only in which functions read and write them.
struct Kind {
    label: &'static str,
    usage: &'static str,
    template: &'static str,
    names: fn(&Path) -> Vec<(String, Source)>,
    text: fn(&Path, &str) -> Result<(String, Source)>,
    save: fn(&str, bool, &Path, &str) -> Result<PathBuf>,
    delete: fn(&str, bool, &Path) -> Result<PathBuf>,
}

fn run(kind: &Kind, arguments: &[OsString]) -> Result<Outcome> {
    let action = text(arguments, 0, kind.usage)?;
    let root = directory(arguments)?;
    let user = arguments.iter().any(|value| value == "--user");
    match action.as_str() {
        "list" => list(kind, &root, arguments)?,
        "show" => {
            let name = text(arguments, 1, kind.usage)?;
            let (body, source) = (kind.text)(&root, &name)?;
            println!("# {name} · {}", source.label());
            print!("{body}");
        }
        "create" => {
            let name = text(arguments, 1, kind.usage)?;
            let path = edit(kind, &name, user, &root, kind.template.to_owned())?;
            println!("Saved {}", path.display());
        }
        "edit" => {
            let name = text(arguments, 1, kind.usage)?;
            // A built-in has no file to open, so its text is the seed for a copy
            // in a layer the user owns.
            let seed = (kind.text)(&root, &name)
                .map(|(body, _)| body)
                .unwrap_or_else(|_| kind.template.to_owned());
            let path = edit(kind, &name, user, &root, seed)?;
            println!("Saved {}", path.display());
        }
        "delete" => {
            let name = text(arguments, 1, kind.usage)?;
            let path = (kind.delete)(&name, user, &root)?;
            println!("Deleted {}", path.display());
        }
        unknown => anyhow::bail!(
            "unknown {} command `{unknown}`\n\n{}",
            kind.label,
            kind.usage
        ),
    }
    Ok(Outcome::Exit(0))
}

fn list(kind: &Kind, root: &Path, arguments: &[OsString]) -> Result<()> {
    let names = (kind.names)(root);
    if arguments.iter().any(|value| value == "--json") {
        let rows: Vec<_> = names
            .iter()
            .map(|(name, source)| serde_json::json!({"name": name, "source": source.label()}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    for (name, source) in names {
        println!("{name}\t{}", source.label());
    }
    Ok(())
}

/// Open `$EDITOR` on a draft and only move it into place once it parses, so a
/// file that would not load never replaces a working one.
fn edit(kind: &Kind, name: &str, user: bool, root: &Path, seed: String) -> Result<PathBuf> {
    let draft = std::env::temp_dir().join(format!("synapse-{}-{name}.toml", kind.label));
    std::fs::write(&draft, seed)
        .with_context(|| format!("could not create {}", draft.display()))?;

    let status = std::process::Command::new(editor())
        .arg(&draft)
        .status()
        .context("could not open your editor")?;
    if !status.success() {
        let _ = std::fs::remove_file(&draft);
        anyhow::bail!("the editor exited without saving");
    }

    let edited = std::fs::read_to_string(&draft)?;
    match (kind.save)(name, user, root, &edited) {
        Ok(path) => {
            let _ = std::fs::remove_file(&draft);
            Ok(path)
        }
        Err(error) => {
            Err(error).with_context(|| format!("your draft is still at {}", draft.display()))
        }
    }
}
