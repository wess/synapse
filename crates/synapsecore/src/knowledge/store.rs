use super::model::Header;
use super::{Catalog, Guide, Page, WriteRequest};
use super::{
    check::{Check, Checked},
    io::{self, Folder},
    paths,
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

const FILELIMIT: usize = 256 * 1024;
const PAGELIMIT: usize = 6000;
const GUIDELIMIT: usize = 32;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn decode(text: &str) -> Result<(Header, &str)> {
    let rest = text
        .strip_prefix("+++\n")
        .or_else(|| text.strip_prefix("+++\r\n"))
        .context("guide is missing TOML metadata")?;
    let (header, body) = rest
        .split_once("\n+++\n")
        .or_else(|| rest.split_once("\r\n+++\r\n"))
        .context("guide metadata is incomplete")?;
    let header: Header =
        toml::from_str(header).map_err(|_| anyhow::anyhow!("guide metadata is invalid TOML"))?;
    ensure!(
        !header.sources.is_empty() && header.sources.len() <= 64,
        "guide needs 1-64 sources"
    );
    ensure!(
        header.description.len() <= 240 && !header.description.contains(['\n', '\r']),
        "invalid guide description"
    );
    ensure!(
        header.sources.keys().all(|path| path.len() <= 512),
        "source paths exceed 512 bytes"
    );
    Ok((header, body))
}

fn inspected(root: &Folder, name: &str, text: &str, check: &mut Check) -> Result<(Guide, Header)> {
    let (header, _) = decode(text)?;
    let mut changed = Vec::new();
    let mut unchecked = 0;
    for (path, hash) in &header.sources {
        match check.source(root, path) {
            Checked::Hash(current) if current == *hash => {}
            Checked::Unchecked => unchecked += 1,
            _ => changed.push(crate::brain::redact(path).body),
        }
    }
    Ok((
        Guide {
            name: name.to_owned(),
            description: crate::brain::redact(&header.description).body,
            path: format!(".synapse/knowledge/{name}.md"),
            revision: digest(text.as_bytes()),
            unchecked,
            status: if unchecked > 0 {
                "unchecked"
            } else if changed.is_empty() {
                "current"
            } else {
                "needsreview"
            }
            .to_owned(),
            changedcount: changed.len(),
            changed: changed.into_iter().take(8).collect(),
        },
        header,
    ))
}

pub fn catalog(project: &Path) -> Result<Catalog> {
    let root = paths::root(project)?;
    let rootfolder = Folder::open(&root)?;
    let mut check = Check::new();
    let mut result = Catalog {
        project: root.display().to_string(),
        guides: Vec::new(),
        overview: None,
        sourcebytes: 0,
        warnings: Vec::new(),
        note: "No guides yet. When asked to learn this project, inspect the code and use writeknowledge to create overview and focused guides. Read guides on demand; verify changed sources before relying on them.".to_owned(),
    };
    let directory = match io::knowledge(&rootfolder, false) {
        Ok(directory) => directory,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(result);
        }
        Err(error) => return Err(error),
    };
    let (entries, truncated) = directory.entries(128)?;
    if truncated {
        result
            .warnings
            .push("Directory scan stopped at 128 entries; remove unrelated files.".to_owned());
    }
    let mut selected: Vec<_> = entries
        .into_iter()
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext == "md")
        })
        .collect();
    selected.sort_by_key(|name| (name != "overview.md", name.clone()));
    for (number, entry) in selected.into_iter().enumerate() {
        if number == GUIDELIMIT {
            result.warnings.push(
                "Only the first 32 guides are inspected; consolidate remaining guides.".to_owned(),
            );
            break;
        }
        let path = std::path::PathBuf::from(entry);
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let loaded = (|| -> Result<_> {
            paths::name(name)?;
            let text = String::from_utf8(
                directory.bytes(path.to_str().context("invalid guide filename")?, FILELIMIT)?,
            )?;
            let (guide, _) = inspected(&rootfolder, name, &text, &mut check)?;
            if name == "overview" {
                let (_, body) = decode(&text)?;
                result.overview = Some(page(&crate::brain::redact(body).body, 0, 1800)?.0);
            }
            Ok(guide)
        })();
        match loaded {
            Ok(guide) => result.guides.push(guide),
            Err(_) if result.warnings.len() < 8 => {
                let label = if paths::name(name).is_ok() {
                    name
                } else {
                    "invalid filename"
                };
                result
                    .warnings
                    .push(format!("{label}: unreadable or invalid guide"));
            }
            Err(_) => {}
        }
    }
    if !result.guides.is_empty() {
        result.note = "Read relevant guides with knowledge(name, project). Current means tracked source hashes match; it does not prove accuracy. Verify source code, especially guides marked needsreview or unchecked. Refresh only when requested, preserving existing edits and passing the revision.".to_owned();
    }
    result.sourcebytes = check.bytes;
    while serde_json::to_vec(&result)?.len() > 32768 {
        if result.guides.pop().is_none() {
            result.overview = None;
            break;
        }
        if !result
            .warnings
            .iter()
            .any(|warning| warning.contains("response size"))
        {
            result.warnings.push(
                "Guide index shortened to bound response size; read other guides by name."
                    .to_owned(),
            );
        }
    }
    Ok(result)
}

