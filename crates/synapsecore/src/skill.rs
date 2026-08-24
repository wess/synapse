mod install;
pub mod library;
mod model;
mod receipts;
mod record;

pub use install::{State, Status, install, remove, skillish, state, target};
pub use model::{ENTRY, Shelf, Skill, compose, validname};
pub use receipts::Receipts;
pub use record::{Proposal, Revision};

use anyhow::{Context, Result};

/// The install state of every library skill in every connected tool, plus any
/// skill a tool has that the library does not know about.
pub async fn survey(home: &std::path::Path) -> Result<(Vec<Status>, Vec<String>)> {
    let receipts = Receipts::open(crate::files::database()?).await?;
    let (skills, problems) = library::all()?;
    let waiting = receipts.proposals().await.unwrap_or_default();
    let mut statuses = Vec::new();
    for agent in crate::agent::agents(home) {
        for skill in &skills {
            // A tool with nowhere to keep a project skill gets no row for it.
            // "Not installed" would be a lie that reads like something to fix.
            if target(&agent, &skill.shelf, &skill.name).is_none() {
                continue;
            }
            statuses.push(Status {
                skill: skill.name.clone(),
                tool: agent.name.to_owned(),
                state: state(&receipts, &agent, skill).await?,
                path: target(&agent, &skill.shelf, &skill.name)
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                scope: skill.shelf.label().to_owned(),
                project: skill.shelf.root().unwrap_or_default().to_owned(),
                proposed: waiting
                    .iter()
                    .any(|item| item.shelf == skill.shelf.key() && item.skill == skill.name),
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
    let source = target(agent, &Shelf::Global, name)
        .with_context(|| format!("{} has no personal skills folder", agent.name))?;
    anyhow::ensure!(
        skillish(&source),
        "{} has no skill named `{name}`",
        agent.name
    );
    let destination = library::path(&Shelf::Global, name)?;
    anyhow::ensure!(
        !destination.join(ENTRY).exists(),
        "the library already has a skill named `{name}`"
    );
    let skill = model::read(&source, Shelf::Global)?;
    library::copy(&source, &destination, &skill.files)?;

    let adopted = library::read(&Shelf::Global, name)?;
    receipts
        .record(
            "",
            name,
            &agent.name,
            &source,
            &skill.digest,
            &adopted.digest,
        )
        .await?;
    Ok(destination)
}

/// Write a skill an agent worked out, and leave it waiting for a person.
///
/// It lands in the library and in no tool. That is the whole gate: the library
/// is Synapse's own folder, so writing there breaks nothing and costs nobody
/// context, while a skill's description is loaded into every session of every
/// tool that holds it — which is a bill the user has to agree to. So teaching
/// is free and installing is a decision, rather than the other way around.
pub async fn teach(
    receipts: &Receipts,
    shelf: &Shelf,
    name: &str,
    description: &str,
    body: &str,
    tool: &str,
    note: &str,
) -> Result<std::path::PathBuf> {
    let content = compose(name, description, body)?;
    let path = library::write(shelf, name, &content)?;
    receipts
        .propose(
            shelf.key(),
            name,
            shelf.root().unwrap_or_default(),
            tool,
            note.trim(),
        )
        .await?;
    Ok(path)
}

/// Replace a skill's instructions, keeping what it used to say.
///
/// Unlike a new skill this does reach the tools, and deliberately: the user
/// already agreed to this skill being loaded, and a correction that never
/// arrives leaves every session running the version that was wrong. It reaches
/// only copies Synapse itself wrote and nobody has edited since — a copy
/// somebody changed by hand is still theirs — and the previous text is kept, so
/// `skill revert` is always the way back.
pub async fn revise(
    receipts: &Receipts,
    home: &std::path::Path,
    skill: &Skill,
    description: Option<&str>,
    body: &str,
    tool: &str,
    note: &str,
) -> Result<(i64, Vec<String>)> {
    let entry = library::path(&skill.shelf, &skill.name)?.join(ENTRY);
    let previous = std::fs::read_to_string(&entry)
        .with_context(|| format!("could not read {}", entry.display()))?;
    let content = compose(&skill.name, description.unwrap_or(&skill.description), body)?;
    anyhow::ensure!(
        content != previous,
        "`{}` already says exactly that",
        skill.name
    );

    let revision = receipts
        .revised(skill.shelf.key(), &skill.name, &previous, note.trim(), tool)
        .await?;
    library::save(&skill.shelf, &skill.name, &content)?;
    let updated = library::read(&skill.shelf, &skill.name)?;
    Ok((revision, propagate(receipts, home, &updated).await?))
}

/// Put a revised skill into every tool already holding a copy Synapse wrote and
/// nobody has touched. Anything else is left where it is and reported by
/// `skill status`, which is where a copy somebody edited belongs.
pub async fn propagate(
    receipts: &Receipts,
    home: &std::path::Path,
    skill: &Skill,
) -> Result<Vec<String>> {
    let holders = receipts.holders(skill.shelf.key(), &skill.name).await?;
    let mut reached = Vec::new();
    for agent in crate::agent::agents(home) {
        if !holders.contains(&agent.name) {
            continue;
        }
        if state(receipts, &agent, skill).await?.protected() {
            continue;
        }
        if install(receipts, &agent, skill, false).await.is_ok() {
            reached.push(agent.name.clone());
        }
    }
    Ok(reached)
}

/// Take a skill back to what a revision says it used to be, keeping the version
/// being replaced. Reverting is itself a revision, so a revert can be reverted.
pub async fn revert(
    receipts: &Receipts,
    home: &std::path::Path,
    skill: &Skill,
    id: Option<i64>,
) -> Result<(i64, Vec<String>)> {
    let history = receipts.revisions(skill.shelf.key(), &skill.name).await?;
    let wanted = match id {
        Some(id) => {
            receipts
                .revision(skill.shelf.key(), &skill.name, id)
                .await?
        }
        None => history
            .into_iter()
            .next()
            .with_context(|| format!("`{}` has never been revised", skill.name))?,
    };
    let entry = library::path(&skill.shelf, &skill.name)?.join(ENTRY);
    let previous = std::fs::read_to_string(&entry)
        .with_context(|| format!("could not read {}", entry.display()))?;
    let revision = receipts
        .revised(
            skill.shelf.key(),
            &skill.name,
            &previous,
            &format!("reverted to revision {}", wanted.id),
            "",
        )
        .await?;
    library::save(&skill.shelf, &skill.name, &wanted.body)?;
    let restored = library::read(&skill.shelf, &skill.name)?;
    Ok((revision, propagate(receipts, home, &restored).await?))
}

/// Approve a proposed skill: install it where it belongs and stop calling it
/// proposed. Approving something that was never proposed is not an error — it
/// is an ordinary install, which is what the user asked for either way.
pub async fn approve(
    receipts: &Receipts,
    agents: &[crate::agent::Agent],
    skill: &Skill,
    replace: bool,
) -> Result<Vec<(String, Result<State>)>> {
    let mut results = Vec::new();
    for agent in agents {
        if target(agent, &skill.shelf, &skill.name).is_none() {
            continue;
        }
        results.push((
            agent.name.clone(),
            install(receipts, agent, skill, replace).await,
        ));
    }
    if results.iter().any(|(_, outcome)| outcome.is_ok()) {
        receipts.settle(skill.shelf.key(), &skill.name).await?;
    }
    Ok(results)
}

/// Turn a proposed skill down: it leaves the library with its history, and
/// nothing else on the machine ever knew about it.
///
/// Only a skill still waiting for review can be rejected. Once one is approved
/// it is an ordinary skill, and the way to remove it is `skill delete`, which
/// says what it does.
pub async fn reject(receipts: &Receipts, skill: &Skill) -> Result<std::path::PathBuf> {
    anyhow::ensure!(
        receipts
            .proposed(skill.shelf.key(), &skill.name)
            .await?
            .is_some(),
        "`{}` is not waiting for review; use `synapse skill delete` to remove it",
        skill.name
    );
    let path = library::delete(&skill.shelf, &skill.name)?;
    receipts
        .forgethistory(skill.shelf.key(), &skill.name)
        .await?;
    Ok(path)
}
