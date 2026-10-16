use crate::sources::model::{Kind, Origin, Source, validname};
use crate::vault::VaultStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// A repository's own sources, relative to its root.
pub const PROJECTFILE: &str = ".synapse/sources.toml";

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    source: Vec<Source>,
}

fn builtins() -> Vec<Source> {
    vec![
        Source {
            name: "skills.sh".to_owned(),
            kind: Kind::SkillsIndex,
            url: "https://skills.sh".to_owned(),
            repo: String::new(),
            reference: String::new(),
            path: String::new(),
            token: String::new(),
            origin: Origin::Builtin,
        },
        Source {
            name: "mcp-registry".to_owned(),
            kind: Kind::McpRegistry,
            url: "https://registry.modelcontextprotocol.io".to_owned(),
            repo: String::new(),
            reference: String::new(),
            path: String::new(),
            token: String::new(),
            origin: Origin::Builtin,
        },
    ]
}

pub fn userfile() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("sources.toml"))
}

fn parse(path: &Path, text: &str, origin: &Origin) -> Result<Vec<Source>> {
    let file: File =
        toml::from_str(text).with_context(|| format!("could not read {}", path.display()))?;
    let mut sources = file.source;
    for source in &mut sources {
        source
            .validate()
            .with_context(|| format!("in {}", path.display()))?;
        source.origin = origin.clone();
    }
    Ok(sources)
}

/// The person's own list, or the built-ins when they have never changed it.
fn user() -> Result<Vec<Source>> {
    let path = userfile()?;
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&path, &text, &Origin::User),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(builtins()),
        Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
    }
}

