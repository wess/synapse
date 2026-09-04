mod learn;
mod mesh;
mod model;
mod server;

/// What the tool definitions cost a session at these settings, per tool, in
/// bytes of JSON. The mesh's sixteen and self-improvement's two are the whole
/// reason those settings gate the router rather than the handler — this is how
/// somebody sees what that saves them.
pub fn toolcost(mesh: bool, learn: bool) -> Vec<(String, usize)> {
    server::Server::definitions(mesh, learn)
}

pub async fn run() -> anyhow::Result<()> {
    let database = crate::files::database()?;
    let guidance = crate::instructions::ensure(&crate::files::soul()?)?;
    let brain = crate::brain::Brain::open(&database).await?;
    let vaults = crate::vault::VaultStore::open(&database).await?;
    // Read once at startup: the tool list a client sees is fixed for the life of
    // its session, so a mesh switched on mid-session takes effect the next time
    // that tool starts.
    let enabled = brain.mesh().await?;
    let mesh = if enabled {
        let mesh = crate::relay::Mesh::open(&database).await?;
        // Clear workers left behind by a session that was killed rather than
        // closed, before this one starts adding its own.
        let _ = crate::relay::reapstrays(&mesh).await;
        Some(mesh)
    } else {
        None
    };
    // Read once at startup for the same reason the mesh is: the tool list a
    // client sees is fixed for the life of its session.
    let learning = brain.learn().await?;
    let receipts = match learning {
        true => Some(crate::skill::Receipts::open(&database).await?),
        false => None,
    };
    let instructions = crate::instructions::modelfacing(&guidance, enabled, learning);
    server::run(brain, vaults, mesh, receipts, instructions).await
}
