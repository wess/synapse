//! Finding a skill somebody else published, and bringing it into the library.
//!
//! This reaches the network only when somebody asks it to. It asks every
//! source that holds skills — see `crate::sources` — at once: a skills index
//! such as skills.sh, whose `search` weighs a match by how many people
//! installed it and whose `download` hands back a skill's files in one
//! response, and any GitHub repository a person or their team added. Those
//! index endpoints are the ones skills.sh's own CLI calls and are not
//! documented, so every response is checked against the standard before
//! anything is written.
//!
//! A skill brought in this way is *proposed*, exactly like one an agent wrote.
//! It is somebody else's instructions, its description would be loaded into
//! every session of every tool holding it, and a skill can carry scripts — so
//! it waits in the library, readable, until a person approves it.

use crate::skill::{ENTRY, Receipts, Shelf, library, model};
use crate::sources::{self, Kind, Source};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// What a proposal records as having written a skill from a source, before
/// the source's name. Distinct from any tool's name, which is what an agent's
/// proposal records.
pub const ORIGIN: &str = "source:";
const LIMIT: usize = 20;

/// Anything unbounded that Synapse writes gets a bound. A skill is a handful of
/// small files; a response that is not one should be refused, not mirrored.
const MAXFILES: usize = 64;
const MAXBYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Found {
    /// The source's name, which is half of what `preview` and `fetch` take.
    pub from: String,
    /// The source's own name for the skill: `owner/repo/skill` on an index,
    /// the folder on a repository.
    pub id: String,
    pub name: String,
    /// `owner/repo`, the GitHub repository it lives in.
    pub repository: String,
    /// What an index counted; a repository counts nothing.
    pub installs: Option<u64>,
    /// Known up front only from a repository, whose catalogue is read whole.
    pub description: String,
    /// A page to read it on.
    pub page: String,
}

impl Found {
    /// `source:id`, which is what `synapse skill get` takes.
    pub fn qualified(&self) -> String {
        format!("{}:{}", self.from, self.id)
    }
}

/// One published skill, read but not kept.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub from: String,
    pub id: String,
    pub name: String,
    pub description: String,
    /// The whole `SKILL.md`, frontmatter included, because that is what a
    /// person approving it has to read.
    pub entry: String,
    pub files: Vec<String>,
    pub page: String,
}

/// What a search found, and what each source that could not answer said.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Searched {
    pub found: Vec<Found>,
    pub problems: Vec<String>,
}

/// Ask every source that holds skills, together. A source that fails is
/// reported beside the rest rather than failing the search: an index being
/// down is no reason to hide a team's own repository.
pub async fn search(query: &str, folder: Option<&Path>) -> Result<Searched> {
    let query = query.trim();
    anyhow::ensure!(
        query.chars().any(char::is_alphanumeric),
        "search for a word, such as `rust` or `release notes`"
    );
    let (all, mut problems) = sources::resolved(folder).await?;
    let asked: Vec<Source> = all
        .into_iter()
        .filter(|source| source.skills() && source.usable())
        .collect();
    anyhow::ensure!(
        !asked.is_empty(),
        "no source holds skills; add one with `synapse source add`"
    );
    let mut tasks = tokio::task::JoinSet::new();
    for (index, source) in asked.iter().cloned().enumerate() {
        let query = query.to_owned();
        tasks.spawn(async move { (index, searchone(&source, &query).await) });
    }
    let mut answers: Vec<Option<Result<Vec<Found>>>> = asked.iter().map(|_| None).collect();
    while let Some(joined) = tasks.join_next().await {
        if let Ok((index, answer)) = joined {
            answers[index] = Some(answer);
        }
    }
    // Merged in the order the sources are listed, so a person's own sources
    // come where they put them rather than wherever the network was quickest.
    let mut found = Vec::new();
    for (source, answer) in asked.iter().zip(answers) {
        match answer {
            Some(Ok(mut items)) => found.append(&mut items),
            Some(Err(error)) => problems.push(format!("{}: {error:#}", source.name)),
            None => problems.push(format!(
                "{}: the search stopped before it answered",
                source.name
            )),
        }
    }
    Ok(Searched { found, problems })
}

