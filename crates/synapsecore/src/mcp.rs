mod learn;
mod mesh;
mod model;
mod server;

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
