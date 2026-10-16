//! `synapse mcp run <name>`: what a tool actually starts.
//!
//! Its stdout is the MCP channel the tool is about to read, so nothing here
//! prints there — a word on stdout before the server's first message is a
//! corrupted handshake. Every failure goes to stderr, which every tool shows
//! as the server's log.

use crate::servers::library;
use crate::servers::model::Server;
use crate::vault::VaultStore;
use anyhow::{Context, Result};

/// The variables the server is started with: its plain settings, and each
/// secret fetched from the vault. Refuses while anything is still missing, so
/// a server never starts half-configured and fails somewhere less obvious.
pub async fn environment(server: &Server) -> Result<Vec<(String, String)>> {
    let missing = server.missing();
    anyhow::ensure!(
        missing.is_empty(),
        "`{}` still needs {}; set a secret with `synapse mcp secret {} <VARIABLE>`, or edit it with `synapse mcp edit {}`",
        server.name,
        missing.join(", "),
        server.name,
        server.name
    );
    let mut variables: Vec<(String, String)> = server
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if server.secrets.is_empty() {
        return Ok(variables);
    }
    let store = VaultStore::open(crate::files::database()?).await?;
    for (variable, reference) in &server.secrets {
        let secret = store.findsecret(reference).await?.with_context(|| {
            format!(
                "`{}` reads {variable} from {reference}, which is not in the vault",
                server.name
            )
        })?;
        let value = crate::vault::getsecret(&secret.account)
            .await
            .with_context(|| format!("could not read {reference} from the vault"))?;
        variables.push((variable.clone(), value));
    }
    Ok(variables)
}

/// Start the server in this process's place. Only returns on failure.
pub fn run(name: &str) -> Result<i32> {
    let server = library::read(name)?;
    let variables = tokio::runtime::Runtime::new()?.block_on(environment(&server))?;
    // The tool's PATH can be the narrow one a Finder-launched app inherits, and
    // `npx` lives under a version manager more often than not.
    let mut command = crate::agent::command(std::path::Path::new(&server.command));
    command.args(&server.args).envs(variables);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Replaces this process, so the tool and the server talk directly and
        // Synapse is gone from the path the moment the server is.
        let error = command.exec();
        Err(error).with_context(|| {
            format!(
                "could not start `{}` ({})",
                server.name,
                server.commandline()
            )
        })
    }
    #[cfg(not(unix))]
    {
        let status = command.status().with_context(|| {
            format!(
                "could not start `{}` ({})",
                server.name,
                server.commandline()
            )
        })?;
        Ok(status.code().unwrap_or(1))
    }
}
