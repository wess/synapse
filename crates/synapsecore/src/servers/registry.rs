//! Finding a server in an MCP registry, or in a repository's `server.json`.
//!
//! Every source that holds MCP servers is asked at once — see
//! `crate::sources`. The built-in one is registry.modelcontextprotocol.io, the
//! documented, keyless index the MCP project runs. It is slow — fifteen to
//! forty-five seconds a search when measured — so the window asks it off the
//! main thread and says it may take a while, and the client waits a full
//! minute before giving up. A GitHub source contributes the `server.json`
//! files it holds, which are the same entries a registry serves.
//!
//! An entry describes how to run a server rather than handing over a command,
//! so turning one into a library server is a translation: an npm package is
//! `npx`, a PyPI one `uvx`, an OCI image `docker run`. A required setting the
//! entry gives no value for is left empty, a required argument becomes a
//! `<placeholder>`, and a secret becomes a variable with no reference — all of
//! which `Server::missing` reports and `run` refuses to start without.

use crate::servers::model::{Server, slug};
use crate::sources::{self, Kind, Source};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const LIMIT: usize = 30;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Listing {
    /// The source it came from.
    pub from: String,
    /// The entry's name, such as `io.github.owner/server`.
    pub id: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub repository: String,
    /// The library server it would become, when there is a way to run it here.
    pub draft: Option<Server>,
    /// Why it cannot be added, or what is left to fill in once it is.
    pub note: String,
}

