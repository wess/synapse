//! Where the library keeps its servers: one TOML file each, in Synapse's own
//! folder, so a person can read and edit one like any other config.

use crate::servers::model::{Server, validname};
use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn directory() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("mcp"))
}

fn path(name: &str) -> Result<PathBuf> {
    validname(name)?;
    Ok(directory()?.join(format!("{name}.toml")))
}

/// Every server, by name, and a line for each file that does not read — a
/// broken file is reported so it can be fixed rather than silently vanishing.
pub fn all() -> Result<(Vec<Server>, Vec<String>)> {
    let directory = directory()?;
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut servers = Vec::new();
    let mut problems = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        match std::fs::read_to_string(&path)
            .context("could not read it")
            .and_then(|text| Server::parse(name, &text))
        {
            Ok(server) => servers.push(server),
            Err(error) => problems.push(format!("{}: {error:#}", path.display())),
        }
    }
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((servers, problems))
}

pub fn read(name: &str) -> Result<Server> {
    let path = path(name)?;
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("the library has no MCP server named `{name}`"))?;
    Server::parse(name, &text)
}

pub fn exists(name: &str) -> bool {
    path(name).is_ok_and(|path| path.is_file())
}

/// Write a server, replacing the file if there is one. Callers that must not
/// replace check `exists` first and say so in their own words.
pub fn save(server: &Server) -> Result<PathBuf> {
    let path = path(&server.name)?;
    // Parsed back before it is written, so a file this function produced is
    // always one `read` accepts.
    let text = server.text()?;
    Server::parse(&server.name, &text)?;
    std::fs::create_dir_all(directory()?).with_context(|| {
        format!(
            "could not create {}",
            directory().unwrap_or_default().display()
        )
    })?;
    crate::files::write(&path, &text)?;
    Ok(path)
}

pub fn delete(name: &str) -> Result<PathBuf> {
    let path = path(name)?;
    anyhow::ensure!(
        path.is_file(),
        "the library has no MCP server named `{name}`"
    );
    std::fs::remove_file(&path).with_context(|| format!("could not remove {}", path.display()))?;
    let _ = std::fs::remove_file(path.with_extension("toml.synapsebackup"));
    Ok(path)
}
