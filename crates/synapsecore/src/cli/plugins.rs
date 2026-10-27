//! The local development loop for declarative plugins.
//!
//! A plugin is checked before anything reaches the installed folder. Installs
//! copy only the manifest and assets it names, so an unrelated file in a
//! development checkout cannot hitch a ride into Synapse. Replacing keeps one
//! bounded rollback directory beside the installed copy.

use crate::cli::Outcome;
use crate::plugins::{self, Plugin};
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: synapse plugin <list|path|init|create|check|install>";

pub fn run(arguments: &[OsString]) -> Result<Outcome> {
    let action = super::relay::text(arguments, 0, USAGE)?;
    match action.as_str() {
        "list" => list(arguments.iter().any(|value| value == "--json"))?,
        "path" => path()?,
        "init" => init(arguments)?,
        "create" => create(arguments)?,
        "check" => check(arguments)?,
        "install" => install(arguments)?,
        unknown => anyhow::bail!("unknown plugin command `{unknown}`\n\n{USAGE}"),
    }
    Ok(Outcome::Exit(0))
}

fn list(json: bool) -> Result<()> {
    let root = plugins::folder()?;
    let catalog = plugins::catalog(&root);
    if json {
        let rows = catalog
            .plugins
            .iter()
            .map(|plugin| {
                serde_json::json!({
                    "id": plugin.manifest.id,
                    "name": plugin.manifest.name,
                    "version": plugin.manifest.version,
                    "kind": plugin.manifest.kind,
                    "built_in": plugin.root.is_none(),
                    "path": plugin.root,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for plugin in &catalog.plugins {
            println!(
                "{}\t{}\t{}\t{}",
                plugin.manifest.id,
                plugin.manifest.version,
                if plugin.root.is_none() {
                    "built in"
                } else {
                    "installed"
                },
                plugin.manifest.name
            );
        }
    }
    for problem in catalog.problems {
        eprintln!("warning: {problem}");
    }
    Ok(())
}

fn path() -> Result<()> {
    println!("{}", plugins::folder()?.display());
    Ok(())
}

fn create(arguments: &[OsString]) -> Result<()> {
    let id = super::relay::text(arguments, 1, "usage: synapse plugin create <id> [folder]")?;
    plugins::validateid(&id)?;
    ensure!(id != "workshop", "`workshop` is the built-in plugin id");
    let target = arguments
        .get(2)
        .filter(|value| !value.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&id));
    ensure!(!target.exists(), "{} already exists", target.display());
    if let Some(parent) = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::create_dir(&target).with_context(|| format!("could not create {}", target.display()))?;
    if let Err(error) = writemanifest(&target, &id) {
        let _ = fs::remove_dir(&target);
        return Err(error);
    }
    println!("Created {}", target.display());
    println!("Check it with `synapse plugin check {}`.", target.display());
    Ok(())
}

fn init(arguments: &[OsString]) -> Result<()> {
    let target = folderargument(arguments, 1).unwrap_or_else(|| PathBuf::from("."));
    ensure!(target.is_dir(), "{} is not a folder", target.display());
    let requested = super::relay::value(arguments, "--id");
    ensure!(
        !arguments.iter().any(|value| value == "--id") || requested.is_some(),
        "--id needs a value"
    );
    let id = requested
        .or_else(|| {
            target
                .canonicalize()
                .ok()?
                .file_name()?
                .to_str()
                .map(ToOwned::to_owned)
        })
        .context("the folder name is not a plugin id; provide --id <id>")?;
    plugins::validateid(&id)
        .with_context(|| "the folder name is not a plugin id; provide --id <id>")?;
    ensure!(id != "workshop", "`workshop` is the built-in plugin id");
    ensure!(
        !target.join("plugin.toml").exists(),
        "{} already has a plugin.toml",
        target.display()
    );
    writemanifest(&target, &id)?;
    println!("Initialized {} as plugin `{id}`.", target.display());
    println!("Check it with `synapse plugin check {}`.", target.display());
    Ok(())
}

fn writemanifest(target: &Path, id: &str) -> Result<()> {
    let manifest = format!(
        "api = 1\nid = \"{id}\"\nname = \"{id}\"\nversion = \"0.1.0\"\nkind = \"scene\"\n\n[scene]\nfloor = \"#87959c\"\nwall = \"#464d68\"\nwood = \"#61576c\"\naccent = \"#b5d8df\"\n# background = \"room.png\"\n# characters = \"characters.png\"\nframewidth = 26\nframeheight = 36\n"
    );
    crate::files::write(&target.join("plugin.toml"), &manifest)
}

fn check(arguments: &[OsString]) -> Result<()> {
    let source = folderargument(arguments, 1).unwrap_or_else(|| PathBuf::from("."));
    let plugin = plugins::load(&source)
        .with_context(|| format!("{} is not a valid plugin", source.display()))?;
    if arguments.iter().any(|value| value == "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": plugin.manifest.id,
                "name": plugin.manifest.name,
                "version": plugin.manifest.version,
                "kind": plugin.manifest.kind,
                "path": plugin.root,
                "valid": true,
            }))?
        );
    } else {
        println!(
            "{} {} is a valid {} plugin.",
            plugin.manifest.name, plugin.manifest.version, plugin.manifest.kind
        );
    }
    Ok(())
}

