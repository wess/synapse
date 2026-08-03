//! `synapseserve` — the sync server, as a process.
//!
//! Everything it needs comes from the environment, so a unit file or a
//! container is the whole of its configuration:
//!
//! - `SYNAPSESERVE_TOKEN`  the shared access token. Required; no default.
//! - `SYNAPSESERVE_DATA`   where the log lives. Defaults beside the process.
//! - `SYNAPSESERVE_ADDR`   what to bind. Defaults to loopback.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use synapseserve::{State, auth::Token, serve, store::Store};
use tokio::net::TcpListener;

const DEFAULTADDR: &str = "127.0.0.1:8787";
const DEFAULTDATA: &str = "synapseserve.db";

#[tokio::main]
async fn main() -> Result<()> {
    // Required rather than defaulted. A server that starts without a token is
    // a server somebody deploys without one.
    let token = std::env::var("SYNAPSESERVE_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .context("set SYNAPSESERVE_TOKEN to the shared access token before starting")?;
    let token = Token::new(token)?;

    let path = std::env::var("SYNAPSESERVE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULTDATA));
    let address = std::env::var("SYNAPSESERVE_ADDR").unwrap_or_else(|_| DEFAULTADDR.to_string());

    let store = Store::open(&path).await?;
    let head = store.head().await?;
    let listener = TcpListener::bind(&address)
        .await
        .with_context(|| format!("could not bind {address}"))?;

    // Loopback by default, and no TLS in this process. The envelopes are
    // sealed either way, but the access token is not — put a terminator in
    // front of it before it faces a network.
    println!("synapseserve listening on {address}");
    println!("log {} at sequence {head}", path.display());
    println!("plain http — terminate TLS in front of this");

    serve(
        listener,
        State {
            store: Arc::new(store),
            token: Arc::new(token),
        },
    )
    .await
}
