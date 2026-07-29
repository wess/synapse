mod server;

pub async fn run() -> anyhow::Result<()> {
    let database = crate::files::database()?;
    let guidance = crate::instructions::ensure(&crate::files::soul()?)?;
    let instructions = crate::instructions::modelfacing(&guidance);
    let brain = crate::brain::Brain::open(&database).await?;
    let vaults = crate::vault::VaultStore::open(&database).await?;
    server::run(brain, vaults, instructions).await
}
