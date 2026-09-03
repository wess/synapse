//! The four things a person does to a connection, in one place.
//!
//! Connecting is not one write — it is the tool's own CLI registering a server,
//! a managed block in an instruction file, Claude Code's hooks, and now a
//! record of what descriptor it was all done from. Every surface needs the same
//! four verbs over that, and a verb assembled separately in the window, the
//! terminal, and the CLI is a verb that means three things by next year.
//!
//! So the composition lives here and the surfaces only choose which one to
//! call:
//!
//! - [`connect`] wires a tool in, leaving a working registration alone.
//! - [`refresh`] writes the registration again, for a descriptor that moved in
//!   a release.
//! - [`reset`] takes the connection out and makes it again, for when refreshing
//!   is not enough because what is there has to go first.
//! - [`remove`] takes it out and leaves it out.
//!
//! Each one keeps the connection record in step, because a record that survives
//! its connection is worse than no record: it reports a tool as current the
//! moment somebody disconnects it.

use crate::agent::{Agent, Detection, Removed, receipt};
use anyhow::Result;
use std::path::Path;

/// Wire a tool in. An existing registration that already points here is left
/// as it is; everything else — the guidance pointer, the notice — is brought up
/// to date, which is what makes running this twice useful rather than wasted.
pub async fn connect(
    agent: &Agent,
    detection: &Detection,
    server: &Path,
    soul: &Path,
    database: &Path,
) -> Result<()> {
    super::setup(agent, detection, server, soul)?;
    remember(agent, database).await;
    Ok(())
}

/// Apply this release's descriptor to a tool that is already connected.
///
/// The registration is written again rather than skipped, because detection
/// cannot see the difference between a descriptor that changed and one that did
/// not — it only knows an entry is present and points at this binary. Nothing
/// is removed: the skills stay, the instruction file keeps its own text, and a
/// tool that was connected stays connected the whole way through.
pub async fn refresh(
    agent: &Agent,
    detection: &Detection,
    server: &Path,
    soul: &Path,
    database: &Path,
) -> Result<()> {
    super::reapply(agent, detection, server, soul)?;
    remember(agent, database).await;
    Ok(())
}

/// Take the connection out and make it again.
///
/// The bigger hammer, and the one that costs something: disconnecting removes
/// the skills Synapse installed for this tool, so they are installed again by
/// whatever puts them back — this puts the connection back, not the library.
/// Use it when what is on disk has to go before the new answer can land;
/// [`refresh`] is the one to reach for first.
///
/// The teardown's report comes back rather than being thrown away, because a
/// step that failed on the way out is the most useful thing to say about a
/// reset that then appeared to succeed.
///
/// Takes no [`Detection`] on purpose: whatever was true before the teardown is
/// wrong after it, so this reads the machine again rather than being handed a
/// stale answer that would skip the very write it exists to make.
pub async fn reset(agent: &Agent, server: &Path, soul: &Path, database: &Path) -> Result<Removed> {
    let removed = super::disconnect(agent, server).await;
    let _ = receipt::forget(database, &agent.slug).await;
    let after = super::detect(agent, Some(server));
    super::reapply(agent, &after, server, soul)?;
    remember(agent, database).await;
    Ok(removed)
}

/// Take a tool back out and leave it out.
pub async fn remove(agent: &Agent, server: &Path, database: &Path) -> Removed {
    let removed = super::disconnect(agent, server).await;
    let _ = receipt::forget(database, &agent.slug).await;
    removed
}

/// Record what the tool was just connected from.
///
/// Deliberately not fatal. The connection is made and working at this point;
/// failing the whole operation because a note about it could not be written
/// would turn a cosmetic problem into a red error over a green outcome. The
/// cost of losing it is that the row says nothing about being out of date,
/// which is exactly what it says on a machine that connected before this
/// release existed.
async fn remember(agent: &Agent, database: &Path) {
    let root = super::catalog::projectroot();
    if let Ok((text, _)) = super::tool::text(root.as_deref(), &agent.slug) {
        let _ = receipt::record(database, &agent.slug, &text).await;
    }
}