impl Listing {
    /// `source:id`, which is what `synapse mcp get` takes.
    pub fn qualified(&self) -> String {
        format!("{}:{}", self.from, self.id)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Searched {
    pub found: Vec<Listing>,
    pub problems: Vec<String>,
}

/// Ask every source that holds MCP servers, together. What can be run here
/// comes first across all of them; within that, sources keep the order they
/// are listed in.
pub async fn search(query: &str, folder: Option<&Path>) -> Result<Searched> {
    let query = query.trim();
    anyhow::ensure!(
        query.chars().any(char::is_alphanumeric),
        "search for a word, such as `github` or `postgres`"
    );
    let (all, mut problems) = sources::resolved(folder).await?;
    let asked: Vec<Source> = all
        .into_iter()
        .filter(|source| source.servers() && source.usable())
        .collect();
    anyhow::ensure!(
        !asked.is_empty(),
        "no source holds MCP servers; add one with `synapse source add`"
    );
    let mut tasks = tokio::task::JoinSet::new();
    for (index, source) in asked.iter().cloned().enumerate() {
        let query = query.to_owned();
        tasks.spawn(async move { (index, searchone(&source, &query).await) });
    }
    let mut answers: Vec<Option<Result<Vec<Listing>>>> = asked.iter().map(|_| None).collect();
    while let Some(joined) = tasks.join_next().await {
        if let Ok((index, answer)) = joined {
            answers[index] = Some(answer);
        }
    }
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
    // A registry sorts by name and has nothing better to sort by, so the one
    // useful order is what can be run here first. Stable, so each source's
    // own order holds within each half.
    found.sort_by_key(|listing| listing.draft.is_none());
    Ok(Searched { found, problems })
}

async fn searchone(source: &Source, query: &str) -> Result<Vec<Listing>> {
    match source.kind {
        Kind::McpRegistry => parse(&fetch(source, query).await?, &source.name),
        Kind::Github => {
            let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
            Ok(repository(source)
                .await?
                .into_iter()
                .filter(|listing| {
                    let haystack =
                        format!("{} {} {}", listing.id, listing.title, listing.description)
                            .to_lowercase();
                    words.iter().all(|word| haystack.contains(word))
                })
                .collect())
        }
        Kind::SkillsIndex => Ok(Vec::new()),
    }
}

/// Every `server.json` in a repository, as listings. A file that is not an
/// entry is skipped: a repository can hold other things called that.
async fn repository(source: &Source) -> Result<Vec<Listing>> {
    Ok(sources::github::servers(source)
        .await?
        .into_iter()
        .filter_map(|server| serde_json::from_str::<Wire>(&server.text).ok())
        .map(|wire| listing(wire, &source.name))
        .collect())
}

/// One entry, as `source:id`, or a bare id meaning the first registry.
pub async fn find(qualified: &str, folder: Option<&Path>) -> Result<Listing> {
    let qualified = qualified.trim();
    let (all, _) = sources::resolved(folder).await?;
    let usable = |source: &&Source| source.servers() && source.usable();
    let (source, id) = match qualified.split_once(':') {
        Some((name, id)) if !name.contains('/') => {
            match all.iter().filter(usable).find(|source| source.name == name) {
                Some(source) => (source.clone(), id.to_owned()),
                None => anyhow::bail!("there is no source named `{name}` that holds MCP servers"),
            }
        }
        _ => (
            all.iter()
                .filter(usable)
                .find(|source| source.kind == Kind::McpRegistry)
                .cloned()
                .with_context(|| format!("`{qualified}` names no source; write it as source:id"))?,
            qualified.to_owned(),
        ),
    };
    let listings = match source.kind {
        Kind::McpRegistry => parse(&fetch(&source, &id).await?, &source.name)?,
        Kind::Github => repository(&source).await?,
        Kind::SkillsIndex => Vec::new(),
    };
    listings
        .into_iter()
        .find(|listing| listing.id == id)
        .with_context(|| format!("{} has no server named `{id}`", source.name))
}

async fn fetch(source: &Source, query: &str) -> Result<String> {
    let response = reqwest::Client::builder()
        .user_agent(concat!("synapse/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("could not start an HTTP client")?
        .get(format!("{}/v0/servers", source.url))
        .query(&[
            ("search", query.to_owned()),
            ("limit", LIMIT.to_string()),
            ("version", "latest".to_owned()),
        ])
        .send()
        .await
        .with_context(|| format!("could not reach {}; a registry can be slow, so try once more before assuming it is down", source.url))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    anyhow::ensure!(status.is_success(), "{} answered {status}", source.url);
    Ok(text)
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    servers: Vec<Wrapper>,
}

#[derive(Deserialize)]
struct Wrapper {
    server: Wire,
}

#[derive(Deserialize)]
struct Wire {
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    repository: Option<Repository>,
    #[serde(default)]
    packages: Vec<Package>,
    #[serde(default)]
    remotes: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Repository {
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Package {
    #[serde(default)]
    registry_type: String,
    #[serde(default)]
    identifier: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    transport: Transport,
    #[serde(default)]
    environment_variables: Vec<Variable>,
    #[serde(default)]
    package_arguments: Vec<Argument>,
}

#[derive(Default, Deserialize)]
struct Transport {
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Variable {
    name: String,
    #[serde(default)]
    is_secret: bool,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    value: String,
    #[serde(default)]
    default: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Argument {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    default: String,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    value_hint: String,
}

fn parse(text: &str, from: &str) -> Result<Vec<Listing>> {
    let page: Page = serde_json::from_str(text)
        .context("the registry sent something this version cannot read")?;
    let mut seen = std::collections::HashSet::new();
    Ok(page
        .servers
        .into_iter()
        .map(|wrapper| listing(wrapper.server, from))
        // `version=latest` should leave one of each, and a registry that sends
        // two anyway should not show the same server twice.
        .filter(|listing| seen.insert(listing.id.clone()))
        .collect())
}

fn listing(wire: Wire, from: &str) -> Listing {
    let mut listing = Listing {
        from: from.to_owned(),
        id: wire.name.clone(),
        title: wire.title,
        description: wire
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        version: wire.version,
        repository: wire
            .repository
            .map(|repository| repository.url)
            .unwrap_or_default(),
        draft: None,
        note: String::new(),
    };
    let runnable = wire
        .packages
        .iter()
        .filter(|package| package.transport.kind == "stdio" || package.transport.kind.is_empty())
        .find_map(|package| draft(&wire.name, &listing.description, package, from));
    match runnable {
        Some(server) => {
            let missing = server.missing();
            if !missing.is_empty() {
                listing.note = format!("Needs {} before it can start", missing.join(", "));
            }
            listing.draft = Some(server);
        }
        None if !wire.remotes.is_empty() => {
            listing.note = "A remote server. Synapse keeps servers it starts itself, so add this one in each tool directly.".to_owned();
        }
        None => {
            listing.note =
                "No package Synapse knows how to run (npm, PyPI, or a container image).".to_owned();
        }
    }
    listing
}

/// The library server a package would be, or `None` for a kind of package
/// there is no command for here.
fn draft(id: &str, description: &str, package: &Package, from: &str) -> Option<Server> {
    // A `server.json` straight out of a repository is often the template a
    // release fills in, with `${VERSION}` where the version goes. Nothing here
    // can fill it, so it is not pinned and the template part is dropped.
    let pinned = !package.version.is_empty()
        && package.version != "latest"
        && !package.version.contains("${");
    let identifier = untemplated(&package.identifier);
    let mut server = Server {
        name: name(id),
        description: description.to_owned(),
        source: format!("{from}: {id}"),
        ..Server::default()
    };
    match package.registry_type.as_str() {
        "npm" => {
            server.command = "npx".to_owned();
            server.args = vec![
                "-y".to_owned(),
                match pinned {
                    true => format!("{identifier}@{}", package.version),
                    false => identifier.clone(),
                },
            ];
        }
        "pypi" => {
            server.command = "uvx".to_owned();
            server.args = vec![match pinned {
                true => format!("{identifier}=={}", package.version),
                false => identifier.clone(),
            }];
        }
        "oci" => {
            server.command = "docker".to_owned();
            server.args = vec!["run".to_owned(), "-i".to_owned(), "--rm".to_owned()];
            // A container sees none of the environment it is started with
            // unless each variable is named.
            for variable in package
                .environment_variables
                .iter()
                .filter(|variable| wanted(variable))
            {
                server.args.push("-e".to_owned());
                server.args.push(variable.name.clone());
            }
            server.args.push(identifier.clone());
        }
        _ => return None,
    }
    if identifier.is_empty() {
        return None;
    }
    for argument in &package.package_arguments {
        let given = first(&argument.value, &argument.default);
        let hole = || {
            format!(
                "<{}>",
                first(&argument.value_hint, &argument.name).trim_start_matches('-')
            )
        };
        match argument.kind.as_str() {
            "named" if !given.is_empty() => {
                server.args.push(argument.name.clone());
                server.args.push(given.to_owned());
            }
            "named" if argument.is_required => {
                server.args.push(argument.name.clone());
                server.args.push(hole());
            }
            "positional" if !given.is_empty() => server.args.push(given.to_owned()),
            "positional" if argument.is_required => server.args.push(hole()),
            _ => {}
        }
    }
    for variable in package
        .environment_variables
        .iter()
        .filter(|variable| wanted(variable))
    {
        if variable.is_secret {
            server.secrets.insert(variable.name.clone(), String::new());
        } else {
            server.env.insert(
                variable.name.clone(),
                first(&variable.value, &variable.default).to_owned(),
            );
        }
    }
    Some(server)
}

/// An identifier with any unfilled `${…}` tag or version taken off, so
/// `ghcr.io/x/image:${VERSION}` runs the image's latest rather than a tag
/// that does not exist.
fn untemplated(identifier: &str) -> String {
    match identifier.find("${") {
        Some(at) => identifier[..at]
            .trim_end_matches([':', '@', '='])
            .to_owned(),
        None => identifier.to_owned(),
    }
}

/// Only what the server needs. An optional variable left out is the server's
/// own default; written in empty, it would read as one still to be filled.
fn wanted(variable: &Variable) -> bool {
    variable.is_required
}

fn first<'a>(one: &'a str, two: &'a str) -> &'a str {
    if one.is_empty() { two } else { one }
}

/// `io.github.owner/filesystem-mcp` becomes `filesystem-mcp`: the part after
/// the namespace is what the author called it.
fn name(id: &str) -> String {
    let last = id.rsplit('/').next().unwrap_or(id);
    match slug(last) {
        name if name.is_empty() || name == "synapse" => slug(id),
        name => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"{"servers":[
      {"server":{"name":"io.github.j0hanz/filesystem-mcp","description":"Read and\n write files.","version":"2.9.1",
        "repository":{"url":"https://github.com/j0hanz/filesystem-mcp"},
        "packages":[{"registryType":"npm","identifier":"@j0hanz/filesystem-mcp","version":"2.9.1","transport":{"type":"stdio"},
          "environmentVariables":[{"name":"FS_ALLOWED_DIRS"},{"name":"API_KEY","isSecret":true,"isRequired":true}],
          "packageArguments":[{"type":"positional","isRequired":true,"valueHint":"directory"},{"type":"named","name":"--read-only"}]}]}},
      {"server":{"name":"com.example/remote","description":"Hosted.","version":"1.0.0","remotes":[{"type":"streamable-http","url":"https://x"}]}},
      {"server":{"name":"io.github.x/pyserver","version":"0.1.4","packages":[{"registryType":"pypi","identifier":"vs-server","version":"0.1.4","transport":{"type":"stdio"},
          "environmentVariables":[{"name":"MODE","isRequired":true,"default":"fast"}]}]}},
      {"server":{"name":"io.github.x/image","version":"1.0","packages":[{"registryType":"oci","identifier":"ghcr.io/x/image:1.0","transport":{"type":"stdio"},
          "environmentVariables":[{"name":"TOKEN","isSecret":true,"isRequired":true}]}]}},
      {"server":{"name":"io.github.x/dotnet","version":"1.0","packages":[{"registryType":"nuget","identifier":"X","transport":{"type":"stdio"}}]}}
    ]}"#;

    #[test]
    fn each_kind_of_package_becomes_a_command_this_machine_can_run() {
        let mut listings = parse(PAGE, "mcp-registry").unwrap();
        listings.sort_by_key(|listing| listing.draft.is_none());
        assert_eq!(listings.len(), 5);
        // What can be run comes first; the registry's order holds within.
        let ids: Vec<&str> = listings.iter().map(|listing| listing.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "io.github.j0hanz/filesystem-mcp",
                "io.github.x/pyserver",
                "io.github.x/image",
                "com.example/remote",
                "io.github.x/dotnet"
            ]
        );

        let files = listings[0].draft.as_ref().unwrap();
        assert_eq!(files.name, "filesystem-mcp");
        assert_eq!(files.description, "Read and write files.");
        assert_eq!(
            files.args,
            ["-y", "@j0hanz/filesystem-mcp@2.9.1", "<directory>"]
        );
        // The optional variable is the server's own business; the required
        // secret is a hole to fill.
        assert!(files.env.is_empty());
        assert_eq!(files.secrets.get("API_KEY").map(String::as_str), Some(""));
        assert_eq!(
            listings[0].note,
            "Needs API_KEY, <directory> before it can start"
        );

        assert!(listings[3].draft.is_none());
        assert!(listings[3].note.contains("remote"));

        let python = listings[1].draft.as_ref().unwrap();
        assert_eq!(python.commandline(), "uvx vs-server==0.1.4");
        assert_eq!(python.env.get("MODE").map(String::as_str), Some("fast"));
        assert!(listings[1].note.is_empty());

        let image = listings[2].draft.as_ref().unwrap();
        assert_eq!(
            image.args,
            ["run", "-i", "--rm", "-e", "TOKEN", "ghcr.io/x/image:1.0"]
        );

        assert!(listings[4].draft.is_none());
    }

    #[test]
    fn a_templated_version_is_left_unpinned_rather_than_passed_through() {
        assert_eq!(
            untemplated("ghcr.io/github/github-mcp-server:${VERSION}"),
            "ghcr.io/github/github-mcp-server"
        );
        assert_eq!(untemplated("@scope/pkg@${VERSION}"), "@scope/pkg");
        assert_eq!(untemplated("plain-package"), "plain-package");
        let page = r#"{"servers":[{"server":{"name":"io.github.x/t","version":"${VERSION}","packages":[{"registryType":"npm","identifier":"t-server","version":"${VERSION}","transport":{"type":"stdio"}}]}}]}"#;
        let listing = &parse(page, "repo").unwrap()[0];
        assert_eq!(
            listing.draft.as_ref().unwrap().commandline(),
            "npx -y t-server"
        );
    }

    #[test]
    fn a_library_name_is_the_part_the_author_chose() {
        assert_eq!(name("io.github.j0hanz/filesystem-mcp"), "filesystem-mcp");
        assert_eq!(
            name("io.github.someone/synapse"),
            "io-github-someone-synapse"
        );
    }
}
