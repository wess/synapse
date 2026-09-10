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
    /// Files carrying a Synapse block written by an older release. They need a
    /// sync to gain the current session notice, which is a different problem
    /// from never having been set up at all.
    pub stale: usize,
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
        .filter(|agent| pointermatches(&agent.instructions, soul, needsnotice(agent)))
        .count();
    let stale = agents
        .iter()
        .filter(|agent| pointerstale(&agent.instructions, soul, needsnotice(agent)))
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
        stale,
        total: agents.len(),
        consolidated,
    }
}

pub fn sync(home: &Path, soul: &Path) -> Result<GuidanceReport> {
    crate::instructions::ensure(soul)?;
    let agents = agent::agents(home);
    for agent in &agents {
        writepointer(&agent.instructions, soul, false, needsnotice(agent))?;
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
            writepointer(&agent.instructions, soul, true, needsnotice(agent))?;
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

pub fn writepointer(path: &Path, soul: &Path, pointeronly: bool, announce: bool) -> Result<()> {
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
    let block = pointerblock(soul, announce);
    let merged = if unmanaged.is_empty() {
        format!("{block}\n")
    } else {
        format!("{unmanaged}\n\n{block}\n")
    };
    files::write(path, &merged).with_context(|| format!("could not update {}", path.display()))
}

/// Take the Synapse block back out of a tool's instruction file, leaving
/// everything the user wrote around it exactly where it was.
///
/// Returns whether there was a block to remove. A file Synapse never wrote to
/// is left untouched rather than rewritten, so disconnecting cannot reformat
/// somebody's instructions as a side effect.
pub fn removepointer(path: &Path) -> Result<bool> {
    let Ok(current) = fs::read_to_string(path) else {
        return Ok(false);
    };
    let stripped = stripmanaged(&current);
    if stripped == current {
        return Ok(false);
    }
    let trimmed = stripped.trim();
    let content = if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    };
    files::write(path, &content).with_context(|| format!("could not update {}", path.display()))?;
    Ok(true)
}

/// Whether this tool's block has to carry the session-start notice, or whether
/// the tool says it for itself. See [`crate::agent::Kind::announces`].
pub fn needsnotice(agent: &agent::Agent) -> bool {
    !agent.kind.announces()
}

fn pointerblock(soul: &Path, announce: bool) -> String {
    format!(
        "{START}\n{}\n{END}",
        crate::instructions::managed(soul, announce)
    )
}

/// Whether one tool's instruction file carries a current pointer, so a
/// report can say which tool is set up rather than only how many are.
pub fn pointermatches(path: &Path, soul: &Path, announce: bool) -> bool {
    fs::read_to_string(path)
        .map(|content| content.contains(&pointerblock(soul, announce)))
        .unwrap_or(false)
}

fn pointerstale(path: &Path, soul: &Path, announce: bool) -> bool {
    fs::read_to_string(path)
        .map(|content| {
            (content.contains(START) || content.contains(OLDSTART))
                && !content.contains(&pointerblock(soul, announce))
        })
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

        writepointer(&file, &soul, false, true).unwrap();

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

    /// For a tool with nothing else to say it with. The block is the only thing
    /// loaded before the first reply, so an announcement placed behind the
    /// pointer arrives too late to be printed.
    #[test]
    fn the_managed_block_carries_the_notice_so_it_loads_without_a_read() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("AGENTS.md");
        let soul = directory.path().join("SOUL.md");

        writepointer(&file, &soul, false, true).unwrap();

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains(soul.to_str().unwrap()));
        assert!(content.contains("Synapse connected"));
        assert!(content.contains("Synapse unavailable"));
        assert!(pointermatches(&file, &soul, true));
        assert!(!pointerstale(&file, &soul, true));
    }

    /// Claude Code's session hook prints the notice itself, with the real
    /// count, before the model has written anything. A block that asks for the
    /// line as well is how the user gets it twice — and the model's half is the
    /// guessed one, because `recall` counts query hits and not the store.
    #[test]
    fn a_tool_that_announces_itself_gets_a_block_that_does_not_ask_for_the_line() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("CLAUDE.md");
        let soul = directory.path().join("SOUL.md");

        writepointer(&file, &soul, false, false).unwrap();

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains(soul.to_str().unwrap()));
        assert!(!content.contains("Synapse connected"), "got {content}");
        assert!(!content.contains("first reply"), "got {content}");
        assert!(pointermatches(&file, &soul, false));
        assert!(!pointerstale(&file, &soul, false));
    }

    /// The two blocks are different text, so a machine connected under the
    /// release that asked Claude Code for the line reads as stale and gets the
    /// quiet block on the next sync rather than keeping the old one forever.
    #[test]
    fn a_notice_carrying_block_reads_as_stale_for_a_tool_that_announces_itself() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("CLAUDE.md");
        let soul = directory.path().join("SOUL.md");

        writepointer(&file, &soul, false, true).unwrap();

        assert!(pointerstale(&file, &soul, false));
        assert!(!pointermatches(&file, &soul, false));

        writepointer(&file, &soul, false, false).unwrap();

        assert!(pointermatches(&file, &soul, false));
        assert!(!pointerstale(&file, &soul, false));
        assert_eq!(fs::read_to_string(&file).unwrap().matches(START).count(), 1);
    }

    /// Claude Code and pi say it themselves; anything else has only the block.
    #[test]
    fn only_a_tool_without_its_own_notice_needs_one_in_its_block() {
        let home = tempfile::tempdir().unwrap();
        for agent in agent::agents(home.path()) {
            assert_eq!(
                needsnotice(&agent),
                !matches!(agent.kind, agent::Kind::Claude | agent::Kind::Pi),
                "{}",
                agent.slug
            );
        }
    }

    #[test]
    fn a_block_from_an_older_release_reads_as_stale_rather_than_missing() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("CLAUDE.md");
        let soul = directory.path().join("SOUL.md");
        // The pointer-only block every release before this one wrote.
        fs::write(
            &file,
            format!("{START}\n{}\n{END}\n", crate::instructions::pointer(&soul)),
        )
        .unwrap();

        assert!(!pointermatches(&file, &soul, true));
        assert!(pointerstale(&file, &soul, true));

        writepointer(&file, &soul, false, true).unwrap();

        assert!(pointermatches(&file, &soul, true));
        assert!(!pointerstale(&file, &soul, true));
        assert_eq!(fs::read_to_string(&file).unwrap().matches(START).count(), 1);
    }

    #[test]
    fn missing_global_files_are_not_consolidated() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path(), &directory.path().join("SOUL.md"));

        assert_eq!(state.synced, 0);
        assert!(!state.consolidated);
    }
}
