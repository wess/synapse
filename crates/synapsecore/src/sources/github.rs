//! A GitHub repository as a source of skills and MCP servers.
//!
//! One request to the trees API lists the whole repository, and the files come
//! from raw.githubusercontent.com, which does not count against GitHub's API
//! allowance. That allowance is sixty requests an hour without a token, so the
//! catalogue — every skill's name and description, every `server.json` — is
//! cached in the data folder for an hour, and a refresh whose tree has not
//! changed reads nothing else.

use crate::sources::Source;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const API: &str = "https://api.github.com";
const RAW: &str = "https://raw.githubusercontent.com";
const FRESH: Duration = Duration::from_secs(60 * 60);
/// A repository is a source, not an archive. Past this it is refused rather
/// than mirrored into the cache.
const MAXENTRIES: usize = 500;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Skill {
    /// The folder holding its `SKILL.md`, relative to the repository root.
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Server {
    pub path: String,
    /// The `server.json` as written, which is an MCP Registry entry.
    pub text: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Catalogue {
    sha: String,
    fetched: u64,
    files: Vec<(String, u64)>,
    skills: Vec<Skill>,
    servers: Vec<Server>,
}

#[derive(Deserialize)]
struct Tree {
    sha: String,
    #[serde(default)]
    truncated: bool,
    tree: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    size: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn cachepath(source: &Source) -> Result<std::path::PathBuf> {
    Ok(crate::files::data()?
        .join("cache")
        .join("sources")
        .join(format!("{}.json", source.name)))
}

fn reference(source: &Source) -> &str {
    if source.reference.is_empty() {
        "HEAD"
    } else {
        &source.reference
    }
}

async fn client(source: &Source) -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = crate::sources::token(source).await? {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {token}")
                .parse()
                .context("the source's token is not a valid header")?,
        );
    }
    reqwest::Client::builder()
        .user_agent(concat!("synapse/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .default_headers(headers)
        .build()
        .context("could not start an HTTP client")
}

/// One file of the repository, as text.
pub async fn raw(source: &Source, path: &str) -> Result<String> {
    let response = client(source)
        .await?
        .get(format!(
            "{RAW}/{}/{}/{path}",
            source.repo,
            reference(source)
        ))
        .send()
        .await
        .with_context(|| format!("could not reach GitHub for {}", source.repo))?;
    let status = response.status();
    anyhow::ensure!(
        status.is_success(),
        "GitHub answered {status} for {path} in {}",
        source.repo
    );
    response
        .text()
        .await
        .context("GitHub sent a file this version cannot read")
}

/// The repository's catalogue, from the cache while it is fresh.
async fn catalogue(source: &Source, force: bool) -> Result<Catalogue> {
    let path = cachepath(source)?;
    let cached: Option<Catalogue> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    if !force
        && let Some(cached) = &cached
        && now().saturating_sub(cached.fetched) < FRESH.as_secs()
    {
        return Ok(Catalogue {
            sha: cached.sha.clone(),
            fetched: cached.fetched,
            files: cached.files.clone(),
            skills: cached.skills.clone(),
            servers: cached.servers.clone(),
        });
    }
    let response = client(source)
        .await?
        .get(format!(
            "{API}/repos/{}/git/trees/{}?recursive=1",
            source.repo,
            reference(source)
        ))
        .send()
        .await
        .with_context(|| format!("could not reach GitHub for {}", source.repo))?;
    match response.status().as_u16() {
        200 => {}
        404 => anyhow::bail!(
            "GitHub has no repository {} it will show this machine{}",
            source.repo,
            if source.token.is_empty() {
                "; a private one needs --token"
            } else {
                ""
            }
        ),
        403 | 429 => anyhow::bail!(
            "GitHub is limiting requests from this address; try again within the hour, or give the source a token"
        ),
        code => anyhow::bail!("GitHub answered {code} for {}", source.repo),
    }
    let tree: Tree = response
        .json()
        .await
        .context("GitHub sent a tree this version cannot read")?;
    if let Some(cached) = cached
        && cached.sha == tree.sha
    {
        // Nothing moved: keep what was read, and only say so later.
        let refreshed = Catalogue {
            fetched: now(),
            ..cached
        };
        write(&path, &refreshed);
        return Ok(refreshed);
    }
    let prefix = match source.path.trim_matches('/') {
        "" => String::new(),
        folder => format!("{folder}/"),
    };
    let files: Vec<(String, u64)> = tree
        .tree
        .into_iter()
        .filter(|node| node.kind == "blob" && node.path.starts_with(&prefix))
        .map(|node| (node.path, node.size))
        .collect();
    anyhow::ensure!(
        files.len() <= MAXENTRIES * 20 && !tree.truncated,
        "{} is too large to use as a source; point it at the folder that holds the skills with a path",
        source.repo
    );
    let mut catalogue = Catalogue {
        sha: tree.sha,
        fetched: now(),
        files: files.clone(),
        ..Catalogue::default()
    };
    let entries: Vec<&String> = files
        .iter()
        .map(|(path, _)| path)
        .filter(|path| {
            path.ends_with("/SKILL.md") || *path == "SKILL.md" || path.ends_with("server.json")
        })
        .take(MAXENTRIES)
        .collect();
    for path in entries {
        let Ok(text) = raw(source, path).await else {
            continue;
        };
        if path.ends_with("SKILL.md") {
            let id = path
                .trim_end_matches("SKILL.md")
                .trim_end_matches('/')
                .to_owned();
            if let Some((name, description)) = describe(&text) {
                catalogue.skills.push(Skill {
                    id,
                    name,
                    description,
                });
            }
        } else {
            catalogue.servers.push(Server {
                path: path.clone(),
                text,
            });
        }
    }
    write(&path, &catalogue);
    Ok(catalogue)
}

/// Best effort: a cache that cannot be written is a slower next search, not
/// a failed one.
fn write(path: &std::path::Path, catalogue: &Catalogue) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(catalogue) {
        let _ = std::fs::write(path, text);
    }
}

/// A skill's name and description, or `None` for a file that is not one.
fn describe(text: &str) -> Option<(String, String)> {
    #[derive(Deserialize)]
    struct Front {
        name: Option<String>,
        description: Option<String>,
    }
    let (front, _) = crate::skill::split(text).ok()?;
    let front: Front = serde_saphyr::from_str(front).ok()?;
    let name = front.name?.trim().to_owned();
    crate::skill::validname(&name).ok()?;
    Some((
        name,
        front
            .description
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    ))
}

/// Read the catalogue again now rather than within the hour.
pub async fn refresh(source: &Source) -> Result<(usize, usize)> {
    let catalogue = catalogue(source, true).await?;
    Ok((catalogue.skills.len(), catalogue.servers.len()))
}

/// Every skill whose name or description holds every word of the query, name
/// matches first.
pub async fn skills(source: &Source, query: &str) -> Result<Vec<Skill>> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut found: Vec<Skill> = catalogue(source, false)
        .await?
        .skills
        .into_iter()
        .filter(|skill| {
            let haystack =
                format!("{} {} {}", skill.name, skill.description, skill.id).to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect();
    found.sort_by_key(|skill| {
        !words
            .iter()
            .any(|word| skill.name.to_lowercase().contains(word))
    });
    Ok(found)
}

/// Every file of one skill, relative to its folder.
pub async fn skillfiles(source: &Source, id: &str) -> Result<Vec<(String, String)>> {
    let catalogue = catalogue(source, false).await?;
    anyhow::ensure!(
        catalogue.skills.iter().any(|skill| skill.id == id),
        "{} has no skill at {id}",
        source.name
    );
    let prefix = match id {
        "" => String::new(),
        folder => format!("{folder}/"),
    };
    let mut files = Vec::new();
    for (path, _) in catalogue
        .files
        .iter()
        .filter(|(path, _)| path.starts_with(&prefix))
    {
        let relative = path[prefix.len()..].to_owned();
        // A nested skill is its own skill, not a file of this one.
        if relative.contains('/') && relative.ends_with("/SKILL.md") {
            continue;
        }
        files.push((relative, raw(source, path).await?));
    }
    Ok(files)
}

/// Every `server.json` the repository holds.
pub async fn servers(source: &Source) -> Result<Vec<Server>> {
    Ok(catalogue(source, false).await?.servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skill_in_a_repository_is_known_by_its_frontmatter() {
        let text = "---\nname: release-notes\ndescription: Write\n  release notes.\n---\n\nRead.\n";
        assert_eq!(
            describe(text),
            Some((
                "release-notes".to_owned(),
                "Write release notes.".to_owned()
            ))
        );
        assert_eq!(describe("# not a skill"), None);
        assert_eq!(describe("---\nname: ../up\n---\n\nx\n"), None);
    }
}