fn install(arguments: &[OsString]) -> Result<()> {
    let source = folderargument(arguments, 1).unwrap_or_else(|| PathBuf::from("."));
    let plugin = plugins::load(&source)
        .with_context(|| format!("{} is not a valid plugin", source.display()))?;
    ensure!(
        plugin.manifest.id != "workshop",
        "the built-in `workshop` plugin cannot be replaced"
    );
    let root = plugins::folder()?;
    fs::create_dir_all(&root).with_context(|| format!("could not create {}", root.display()))?;
    let target = root.join(&plugin.manifest.id);
    let source = plugin
        .root
        .as_deref()
        .context("plugin has no source folder")?;
    if target
        .canonicalize()
        .ok()
        .is_some_and(|installed| installed == source)
    {
        println!("{} is already installed and valid.", plugin.manifest.name);
        return Ok(());
    }
    let replace = arguments.iter().any(|value| value == "--replace");
    ensure!(
        !target.exists() || replace,
        "{} is already installed; add --replace after reviewing the existing copy",
        plugin.manifest.id
    );

    let staging = staging(&root, &plugin.manifest.id)?;
    if let Err(error) =
        copydeclared(&plugin, &staging).and_then(|_| plugins::load(&staging).map(|_| ()))
    {
        let _ = fs::remove_dir_all(&staging);
        return Err(error).context("could not stage plugin");
    }

    let backup = root.join(format!(".{}.synapsebackup", plugin.manifest.id));
    let hadtarget = target.exists();
    if hadtarget {
        removemanaged(&backup)?;
        fs::rename(&target, &backup)
            .with_context(|| format!("could not back up {}", target.display()))?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if hadtarget {
            let _ = fs::rename(&backup, &target);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(error).with_context(|| format!("could not install {}", target.display()));
    }
    crate::files::syncdirectory(&root)?;
    println!("Installed {} at {}", plugin.manifest.name, target.display());
    if hadtarget {
        println!("Previous copy: {}", backup.display());
    }
    Ok(())
}

fn folderargument(arguments: &[OsString], index: usize) -> Option<PathBuf> {
    arguments
        .get(index)
        .filter(|value| !value.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
}

fn staging(root: &Path, id: &str) -> Result<PathBuf> {
    for number in 0..100 {
        let path = root.join(format!(
            ".synapseinstall-{id}-{}-{number}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not create staging folder in {}", root.display())
                });
            }
        }
    }
    anyhow::bail!(
        "could not create a unique staging folder in {}",
        root.display()
    )
}

fn copydeclared(plugin: &Plugin, target: &Path) -> Result<()> {
    let source = plugin
        .root
        .as_deref()
        .context("plugin has no source folder")?;
    crate::files::atomiccopy(&source.join("plugin.toml"), &target.join("plugin.toml"))?;
    for name in [
        plugin.manifest.scene.background.as_deref(),
        plugin.manifest.scene.characters.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let sourceasset = plugins::asset(source, name)?;
        let targetasset = target.join(name);
        crate::files::atomiccopy(&sourceasset, &targetasset)?;
    }
    Ok(())
}

fn removemanaged(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("could not inspect {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    }
    .with_context(|| format!("could not replace managed backup {}", path.display()))
}