async fn searchone(source: &Source, query: &str) -> Result<Vec<Found>> {
    match source.kind {
        Kind::SkillsIndex => {
            let response = client()?
                .get(format!("{}/api/search", source.url))
                .query(&[("q", query.to_owned()), ("limit", LIMIT.to_string())])
                .send()
                .await
                .with_context(|| format!("could not reach {}", source.url))?;
            let text = answer(response, &source.url, None).await?;
            Ok(parse(&text, source))
        }
        Kind::Github => Ok(sources::github::skills(source, query)
            .await?
            .into_iter()
            .map(|skill| Found {
                from: source.name.clone(),
                page: page(source, &skill.id),
                id: skill.id,
                name: skill.name,
                repository: source.repo.clone(),
                installs: None,
                description: skill.description,
            })
            .collect()),
        Kind::McpRegistry => Ok(Vec::new()),
    }
}

/// Where a person can read a skill before fetching it.
fn page(source: &Source, id: &str) -> String {
    match source.kind {
        Kind::Github => format!(
            "https://github.com/{}/tree/{}/{id}",
            source.repo,
            if source.reference.is_empty() {
                "HEAD"
            } else {
                &source.reference
            }
        ),
        _ => format!("{}/{id}", source.url),
    }
}

/// `source:id`, or a bare id meaning the first skills index — which keeps
/// `synapse skill get owner/repo/skill` working as it always has.
pub async fn locate(qualified: &str, folder: Option<&Path>) -> Result<(Source, String)> {
    let (all, _) = sources::resolved(folder).await?;
    let usable = |source: &&Source| source.skills() && source.usable();
    if let Some((name, id)) = qualified.split_once(':')
        && !name.contains('/')
        && let Some(source) = all.iter().filter(usable).find(|source| source.name == name)
    {
        return Ok((source.clone(), id.to_owned()));
    }
    let source = all
        .iter()
        .filter(usable)
        .find(|source| source.kind == Kind::SkillsIndex)
        .with_context(|| format!("`{qualified}` names no source; write it as source:id"))?;
    Ok((source.clone(), qualified.to_owned()))
}

/// Read a published skill without keeping anything.
pub async fn preview(source: &Source, id: &str) -> Result<Preview> {
    let files = download(source, id).await?;
    let entry = files
        .iter()
        .find(|file| file.path == ENTRY)
        .with_context(|| format!("{id} has no {ENTRY}, so it is not a skill"))?;
    let name = declared(&entry.contents)?;
    let (_, description) = model::parse(&name, &entry.contents)?;
    Ok(Preview {
        from: source.name.clone(),
        id: id.to_owned(),
        name,
        description,
        entry: entry.contents.clone(),
        files: files.iter().map(|file| file.path.clone()).collect(),
        page: page(source, id),
    })
}

