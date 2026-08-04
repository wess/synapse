use crate::brain::MemoryScope;
use crate::imports::{ImportCandidate, ImportProvider};
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const CHUNK: usize = 6_000;

pub fn scan(path: &Path) -> Result<Vec<ImportCandidate>> {
    let files = markdownfiles(path)?;
    let mut candidates = Vec::new();
    for file in files {
        candidates.extend(read(&file, ImportProvider::Markdown, None)?);
    }
    Ok(candidates)
}

pub(crate) fn read(
    path: &Path,
    provider: ImportProvider,
    project: Option<&Path>,
) -> Result<Vec<ImportCandidate>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("could not read memory file {}", path.display()))?;
    let (frontmatter, body) = splitfrontmatter(&content);
    let name = frontmatter
        .as_ref()
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "memory".to_owned());
    let description = frontmatter
        .as_ref()
        .and_then(|value| value.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let mut body = body.trim().to_owned();
    if body.is_empty() {
        body = description.to_owned();
    } else if !description.is_empty() && !body.contains(description) {
        body = format!("{description}\n\n{body}");
    }
    if body.is_empty() {
        return Ok(Vec::new());
    }
    let (scope, project) = match project {
        Some(project) => (
            MemoryScope::Project,
            crate::brain::projectroot(project)?
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ),
        None => (MemoryScope::Global, String::new()),
    };
    let warning = instructionwarning(path).or_else(|| crate::imports::warning(&body));
    let created = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let chunks = chunks(&body);
    let count = chunks.len();
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(index, body)| ImportCandidate {
            provider,
            externalid: format!("{}#{index}", path.display()),
            body: if count > 1 {
                format!("# {name} · part {} of {count}\n\n{body}", index + 1)
            } else {
                body
            },
            source: format!("{} · {name}", provider.name()),
            scope,
            project: project.clone(),
            path: path.to_path_buf(),
            created,
            warning: warning.clone(),
        })
        .collect())
}

fn markdownfiles(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        anyhow::ensure!(
            path.extension().and_then(|value| value.to_str()) == Some("md"),
            "{} is not a Markdown file",
            path.display()
        );
        return Ok(vec![path.to_path_buf()]);
    }
    anyhow::ensure!(path.is_dir(), "{} does not exist", path.display());
    let mut pending = vec![path.to_path_buf()];
    let mut files = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in
            fs::read_dir(&folder).with_context(|| format!("could not read {}", folder.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn splitfrontmatter(content: &str) -> (Option<Value>, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return (None, content);
    };
    let Some(end) = rest.find("\n---\n") else {
        return (None, content);
    };
    let frontmatter = serde_saphyr::from_str::<Value>(&rest[..end]).ok();
    (frontmatter, &rest[end + 5..])
}

fn chunks(body: &str) -> Vec<String> {
    if body.chars().count() <= CHUNK {
        return vec![body.trim().to_owned()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for section in sections(body) {
        if current.chars().count() + section.chars().count() + 2 > CHUNK && !current.is_empty() {
            chunks.push(current.trim().to_owned());
            current.clear();
        }
        if section.chars().count() > CHUNK {
            for paragraph in section.split("\n\n") {
                if current.chars().count() + paragraph.chars().count() + 2 > CHUNK
                    && !current.is_empty()
                {
                    chunks.push(current.trim().to_owned());
                    current.clear();
                }
                if !current.is_empty() {
                    current.push_str("\n\n");
                }
                current.push_str(paragraph);
            }
        } else {
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(&section);
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_owned());
    }
    chunks
}

fn sections(body: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = String::new();
    for line in body.lines() {
        if line.starts_with('#') && line.contains(' ') && !current.trim().is_empty() {
            sections.push(current.trim().to_owned());
            current.clear();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        sections.push(current.trim().to_owned());
    }
    sections
}

fn instructionwarning(path: &Path) -> Option<String> {
    match path.file_name().and_then(|value| value.to_str()) {
        Some("AGENTS.md" | "CLAUDE.md" | "SOUL.md" | "MEMORY.md") => {
            Some("looks like an instruction or memory index file".to_owned())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frontmatter_and_splits_large_files() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("decision.md");
        fs::write(
            &file,
            format!(
                "---\nname: project-decision\ndescription: Keep this summary.\n---\n\n# One\n\n{}\n\n# Two\n\n{}",
                "first ".repeat(1_000),
                "second ".repeat(1_000)
            ),
        )
        .unwrap();

        let candidates = read(&file, ImportProvider::Claude, Some(directory.path())).unwrap();
        assert!(candidates.len() > 1);
        assert!(candidates[0].body.contains("project-decision"));
        assert_eq!(candidates[0].scope, MemoryScope::Project);
    }
}
