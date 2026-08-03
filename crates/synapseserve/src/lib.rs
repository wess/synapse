//! The Synapse sync server.
//!
//! It holds an append-only log of sealed envelopes and hands back the ones a
//! client has not seen. It cannot read a memory, cannot tell a stored memory
//! from a deleted one, and resolves no conflicts — clients do that, because
//! only they can see the timestamps.
//!
//! Nothing here is required to use Synapse. A machine with no server
//! configured keeps every memory locally and loses no capability, which is
//! also why an outage is not an emergency.

pub mod auth;
pub mod http;
pub mod store;

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::net::TcpListener;

/// What the handlers share. Cheap to clone: the store is behind an `Arc` and
/// the token is a short string.
#[derive(Clone)]
pub struct State {
    pub store: Arc<store::Store>,
    pub token: Arc<auth::Token>,
}

/// Serve until the process is asked to stop.
///
/// Takes an already-bound listener so a caller can bind port 0 and read back
/// the address, which is what the tests do.
pub async fn serve(listener: TcpListener, state: State) -> Result<()> {
    axum::serve(listener, http::router(state))
        .await
        .context("the sync server stopped")
}