/// The sources file of the repository holding `folder`, if it has one.
pub fn projectfile(folder: &Path) -> Option<PathBuf> {
    let root = crate::brain::projectroot(folder).ok().flatten()?;
    let path = root.join(PROJECTFILE);
    path.is_file().then_some(path)
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// The repository's sources, each marked trusted or not by whether the file's
/// exact contents were approved. An edited file reverts to untrusted on its
/// own, which is the point: approval is of what was read, not of the path.
pub async fn project(folder: &Path) -> Result<Option<Vec<Source>>> {
    let Some(path) = projectfile(folder) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let store = VaultStore::glance(crate::files::database()?).await?;
    let trusted = store.digest(&path).await?.as_deref() == Some(digest(&text).as_str());
    parse(
        &path,
        &text,
        &Origin::Project {
            file: path.clone(),
            trusted,
        },
    )
    .map(Some)
}

/// Every source, in order: the person's own (or the built-ins), then the
/// repository's. A project source whose name is already taken is skipped and
/// reported, because `team:release` must mean one thing.
pub async fn resolved(folder: Option<&Path>) -> Result<(Vec<Source>, Vec<String>)> {
    let mut sources = user()?;
    let mut problems = Vec::new();
    if let Some(folder) = folder {
        match project(folder).await {
            Ok(Some(found)) => {
                for source in found {
                    if sources.iter().any(|existing| existing.name == source.name) {
                        problems.push(format!(
                            "the project's `{}` source has a name already in use, so it was skipped",
                            source.name
                        ));
                        continue;
                    }
                    sources.push(source);
                }
            }
            Ok(None) => {}
            Err(error) => problems.push(format!("{error:#}")),
        }
    }
    Ok((sources, problems))
}

fn save(sources: &[Source]) -> Result<PathBuf> {
    let path = userfile()?;
    let file = File {
        source: sources
            .iter()
            .filter(|source| !matches!(source.origin, Origin::Project { .. }))
            .cloned()
            .collect(),
    };
    let text = toml::to_string_pretty(&file).context("could not write the sources")?;
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    crate::files::write(&path, &text)?;
    Ok(path)
}

/// Add a source to the person's own list. The first change writes the
/// built-ins out with it, so removing one later is an ordinary edit.
pub fn add(source: Source) -> Result<PathBuf> {
    source.validate()?;
    let mut sources = user()?;
    anyhow::ensure!(
        !sources.iter().any(|existing| existing.name == source.name),
        "there is already a source named `{}`",
        source.name
    );
    sources.push(Source {
        origin: Origin::User,
        ..source
    });
    save(&sources)
}

pub fn remove(name: &str) -> Result<PathBuf> {
    let mut sources = user()?;
    let before = sources.len();
    sources.retain(|source| source.name != name);
    anyhow::ensure!(
        sources.len() < before,
        "there is no source named `{name}` in your own list; a project's sources are edited in its {PROJECTFILE}"
    );
    save(&sources)
}

/// Approve the repository's sources file as it reads right now.
pub async fn allow(folder: &Path) -> Result<PathBuf> {
    let path = projectfile(folder).with_context(|| format!("this project has no {PROJECTFILE}"))?;
    let text = std::fs::read_to_string(&path)?;
    parse(
        &path,
        &text,
        &Origin::Project {
            file: path.clone(),
            trusted: false,
        },
    )?;
    let store = VaultStore::open(crate::files::database()?).await?;
    store.trust(&path, &digest(&text)).await?;
    Ok(path)
}

/// The token a private source reads with, from the vault.
pub async fn token(source: &Source) -> Result<Option<String>> {
    if source.token.is_empty() {
        return Ok(None);
    }
    let store = VaultStore::glance(crate::files::database()?).await?;
    let secret = store.findsecret(&source.token).await?.with_context(|| {
        format!(
            "`{}` reads its token from {}, which is not in the vault",
            source.name, source.token
        )
    })?;
    Ok(Some(crate::vault::getsecret(&secret.account).await?))
}

/// Work out what kind of source a location is.
///
/// `owner/repo` or a github.com address is a repository and needs no request.
/// An address is asked: an MCP registry answers `/v0/servers`, a skills index
/// answers `/api/search`. Asking is the only way to tell them apart, and it
/// means a typo is caught when the source is added rather than on every search.
pub async fn infer(name: &str, location: &str, kind: Option<Kind>) -> Result<Source> {
    validname(name)?;
    let location = location.trim().trim_end_matches('/');
    let mut source = Source {
        name: name.to_owned(),
        kind: Kind::Github,
        url: String::new(),
        repo: String::new(),
        reference: String::new(),
        path: String::new(),
        token: String::new(),
        origin: Origin::User,
    };
    if kind == Some(Kind::Github) || kind.is_none() && github(location).is_some() {
        let (repo, reference, path) = github(location)
            .with_context(|| format!("`{location}` is not a repository; expected owner/repo"))?;
        source.repo = repo;
        source.reference = reference;
        source.path = path;
        source.validate()?;
        return Ok(source);
    }
    source.url = location.to_owned();
    let client = reqwest::Client::builder()
        .user_agent(concat!("synapse/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let answers = |path: &'static str, key: &'static str| {
        let client = client.clone();
        let url = format!("{location}{path}");
        async move {
            let Ok(response) = client.get(&url).send().await else {
                return false;
            };
            if !response.status().is_success() {
                return false;
            }
            response
                .json::<serde_json::Value>()
                .await
                .is_ok_and(|value| value.get(key).is_some())
        }
    };
    source.kind = match kind {
        Some(kind) => kind,
        None if answers("/v0/servers?limit=1", "servers").await => Kind::McpRegistry,
        None if answers("/api/search?q=skill&limit=1", "skills").await => Kind::SkillsIndex,
        None => anyhow::bail!(
            "`{location}` answers neither as an MCP registry nor as a skills index; give --kind if it is one"
        ),
    };
    source.validate()?;
    Ok(source)
}

/// `owner/repo`, `owner/repo@ref/path`, or a github.com address with an
/// optional `/tree/<ref>/<path>`.
fn github(location: &str) -> Option<(String, String, String)> {
    let bare = location
        .strip_prefix("https://")
        .or_else(|| location.strip_prefix("http://"))
        .unwrap_or(location);
    let bare = bare.strip_prefix("www.").unwrap_or(bare);
    let (rest, address) = match bare.strip_prefix("github.com/") {
        Some(rest) => (rest, true),
        None => (bare, false),
    };
    if !address && (location.contains("://") || rest.contains('.') && !rest.contains('/')) {
        return None;
    }
    let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    let owner = parts[0];
    let (repo, mut reference) = match parts[1].split_once('@') {
        Some((repo, reference)) => (repo, reference.to_owned()),
        None => (parts[1], String::new()),
    };
    let mut path: Vec<&str> = parts[2..].to_vec();
    if address && matches!(path.first(), Some(&"tree") | Some(&"blob")) && path.len() >= 2 {
        reference = path[1].to_owned();
        path.drain(..2);
    }
    if !address && owner.contains('.') {
        return None;
    }
    Some((
        format!("{owner}/{}", repo.trim_end_matches(".git")),
        reference,
        path.join("/"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_is_read_from_every_way_a_person_writes_one() {
        assert_eq!(
            github("anthropics/skills"),
            Some(("anthropics/skills".to_owned(), String::new(), String::new()))
        );
        assert_eq!(
            github("acme/agents@main/skills"),
            Some((
                "acme/agents".to_owned(),
                "main".to_owned(),
                "skills".to_owned()
            ))
        );
        assert_eq!(
            github("https://github.com/acme/agents/tree/dev/team/skills"),
            Some((
                "acme/agents".to_owned(),
                "dev".to_owned(),
                "team/skills".to_owned()
            ))
        );
        assert_eq!(github("https://skills.sh"), None);
        assert_eq!(github("registry.example.com/v0"), None);
    }

    #[test]
    fn the_built_ins_hold_until_a_person_changes_the_list_and_then_it_is_theirs() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(directory.path());
        let names = || {
            user()
                .unwrap()
                .into_iter()
                .map(|source| source.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(), ["skills.sh", "mcp-registry"]);

        add(Source {
            name: "team".to_owned(),
            kind: Kind::Github,
            url: String::new(),
            repo: "acme/agents".to_owned(),
            reference: String::new(),
            path: "skills".to_owned(),
            token: "work.github".to_owned(),
            origin: Origin::User,
        })
        .unwrap();
        assert_eq!(names(), ["skills.sh", "mcp-registry", "team"]);

        // A built-in can go, which is why the list is written out whole.
        remove("skills.sh").unwrap();
        assert_eq!(names(), ["mcp-registry", "team"]);
        let text = std::fs::read_to_string(userfile().unwrap()).unwrap();
        assert!(text.contains("token = \"work.github\""), "{text}");

        assert!(remove("nothing").is_err());
        assert!(add(user().unwrap()[1].clone()).is_err(), "names are unique");
    }

    #[test]
    fn a_projects_sources_are_searched_only_once_their_exact_contents_are_approved() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(&directory.path().join("data"));
        let project = directory.path().join("repo");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::create_dir_all(project.join(".synapse")).unwrap();
        let file = project.join(PROJECTFILE);
        std::fs::write(
            &file,
            "[[source]]\nname = \"team\"\nkind = \"github\"\nrepo = \"acme/agents\"\n",
        )
        .unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let team = |runtime: &tokio::runtime::Runtime| {
            runtime
                .block_on(resolved(Some(&project)))
                .unwrap()
                .0
                .into_iter()
                .find(|source| source.name == "team")
                .unwrap()
        };

        assert!(!team(&runtime).usable(), "nobody has approved it yet");
        runtime.block_on(allow(&project)).unwrap();
        assert!(team(&runtime).usable());

        // Editing it takes the approval away again.
        std::fs::write(
            &file,
            "[[source]]\nname = \"team\"\nkind = \"github\"\nrepo = \"evil/agents\"\n",
        )
        .unwrap();
        assert!(!team(&runtime).usable());
    }
}
