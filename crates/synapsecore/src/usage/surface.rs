//! A session's startup footprint: Synapse's guidance, its tool definitions,
//! the potential skill description cost, and the session hook's recall.
//! Nothing here has a budget except the recall, which is why it's counted.
//! Character counts are exact; tokens are an estimate.

use super::format::{plural, thousands};
use crate::brain::{Brain, Optimization};
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

const PERTOKEN: usize = super::PERTOKEN as usize;

/// Past this, guidance is reference material every session has to read.
const GUIDANCEGUIDE: usize = 12_000;

/// Every skill description together.
const SKILLSGUIDE: usize = 4_000;

/// One skill description, about two sentences. The rest belongs in the body,
/// which is only read when the skill is used.
const ONESKILLGUIDE: usize = 500;

/// The session hook's opening recall.
const RECALLGUIDE: usize = 8_000;

#[derive(Debug, Clone, Serialize)]
pub struct Surface {
    pub label: &'static str,
    pub detail: String,
    pub chars: usize,
    /// Set when the surface is over its guide, saying what to do about it.
    pub warning: Option<String>,
}

impl Surface {
    pub fn tokens(&self) -> usize {
        self.chars / PERTOKEN
    }
}

/// Measured as the model receives it. `root` scopes the startup recall to a
/// project; `None` counts global memory only.
pub async fn surfaces(brain: &Brain, soul: &Path, root: Option<&Path>) -> Result<Vec<Surface>> {
    let optimization = brain
        .settings()
        .await
        .map(|settings| settings.optimization)
        .unwrap_or_default();
    let mesh = brain.mesh().await.unwrap_or(false);
    let learn = brain.learn().await.unwrap_or(false);
    Ok(vec![
        guidance(soul, mesh, learn)?,
        tooldefinitions(mesh, learn),
        skills()?,
        startuprecall(brain, optimization, root).await,
    ])
}

/// SOUL.md plus whatever blocks the current settings add, since those are sent
/// too.
fn guidance(soul: &Path, mesh: bool, learn: bool) -> Result<Surface> {
    let text = crate::instructions::ensure(soul)?;
    let sent = crate::instructions::modelfacing(&text, mesh, learn);
    let chars = sent.chars().count();
    let extra = match (mesh, learn) {
        (true, true) => " + mesh and learn guidance",
        (true, false) => " + mesh guidance",
        (false, true) => " + learn guidance",
        (false, false) => "",
    };
    Ok(Surface {
        label: "Guidance",
        detail: format!("SOUL.md{extra}"),
        chars,
        warning: (chars > GUIDANCEGUIDE).then(|| {
            format!(
                "Guidance is {} characters, over the {} it is worth keeping to. \
                 Every session of every connected tool reads all of it. \
                 Move what is reference rather than instruction into a skill, \
                 which is read only when it is needed.",
                thousands(chars),
                thousands(GUIDANCEGUIDE)
            )
        }),
    })
}

/// Mesh and learn both gate the router, so switching one off takes its tools
/// out of every session. The detail says what the mesh switch is worth.
fn tooldefinitions(mesh: bool, learn: bool) -> Surface {
    let tools = crate::mcp::toolcost(mesh, learn);
    let chars: usize = tools.iter().map(|(_, size)| size).sum();
    let other: usize = crate::mcp::toolcost(!mesh, learn)
        .iter()
        .map(|(_, size)| size)
        .sum();
    let difference = thousands(chars.abs_diff(other) / PERTOKEN);
    let saved = match mesh {
        true => format!(" · mesh on, costing ~{difference} tokens"),
        false => format!(" · mesh off, saving ~{difference} tokens"),
    };
    Surface {
        label: "Tool schemas",
        detail: format!("{}{saved}", plural(tools.len(), "tool", "tools")),
        chars,
        warning: None,
    }
}

