mod install;
pub mod library;
mod model;
mod receipts;

pub use install::{State, Status, install, remove, skillish, state, target};
pub use model::{ENTRY, validname};
pub use receipts::Receipts;

use anyhow::Result;

/// The install state of every library skill in every connected tool, plus any
/// skill a tool has that the library does not know about.
pub async fn survey(home: &std::path::Path) -> Result<(Vec<Status>, Vec<String>)> {
    let receipts = Receipts::open(crate::files::database()?).await?;
    let (skills, problems) = library::all()?;
    let mut statuses = Vec::new();
    for agent in crate::agent::agents(home) {
        for skill in &skills {
            statuses.push(Status {
                skill: skill.name.clone(),
                tool: agent.name.to_owned(),
                state: state(&receipts, &agent, skill).await?,
                path: target(&agent, &skill.name).display().to_string(),
            });
        }
    }
    Ok((statuses, problems))
}

/// Skills a tool has that the library does not, so `status` can point at
/// something worth adopting rather than pretending the tool is empty.
pub fn unknown(agent: &crate::agent::Agent, known: &[String]) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(&agent.skills) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') || known.iter().any(|item| item == name) {
            continue;
        }
        if skillish(&path) {
            found.push(name.to_owned());
        }
    }
    found.sort();
    found
}

/// Copy a skill a tool already has into the library, which is how an existing
/// hand-made skill becomes one Synapse can keep in step everywhere.
///
/// The tool it came from is recorded as already having it. The two copies are
/// identical at this moment, and without the receipt Synapse would go on
/// treating the original as somebody else's and refuse to keep it in step —
/// which is the opposite of what adopting it was for.
pub async fn adopt(
    receipts: &Receipts,
    agent: &crate::agent::Agent,
    name: &str,
) -> Result<std::path::PathBuf> {
    validname(name)?;
    let source = target(agent, name);
    anyhow::ensure!(
        skillish(&source),
        "{} has no skill named `{name}`",
        agent.name
    );
    let destination = library::path(name)?;
    anyhow::ensure!(
        !destination.join(ENTRY).exists(),
        "the library already has a skill named `{name}`"
    );
    let skill = model::read(&source)?;
    library::copy(&source, &destination, &skill.files)?;

    let adopted = library::read(name)?;
    receipts
        .record(name, agent.name, &source, &skill.digest, &adopted.digest)
        .await?;
    Ok(destination)
}
