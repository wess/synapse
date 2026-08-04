use crate::imports::{ImportCandidate, ImportProvider};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn scan(home: &Path) -> Result<Vec<ImportCandidate>> {
    let root = home.join(".claude").join("projects");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let projects = projectmap(home);
    let mut files = memoryfiles(&root)?;
    files.sort();
    let mut candidates = Vec::new();
    for file in files {
        let Some(encoded) = file
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
        else {
            continue;
        };
        let mapped = projects.get(encoded);
        let project = mapped.filter(|path| path.exists()).map(PathBuf::as_path);
        let mut imported = crate::imports::markdown::read(&file, ImportProvider::Claude, project)?;
        if let Some(path) = mapped.filter(|path| !path.exists()) {
            for candidate in &mut imported {
                candidate.warning = Some(format!(
                    "mapped project no longer exists: {}",
                    path.display()
                ));
            }
        } else if project.is_none() {
            for candidate in &mut imported {
                candidate.warning = Some("could not map this memory to a project root".to_owned());
            }
        }
        candidates.extend(imported);
    }
    Ok(candidates)
}

fn projectmap(home: &Path) -> HashMap<String, PathBuf> {
    let path = home.join(".claude.json");
    let Ok(content) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return HashMap::new();
    };
    value
        .get("projects")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|projects| projects.keys())
        .map(|project| (encode(Path::new(project)), PathBuf::from(project)))
        .collect()
}

fn encode(path: &Path) -> String {
    path.display().to_string().replace('/', "-")
}

fn memoryfiles(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for project in
        fs::read_dir(root).with_context(|| format!("could not read {}", root.display()))?
    {
        let memory = project?.path().join("memory");
        if !memory.is_dir() {
            continue;
        }
        for entry in
            fs::read_dir(&memory).with_context(|| format!("could not read {}", memory.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) == Some("md")
                && path.file_name().and_then(|value| value.to_str()) != Some("MEMORY.md")
            {
                files.push(path);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_leaf_memories_and_skips_the_index() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("work");
        fs::create_dir(&project).unwrap();
        let encoded = encode(&project);
        let memory = home
            .path()
            .join(".claude/projects")
            .join(encoded)
            .join("memory");
        fs::create_dir_all(&memory).unwrap();
        fs::write(memory.join("MEMORY.md"), "index").unwrap();
        fs::write(memory.join("choice.md"), "Keep the focused choice.").unwrap();
        fs::write(
            home.path().join(".claude.json"),
            serde_json::json!({"projects": {project.display().to_string(): {}}}).to_string(),
        )
        .unwrap();

        let candidates = scan(home.path()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].project,
            project.canonicalize().unwrap().display().to_string()
        );
    }

    #[test]
    fn flags_memories_for_projects_that_no_longer_exist() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("removed");
        let memory = home
            .path()
            .join(".claude/projects")
            .join(encode(&project))
            .join("memory");
        fs::create_dir_all(&memory).unwrap();
        fs::write(memory.join("choice.md"), "Keep the historical choice.").unwrap();
        fs::write(
            home.path().join(".claude.json"),
            serde_json::json!({"projects": {project.display().to_string(): {}}}).to_string(),
        )
        .unwrap();

        let candidates = scan(home.path()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].scope, crate::brain::MemoryScope::Global);
        assert!(
            candidates[0]
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("no longer exists"))
        );
    }
}