/// Fetch a published skill and add it to the global shelf, waiting for
/// review. Returns its name and where it landed.
pub async fn fetch(receipts: &Receipts, source: &Source, id: &str) -> Result<(String, PathBuf)> {
    let files = download(source, id).await?;
    let entry = files
        .iter()
        .find(|file| file.path == ENTRY)
        .with_context(|| format!("{id} has no {ENTRY}, so it is not a skill"))?;
    // The folder is named for what the skill says it is called, because the
    // standard requires the two to match and the source's name for it need not.
    let name = declared(&entry.contents)?;
    anyhow::ensure!(
        !library::path(&Shelf::Global, &name)?.join(ENTRY).exists(),
        "the library already has a skill named `{name}`"
    );

    let staging = std::env::temp_dir().join(format!(
        "synapse-skill-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    ));
    let folder = staging.join(&name);
    let outcome = async {
        for file in &files {
            let target = folder.join(&file.path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("could not create {}", parent.display()))?;
            }
            std::fs::write(&target, &file.contents)
                .with_context(|| format!("could not write {}", target.display()))?;
        }
        admit(receipts, &folder, &source.name, id).await
    }
    .await;
    let _ = std::fs::remove_dir_all(&staging);
    outcome.map(|path| (name, path))
}

/// Put a downloaded skill folder into the library as a proposal.
///
/// Split from `fetch` so the part that decides what lands where can be tested
/// without a network.
async fn admit(receipts: &Receipts, folder: &Path, from: &str, id: &str) -> Result<PathBuf> {
    let skill = model::read(folder, Shelf::Global)?;
    let destination = library::path(&Shelf::Global, &skill.name)?;
    anyhow::ensure!(
        !destination.join(ENTRY).exists(),
        "the library already has a skill named `{}`",
        skill.name
    );
    library::ensureshelf(&Shelf::Global)?;
    library::copy(folder, &destination, &skill.files)?;
    receipts
        .propose(
            Shelf::Global.key(),
            &skill.name,
            "",
            &format!("{ORIGIN}{from}"),
            id,
        )
        .await?;
    Ok(destination)
}

/// `owner/repo/skill`, as an index names a skill. Its page address is accepted
/// too, since that is what a person copies out of a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Id {
    owner: String,
    repo: String,
    skill: String,
}

impl Id {
    fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim().trim_end_matches('/');
        let rest = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .unwrap_or(trimmed);
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        let rest = rest.strip_prefix("skills.sh/").unwrap_or(rest);
        let parts: Vec<&str> = rest.split('/').collect();
        anyhow::ensure!(
            parts.len() == 3 && owner(parts[0]) && parts.iter().all(|part| segment(part)),
            "`{value}` is not a skill; expected owner/repo/skill, as `synapse skill search` prints it"
        );
        Ok(Self {
            owner: parts[0].to_owned(),
            repo: parts[1].to_owned(),
            skill: parts[2].to_owned(),
        })
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}/{}", self.owner, self.repo, self.skill)
    }
}

