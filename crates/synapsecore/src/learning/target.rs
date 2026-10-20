use super::{Target, digest};
use crate::brain::{Brain, MemoryScope};
use crate::skill::{self, Shelf};
use anyhow::{Context, Result};
use std::path::Path;

impl Target {
    pub(crate) async fn fingerprint(&self, brain: &Brain, folder: &Path) -> Result<String> {
        let project = super::project(Some(folder))?;
        match self {
            Self::Memory { id } => {
                let memory = brain.memory(*id).await?.context("memory does not exist")?;
                anyhow::ensure!(
                    memory.scope == MemoryScope::Global || memory.project == project,
                    "memory is outside this project"
                );
                digest(&(
                    memory.scope,
                    memory.project,
                    memory.body,
                    memory.source,
                    memory.superseded,
                ))
            }
            Self::Skill { name, scope } => {
                skill::validname(name).map_err(|_| anyhow::anyhow!("invalid skill name"))?;
                let shelf = match scope {
                    MemoryScope::Global => Shelf::Global,
                    MemoryScope::Project => Shelf::project(folder),
                };
                let skill = skill::library::read(&shelf, name)?;
                digest(&(shelf.key(), skill.digest))
            }
        }
    }
}