fn page(text: &str, offset: u32, limit: usize) -> Result<(String, Option<u32>)> {
    let start = offset as usize;
    ensure!(
        start <= text.len() && text.is_char_boundary(start),
        "invalid UTF-8 page offset"
    );
    let mut end = (start + limit).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((
        text[start..end].to_owned(),
        (end < text.len()).then_some(end as u32),
    ))
}

pub fn read(project: &Path, name: &str, offset: u32) -> Result<Page> {
    paths::name(name)?;
    let root = paths::root(project)?;
    let root = Folder::open(&root)?;
    let directory = io::knowledge(&root, false)?;
    let text = String::from_utf8(directory.bytes(&format!("{name}.md"), FILELIMIT)?)?;
    let (guide, header) = inspected(&root, name, &text, &mut Check::new())?;
    let (_, body) = decode(&text)?;
    let body = crate::brain::redact(body).body;
    let (content, next) = page(&body, offset, PAGELIMIT)?;
    Ok(Page {
        guide,
        content,
        sources: header
            .sources
            .into_keys()
            .map(|path| crate::brain::redact(&path).body)
            .collect(),
        offset,
        next,
        total: body.len(),
    })
}

pub fn write(project: &Path, request: &WriteRequest) -> Result<Guide> {
    paths::name(&request.name)?;
    ensure!(
        !request.description.trim().is_empty()
            && request.description.len() <= 240
            && !request.description.contains(['\n', '\r']),
        "description must be one line of 1-240 bytes"
    );
    ensure!(
        !request.content.trim().is_empty() && request.content.len() <= FILELIMIT / 2,
        "guide body must contain 1-131072 bytes"
    );
    ensure!(
        !request.sources.is_empty() && request.sources.len() <= 64,
        "supply 1-64 inspected source files"
    );
    ensure!(
        crate::brain::redact(&request.content).count == 0
            && crate::brain::redact(&request.description).count == 0,
        "remove credentials before saving project knowledge"
    );
    let root = paths::root(project)?;
    let root = Folder::open(&root)?;
    let mut check = Check::new();
    let mut sources = BTreeMap::new();
    for relative in &request.sources {
        paths::validsource(relative)?;
        ensure!(
            crate::brain::redact(relative).count == 0,
            "source path contains credentials"
        );
        let Checked::Hash(hash) = check.source(&root, relative) else {
            anyhow::bail!(
                "source unreadable, changed during inspection, or source scan exceeds 16 MiB; split the guide or reduce its sources"
            );
        };
        sources.insert(relative.clone(), hash);
    }
    let header = Header {
        description: request.description.clone(),
        sources,
    };
    let content = format!("+++\n{}+++\n{}", toml::to_string(&header)?, request.content);
    ensure!(content.len() <= FILELIMIT, "guide exceeds size limit");
    let directory = io::knowledge(&root, true)?;
    let lock = directory.file("write.lock", true)?;
    lock.try_lock_exclusive().map_err(|_| {
        anyhow::anyhow!("another writer is updating project knowledge; retry shortly")
    })?;
    let filename = format!("{}.md", request.name);
    match directory.bytes(&filename, FILELIMIT) {
        Ok(existing) => {
            ensure!(
                request.revision.as_deref() == Some(digest(&existing).as_str()),
                "guide changed or already exists; read it and preserve its edits before saving with its revision"
            );
        }
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            ensure!(
                request.revision.is_none(),
                "guide was removed; inspect the repository before recreating it"
            );
            let (entries, truncated) = directory.entries(128)?;
            ensure!(
                !truncated,
                "knowledge directory exceeds 128 entries; remove unrelated files"
            );
            let count = entries
                .into_iter()
                .filter(|name| {
                    std::path::Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext == "md")
                })
                .count();
            ensure!(
                count < GUIDELIMIT,
                "at most 32 guides per project; consolidate existing guides"
            );
        }
        Err(error) => return Err(error),
    }
    directory.replace(&filename, content.as_bytes())?;
    Ok(inspected(&root, &request.name, &content, &mut check)?.0)
}
