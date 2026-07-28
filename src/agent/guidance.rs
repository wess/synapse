use crate::agent;
use crate::files;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const START: &str = "<!-- synapse:begin -->";
const END: &str = "<!-- synapse:end -->";
const OLDSTART: &str = "<!-- synaps:begin -->";
const OLDEND: &str = "<!-- synaps:end -->";

#[derive(Clone, Debug, Serialize)]
pub struct GuidanceState {
    pub path: PathBuf,
    pub exists: bool,
    pub synced: usize,
    pub total: usize,
    pub consolidated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct GuidanceReport {
    pub path: PathBuf,
    pub files: Vec<PathBuf>,
    pub moved: usize,
}

pub fn state(home: &Path, soul: &Path) -> GuidanceState {
    let agents = agent::agents(home);
    let synced = agents
        .iter()
        .filter(|agent| pointermatches(&agent.instructions, soul))
        .count();
    let consolidated = synced == agents.len()
        && agents.iter().all(|agent| {
            fs::read_to_string(&agent.instructions)
                .map(|content| stripmanaged(&content).trim().is_empty())
                .unwrap_or(false)
        });
    GuidanceState {
        path: soul.to_path_buf(),
        exists: soul.is_file(),
        synced,
        total: agents.len(),
        consolidated,
    }
}

pub fn sync(home: &Path, soul: &Path) -> Result<GuidanceReport> {
    crate::instructions::ensure(soul)?;
    let agents = agent::agents(home);
    for agent in &agents {
        writepointer(&agent.instructions, soul, false)?;
    }
    Ok(GuidanceReport {
        path: soul.to_path_buf(),
        files: agents.into_iter().map(|agent| agent.instructions).collect(),
        moved: 0,
    })
}

pub fn adopt(home: &Path, soul: &Path) -> Result<GuidanceReport> {
    let agents = agent::agents(home);
    let mut snapshots = Vec::with_capacity(agents.len() + 1);
    snapshots.push(files::Snapshot::capture(soul)?);
    for agent in &agents {
        snapshots.push(files::Snapshot::capture(&agent.instructions)?);
    }
    let result = (|| {
        let existing = crate::instructions::ensure(soul)?;
        let guidance = agents
            .iter()
            .filter_map(|agent| fs::read_to_string(&agent.instructions).ok())
            .map(|content| stripmanaged(&content).trim().to_owned())
            .filter(|content| !content.is_empty())
            .collect::<Vec<_>>();
        let (merged, moved) = mergeguidance(&existing, &guidance);
        files::write(soul, &merged)?;
        for agent in &agents {
            writepointer(&agent.instructions, soul, true)?;
        }
        Ok(GuidanceReport {
            path: soul.to_path_buf(),
            files: agents
                .iter()
                .map(|agent| agent.instructions.clone())
                .collect(),
            moved,
        })
    })();
    if let Err(error) = result {
        for snapshot in snapshots.iter().rev() {
            if let Err(rollback) = snapshot.restore() {
                return Err(error).context(format!("guidance rollback also failed: {rollback:#}"));
            }
        }
        return Err(error);
    }
    result
}

pub fn writepointer(path: &Path, soul: &Path, pointeronly: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let current = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let unmanaged = if pointeronly {
        String::new()
    } else {
        stripmanaged(&current).trim().to_owned()
    };
    let block = pointerblock(soul);
    let merged = if unmanaged.is_empty() {
        format!("{block}\n")
    } else {
        format!("{unmanaged}\n\n{block}\n")
    };
    files::write(path, &merged).with_context(|| format!("could not update {}", path.display()))
}

fn pointerblock(soul: &Path) -> String {
    format!("{START}\n{}\n{END}", crate::instructions::pointer(soul))
}

fn pointermatches(path: &Path, soul: &Path) -> bool {
    fs::read_to_string(path)
        .map(|content| content.contains(&pointerblock(soul)))
        .unwrap_or(false)
}

fn stripmanaged(content: &str) -> String {
    stripone(&stripone(content, START, END), OLDSTART, OLDEND)
}

fn stripone(content: &str, startmarker: &str, endmarker: &str) -> String {
    let mut remaining = content.to_owned();
    while let Some(start) = remaining.find(startmarker) {
        let Some(relative) = remaining[start..].find(endmarker) else {
            break;
        };
        let end = start + relative + endmarker.len();
        remaining.replace_range(start..end, "");
    }
    remaining
}

fn mergeguidance(existing: &str, guidance: &[String]) -> (String, usize) {
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for content in guidance {
        if seen.insert(content.clone()) && !existing.contains(content) {
            unique.push(content.clone());
        }
    }
    if unique.is_empty() {
        return (ensurenewline(existing), 0);
    }
    let moved = unique.len();
    let merged = if unique.iter().all(|content| simpleheadings(content)) {
        let mut lines = Vec::new();
        let mut seen = HashSet::new();
        for content in &unique {
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                if seen.insert(line.trim().to_owned()) {
                    lines.push(line.trim().to_owned());
                }
            }
        }
        format!(
            "{}\n\n## Working preferences\n\n{}\n",
            existing.trim_end(),
            lines.join("\n")
        )
    } else {
        let sections = unique
            .iter()
            .enumerate()
            .map(|(index, content)| format!("### Imported source {}\n\n{content}", index + 1))
            .collect::<Vec<_>>()
            .join("\n\n");
        format!(
            "{}\n\n## Working preferences\n\n{}\n",
            existing.trim_end(),
            sections
        )
    };
    (merged, moved)
}

fn simpleheadings(content: &str) -> bool {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| line.trim_start().starts_with("# "))
}

fn ensurenewline(content: &str) -> String {
    format!("{}\n", content.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_replaces_legacy_blocks_and_keeps_user_guidance() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("AGENTS.md");
        let soul = directory.path().join("SOUL.md");
        fs::write(
            &file,
            "# Keep this\n\n<!-- synaps:begin -->\nold\n<!-- synaps:end -->\n",
        )
        .unwrap();

        writepointer(&file, &soul, false).unwrap();

        let content = fs::read_to_string(file).unwrap();
        assert!(content.contains("# Keep this"));
        assert!(content.contains(soul.to_str().unwrap()));
        assert!(!content.contains("synaps:begin"));
    }

    #[test]
    fn adoption_deduplicates_simple_shared_rules() {
        let existing = crate::instructions::DEFAULT;
        let guidance = vec![
            "# Use Bun\n# Keep files small".to_owned(),
            "# Use Bun\n# Avoid secrets".to_owned(),
        ];
        let (merged, moved) = mergeguidance(existing, &guidance);
        assert_eq!(moved, 2);
        assert_eq!(merged.matches("# Use Bun").count(), 1);
        assert!(merged.contains("# Avoid secrets"));
    }

    #[test]
    fn missing_global_files_are_not_consolidated() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path(), &directory.path().join("SOUL.md"));

        assert_eq!(state.synced, 0);
        assert!(!state.consolidated);
    }
}