/// Names and descriptions in the library. This is potential cost: a tool only
/// loads the skills installed in it. Bodies are read on demand and don't count.
fn skills() -> Result<Surface> {
    let (skills, _) = crate::skill::library::all()?;
    let mut chars = 0;
    let mut longest: Option<(String, usize)> = None;
    for skill in &skills {
        let cost = skill.name.chars().count() + skill.description.chars().count();
        chars += cost;
        let description = skill.description.chars().count();
        if longest.as_ref().is_none_or(|(_, held)| description > *held) {
            longest = Some((skill.name.clone(), description));
        }
    }
    let overlong = longest.filter(|(_, size)| *size > ONESKILLGUIDE);
    Ok(Surface {
        label: "Skill library",
        detail: format!(
            "{} in library, if installed",
            plural(skills.len(), "skill", "skills")
        ),
        chars,
        warning: match (chars > SKILLSGUIDE, overlong) {
            (true, _) => Some(format!(
                "Skill descriptions total {} characters, over the {} they are worth keeping to. \
                 Every one is loaded eagerly by every tool holding it.",
                thousands(chars),
                thousands(SKILLSGUIDE)
            )),
            (false, Some((name, size))) => Some(format!(
                "`{name}`'s description is {size} characters. A description says when to \
                 reach for a skill; what it does belongs in the body, which is read only then."
            )),
            (false, None) => None,
        },
    })
}

async fn startuprecall(brain: &Brain, optimization: Optimization, root: Option<&Path>) -> Surface {
    let recalled = brain
        .recallscoped("", u32::MAX, Some(optimization), root)
        .await
        .map(|(_, memories)| memories)
        .unwrap_or_default();
    let chars: usize = recalled
        .iter()
        .map(|memory| memory.body.chars().count())
        .sum();
    let scope = match root {
        Some(_) => "",
        None => ", global only",
    };
    Surface {
        label: "Startup recall",
        detail: format!(
            "{} at {}{scope}",
            plural(recalled.len(), "memory", "memories"),
            optimization.name().to_lowercase()
        ),
        chars,
        // `full` caps nothing, which is fine on purpose and expensive by accident.
        warning: (chars > RECALLGUIDE).then(|| match optimization {
            Optimization::Full => format!(
                "The session opens with {} characters of recall, over the {}. \
                 The budget is `full`, which caps nothing — `synapse settings \
                 optimization balanced` holds it to eight memories and 6,000 characters.",
                thousands(chars),
                thousands(RECALLGUIDE)
            ),
            _ => format!(
                "The session opens with {} characters of recall, over the {}, \
                 even at the {} budget. Memories this long are worth splitting.",
                thousands(chars),
                thousands(RECALLGUIDE),
                optimization.name().to_lowercase()
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mesh_is_the_biggest_switch_and_its_worth_is_named() {
        let base = tooldefinitions(false, false);
        let withmesh = tooldefinitions(true, false);
        assert!(
            withmesh.chars > base.chars * 2,
            "{} vs {}",
            withmesh.chars,
            base.chars
        );
        assert!(base.detail.contains("saving"), "got {}", base.detail);
        assert!(!base.detail.contains("saving ~0 "), "got {}", base.detail);
        assert!(
            !withmesh.detail.contains("saving"),
            "got {}",
            withmesh.detail
        );
    }

    #[tokio::test]
    async fn startup_recall_says_when_it_counted_global_memory_only() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("app");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        brain
            .rememberscoped(
                "Use bun for every script here.",
                None,
                crate::brain::MemoryScope::Project,
                Some(&project),
            )
            .await
            .unwrap();
        brain
            .rememberscoped(
                "Credentials live in the vault.",
                None,
                crate::brain::MemoryScope::Global,
                None,
            )
            .await
            .unwrap();

        let global = startuprecall(&brain, Optimization::Balanced, None).await;
        assert!(
            global
                .detail
                .starts_with("1 memory at balanced, global only"),
            "{}",
            global.detail
        );

        let scoped = startuprecall(&brain, Optimization::Balanced, Some(&project)).await;
        assert_eq!(scoped.detail, "2 memories at balanced");
        assert!(scoped.chars > global.chars);
    }
}