/// GitHub's rule for an account name: letters, digits, and inner hyphens.
/// Stricter than `segment`, which is what stops `github.com/a/b` reading as a
/// skill owned by somebody called `github.com`.
fn owner(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 39
        && !part.starts_with('-')
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn segment(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
}

#[derive(Deserialize)]
struct File {
    path: String,
    contents: String,
}

/// Every file of one skill, checked: inside the skill, within the bounds, and
/// with a `SKILL.md` at its top.
async fn download(source: &Source, id: &str) -> Result<Vec<File>> {
    match source.kind {
        Kind::SkillsIndex => {
            let id = Id::parse(id)?;
            let mut url = reqwest::Url::parse(&source.url)
                .with_context(|| format!("{} is not an address", source.url))?;
            url.path_segments_mut()
                .map_err(|_| anyhow::anyhow!("{} is not an address", source.url))?
                .extend(["api", "download", &id.owner, &id.repo, &id.skill]);
            let response = client()?
                .get(url)
                .send()
                .await
                .with_context(|| format!("could not reach {}", source.url))?;
            let text = answer(response, &source.url, Some(&id.to_string())).await?;

            #[derive(Deserialize)]
            struct Download {
                files: Vec<File>,
            }
            let download: Download = serde_json::from_str(&text).with_context(|| {
                format!("{} sent something this version cannot read", source.name)
            })?;
            check(download.files)
        }
        Kind::Github => check(
            sources::github::skillfiles(source, id)
                .await?
                .into_iter()
                .map(|(path, contents)| File { path, contents })
                .collect(),
        ),
        Kind::McpRegistry => anyhow::bail!("{} holds MCP servers, not skills", source.name),
    }
}

fn check(files: Vec<File>) -> Result<Vec<File>> {
    anyhow::ensure!(
        files.len() <= MAXFILES,
        "the skill has more than {MAXFILES} files; that is not a skill"
    );
    anyhow::ensure!(
        files.iter().map(|file| file.contents.len()).sum::<usize>() <= MAXBYTES,
        "the skill is larger than {} MB; that is not a skill",
        MAXBYTES / 1024 / 1024
    );
    for file in &files {
        anyhow::ensure!(
            plain(&file.path),
            "the source listed a path that leaves the skill: {}",
            file.path
        );
    }
    Ok(files)
}

/// A relative path that stays inside the folder it is relative to.
fn plain(relative: &str) -> bool {
    !relative.is_empty()
        && Path::new(relative)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

/// The name a `SKILL.md` gives itself, checked against the standard.
fn declared(content: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Named {
        name: Option<String>,
    }
    let (frontmatter, _) = model::split(content)?;
    let named: Named =
        serde_saphyr::from_str(frontmatter).context("could not read the skill's frontmatter")?;
    let name = named
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .context("the skill has no `name` in its frontmatter")?;
    model::validname(&name)?;
    Ok(name)
}

async fn answer(response: reqwest::Response, url: &str, id: Option<&str>) -> Result<String> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    match (status.as_u16(), id) {
        (200, _) => Ok(text),
        (404, Some(id)) => anyhow::bail!("{url} has no skill {id}"),
        (429, _) => anyhow::bail!("{url} is limiting requests; try again in a minute"),
        (code, _) => anyhow::bail!("{url} answered {code}"),
    }
}

/// An index's answer, in the index's order: it already weighs installs, and
/// sorting on installs alone puts a popular near-miss above the skill that was
/// asked for.
fn parse(text: &str, source: &Source) -> Vec<Found> {
    #[derive(Deserialize)]
    struct Results {
        skills: Vec<Wire>,
    }
    #[derive(Deserialize)]
    struct Wire {
        id: String,
        name: String,
        #[serde(default)]
        source: String,
        #[serde(default)]
        installs: u64,
    }
    let Ok(results) = serde_json::from_str::<Results>(text) else {
        return Vec::new();
    };
    results
        .skills
        .into_iter()
        // An id that would not survive `Id::parse` is one nothing here could
        // fetch, so it is not offered.
        .filter(|wire| Id::parse(&wire.id).is_ok())
        .map(|wire| Found {
            from: source.name.clone(),
            page: page(source, &wire.id),
            id: wire.id,
            name: wire.name,
            repository: wire.source,
            installs: Some(wire.installs),
            description: String::new(),
        })
        .collect()
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("synapse/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .context("could not start an HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Source {
        Source {
            name: "skills.sh".to_owned(),
            kind: Kind::SkillsIndex,
            url: "https://skills.sh".to_owned(),
            repo: String::new(),
            reference: String::new(),
            path: String::new(),
            token: String::new(),
            origin: sources::Origin::Builtin,
        }
    }

    #[test]
    fn a_skill_is_named_by_its_index_id_or_its_page() {
        let expected = Id {
            owner: "phuryn".to_owned(),
            repo: "pm-skills".to_owned(),
            skill: "release-notes".to_owned(),
        };
        assert_eq!(
            Id::parse("phuryn/pm-skills/release-notes").unwrap(),
            expected
        );
        assert_eq!(
            Id::parse("https://skills.sh/phuryn/pm-skills/release-notes/").unwrap(),
            expected
        );
        assert_eq!(expected.to_string(), "phuryn/pm-skills/release-notes");
    }

    #[test]
    fn anything_else_is_refused_before_a_request_is_made() {
        for value in [
            "release-notes",
            "phuryn/pm-skills",
            "a/b/c/d",
            "a/../c",
            "a/b/c?x=1",
            "https://github.com/a/b",
        ] {
            assert!(Id::parse(value).is_err(), "{value} should be refused");
        }
    }

    #[test]
    fn a_downloaded_path_must_stay_inside_the_skill() {
        assert!(plain("SKILL.md"));
        assert!(plain("scripts/run.sh"));
        assert!(!plain(""));
        assert!(!plain("../outside"));
        assert!(!plain("/etc/passwd"));
        assert!(!plain("scripts/../../outside"));

        let escaping = vec![File {
            path: "../../.zshrc".to_owned(),
            contents: String::new(),
        }];
        assert!(check(escaping).is_err());
    }

    #[test]
    fn a_download_past_the_bounds_is_refused() {
        let many = (0..=MAXFILES)
            .map(|index| File {
                path: format!("{index}.md"),
                contents: String::new(),
            })
            .collect();
        assert!(check(many).is_err());
        let large = vec![File {
            path: ENTRY.to_owned(),
            contents: "x".repeat(MAXBYTES + 1),
        }];
        assert!(check(large).is_err());
    }

    #[test]
    fn search_results_are_read_from_the_index_and_say_where_they_came_from() {
        let text = r#"{"query":"rust","searchType":"fuzzy","skills":[{"id":"apollographql/skills/rust-best-practices","source":"apollographql/skills","skillId":"rust-best-practices","name":"rust-best-practices","installs":20133},{"id":"bad id","name":"x","installs":99999}],"count":2}"#;
        let found = parse(text, &index());
        assert_eq!(found.len(), 1, "an id nothing could fetch is not offered");
        assert_eq!(found[0].repository, "apollographql/skills");
        assert_eq!(found[0].installs, Some(20133));
        assert_eq!(found[0].from, "skills.sh");
        assert_eq!(
            found[0].qualified(),
            "skills.sh:apollographql/skills/rust-best-practices"
        );
        assert_eq!(
            found[0].page,
            "https://skills.sh/apollographql/skills/rust-best-practices"
        );
    }

    #[test]
    fn the_name_comes_from_the_skill_itself() {
        let content = "---\nname: release-notes\ndescription: \"Write: notes.\"\n---\n\nRead.\n";
        assert_eq!(declared(content).unwrap(), "release-notes");
        assert!(declared("---\ndescription: x\n---\n\nRead.\n").is_err());
        assert!(declared("---\nname: ../up\ndescription: x\n---\n\nRead.\n").is_err());
    }

    #[test]
    fn a_bare_id_means_the_first_index_and_a_prefix_names_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(directory.path());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (source, id) = runtime
            .block_on(locate("phuryn/pm-skills/release-notes", None))
            .unwrap();
        assert_eq!(source.name, "skills.sh");
        assert_eq!(id, "phuryn/pm-skills/release-notes");
        let (source, id) = runtime
            .block_on(locate("skills.sh:phuryn/pm-skills/release-notes", None))
            .unwrap();
        assert_eq!(source.name, "skills.sh");
        assert_eq!(id, "phuryn/pm-skills/release-notes");
    }

    #[test]
    fn a_fetched_skill_lands_in_the_library_waiting_for_review_and_in_no_tool() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(directory.path());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let receipts = runtime
            .block_on(Receipts::open(directory.path().join("brain.db")))
            .unwrap();

        let downloaded = directory.path().join("download").join("release-notes");
        std::fs::create_dir_all(downloaded.join("scripts")).unwrap();
        std::fs::write(
            downloaded.join(ENTRY),
            "---\nname: release-notes\ndescription: Write release notes.\n---\n\nRead the log.\n",
        )
        .unwrap();
        std::fs::write(downloaded.join("scripts/log.sh"), "git log\n").unwrap();

        let id = "phuryn/pm-skills/release-notes";
        let landed = runtime
            .block_on(admit(&receipts, &downloaded, "skills.sh", id))
            .unwrap();
        assert!(landed.join("scripts/log.sh").is_file());

        let waiting = runtime
            .block_on(receipts.proposed("", "release-notes"))
            .unwrap()
            .expect("a fetched skill must wait for review");
        assert_eq!(waiting.tool, "source:skills.sh");
        assert_eq!(waiting.note, id);
        assert!(
            runtime
                .block_on(receipts.holders("", "release-notes"))
                .unwrap()
                .is_empty(),
            "nothing may be installed before it is approved"
        );

        let again = runtime.block_on(admit(&receipts, &downloaded, "skills.sh", id));
        assert!(
            again.is_err(),
            "a skill already in the library is never replaced"
        );
    }
}
