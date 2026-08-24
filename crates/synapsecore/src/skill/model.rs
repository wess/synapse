//! The Agent Skills format, as the open standard defines it.
//!
//! A skill is a directory holding a `SKILL.md` with YAML frontmatter and
//! Markdown instructions, plus whatever scripts, references, and assets it
//! needs. Claude Code, Codex, and pi all read the same format, which is the
//! whole reason one library can serve them together.

use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The file every skill directory must contain.
pub const ENTRY: &str = "SKILL.md";

/// Frontmatter length ceilings from the specification.
const NAMELIMIT: usize = 64;
const DESCRIPTIONLIMIT: usize = 1024;

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Every file in the skill directory, relative to its root, sorted. More
    /// than `SKILL.md` means the skill carries scripts, references, or assets.
    pub files: Vec<String>,
    /// Content digest of the whole directory, which is how an installed copy is
    /// recognised as current, changed, or somebody else's.
    pub digest: String,
    /// Which shelf it was read from. Carried on the skill rather than passed
    /// beside it because everything downstream — where to install it, which
    /// receipt describes it, which folder it came from — needs it, and a pair
    /// that can be split is a pair that gets split.
    pub shelf: Shelf,
}

/// Where a skill lives: the shared library, or one project's.
///
/// A procedure worked out in one repository is usually about that repository,
/// and a library where every skill is global is a library that costs every
/// session on the machine to hold one project's checklist. This is the split
/// memory already makes, for the same reason.
#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(tag = "scope", rename_all = "lowercase")]
pub enum Shelf {
    #[default]
    Global,
    Project {
        /// The shelf's directory name under the library.
        slug: String,
        /// The project root it belongs to, as it was when the shelf was made.
        root: String,
    },
}

impl Shelf {
    /// The shelf for a project root. Canonicalised where possible, so the same
    /// repository reached through a symlink is one shelf and not two.
    pub fn project(root: &Path) -> Self {
        let resolved = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self::Project {
            slug: slug(&resolved),
            root: resolved.display().to_string(),
        }
    }

    /// What the database stores. Empty is global, which keeps every receipt
    /// written before shelves existed pointing at the shelf it came from.
    pub fn key(&self) -> &str {
        match self {
            Self::Global => "",
            Self::Project { slug, .. } => slug,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project { .. } => "project",
        }
    }

    pub fn root(&self) -> Option<&str> {
        match self {
            Self::Global => None,
            Self::Project { root, .. } => Some(root),
        }
    }
}

/// A project shelf's directory name: something a person can recognise, plus
/// enough of a digest that two checkouts called `api` are two shelves.
pub fn slug(root: &Path) -> String {
    use sha2::{Digest, Sha256};

    let digest = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
    let readable: String = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .chars()
        .map(|item| match item.is_ascii_alphanumeric() {
            true => item.to_ascii_lowercase(),
            false => '-',
        })
        .collect();
    let readable = readable.trim_matches('-');
    let readable = match readable.is_empty() {
        true => "project",
        false => &readable[..readable.len().min(32)],
    };
    format!("{readable}-{}", &digest[..8])
}

#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// Split a `SKILL.md` into its frontmatter and body. The standard requires the
/// file to open with a `---` fence.
pub fn split(content: &str) -> Result<(&str, &str)> {
    let trimmed = content.strip_prefix('\u{feff}').unwrap_or(content);
    let rest = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
        .context("a skill must begin with a `---` frontmatter fence")?;
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.find("\n---\r\n"))
        .context("the frontmatter is never closed with `---`")?;
    let body = rest[end..]
        .trim_start_matches('\n')
        .trim_start_matches("---")
        .trim_start_matches(['\r', '\n']);
    Ok((&rest[..end], body))
}

/// Read and validate the `SKILL.md` in `directory`, as a skill on `shelf`.
pub fn read(directory: &Path, shelf: Shelf) -> Result<Skill> {
    let name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .context("a skill directory needs a name")?
        .to_owned();
    let entry = directory.join(ENTRY);
    let content = std::fs::read_to_string(&entry)
        .with_context(|| format!("could not read {}", entry.display()))?;
    parse(&name, &content)?;
    let files = contents(directory)?;
    Ok(Skill {
        digest: digest(directory, &files)?,
        name: name.clone(),
        description: parse(&name, &content)?.1,
        files,
        shelf,
    })
}

/// Validate `content` as the skill called `name`, returning its name and
/// description. The directory name is authoritative: the standard requires the
/// `name` field to match it, so a mismatch is refused rather than quietly
/// preferred one way or the other.
pub fn parse(name: &str, content: &str) -> Result<(String, String)> {
    let (frontmatter, body) = split(content)?;
    let parsed: Frontmatter = serde_saphyr::from_str(frontmatter).with_context(|| {
        // An unquoted value containing `: ` is the usual cause, and the YAML
        // error alone rarely says so.
        match frontmatter.lines().any(unquotedcolon) {
            true => format!(
                "could not read the frontmatter of `{name}`; a value containing `: ` has to be quoted"
            ),
            false => format!("could not read the frontmatter of `{name}`"),
        }
    })?;

    let declared = parsed
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("`{name}` has no `name` in its frontmatter"))?;
    validname(declared)?;
    anyhow::ensure!(
        declared == name,
        "`{name}` declares the name `{declared}`; the standard requires them to match"
    );

    let description = parsed
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("`{name}` has no `description` in its frontmatter"))?;
    anyhow::ensure!(
        description.chars().count() <= DESCRIPTIONLIMIT,
        "the description of `{name}` is longer than {DESCRIPTIONLIMIT} characters"
    );
    anyhow::ensure!(
        !body.trim().is_empty(),
        "`{name}` has no instructions after its frontmatter"
    );
    Ok((declared.to_owned(), description.to_owned()))
}

/// Build a whole `SKILL.md` from its parts, and refuse to return one that is
/// not valid.
///
/// This is how a skill an agent wrote gets made. The agent supplies the name,
/// the one-line description, and the instructions; Synapse writes the
/// frontmatter. Letting a model hand over raw frontmatter would mean letting it
/// write YAML keys nobody reads and a `name` that disagrees with its own
/// directory — so it does not get to.
pub fn compose(name: &str, description: &str, body: &str) -> Result<String> {
    validname(name)?;
    let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
    anyhow::ensure!(
        !description.is_empty(),
        "`{name}` needs a description saying when to reach for it"
    );
    let body = body.trim();
    anyhow::ensure!(!body.is_empty(), "`{name}` needs instructions");
    let content = format!(
        "---\nname: {name}\ndescription: {}\n---\n\n{body}\n",
        quoted(&description)
    );
    parse(name, &content)?;
    Ok(content)
}

/// A double-quoted YAML scalar, so a description holding `: ` or a quote is
/// text rather than a parse error.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Whether a frontmatter line holds an unquoted value that itself contains
/// `: `, which YAML reads as a nested key rather than as text.
fn unquotedcolon(line: &str) -> bool {
    let Some((_, value)) = line.split_once(": ") else {
        return false;
    };
    let value = value.trim();
    !value.starts_with('"') && !value.starts_with('\'') && value.contains(": ")
}

/// The naming rules the standard sets: 1 to 64 characters, lowercase letters,
/// digits, and single inner hyphens.
pub fn validname(name: &str) -> Result<()> {
    anyhow::ensure!(!name.is_empty(), "a skill name cannot be empty");
    anyhow::ensure!(
        name.chars().count() <= NAMELIMIT,
        "`{name}` is longer than {NAMELIMIT} characters"
    );
    anyhow::ensure!(
        name.bytes()
            .all(|item| item.is_ascii_lowercase() || item.is_ascii_digit() || item == b'-'),
        "`{name}` may only contain lowercase letters, digits, and hyphens"
    );
    anyhow::ensure!(
        !name.starts_with('-') && !name.ends_with('-'),
        "`{name}` cannot start or end with a hyphen"
    );
    anyhow::ensure!(
        !name.contains("--"),
        "`{name}` cannot contain consecutive hyphens"
    );
    Ok(())
}

/// Every file in the skill, relative to its root and sorted so the digest does
/// not depend on the order the filesystem hands them back.
pub fn contents(directory: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    collect(directory, directory, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<()> {
    let entries = std::fs::read_dir(directory)
        .with_context(|| format!("could not read {}", directory.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Editor leftovers and Synapse's own backups are not part of the skill.
        if name.starts_with('.') || name.ends_with(".synapsebackup") {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect(root, &path, files)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// A digest over every file's path and bytes, so any edit anywhere in the skill
/// changes it.
pub fn digest(directory: &Path, files: &[String]) -> Result<String> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.as_bytes());
        hasher.update([0]);
        let bytes = std::fs::read(directory.join(file))
            .with_context(|| format!("could not read {file}"))?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "---\nname: demo\ndescription: Does a thing. Use when a thing is needed.\n---\n\nDo the thing.\n";

    #[test]
    fn a_valid_skill_parses_into_its_name_and_description() {
        let (name, description) = parse("demo", GOOD).unwrap();
        assert_eq!(name, "demo");
        assert!(description.starts_with("Does a thing."));
    }

    #[test]
    fn the_body_is_everything_after_the_frontmatter() {
        let (frontmatter, body) = split(GOOD).unwrap();
        assert!(frontmatter.contains("name: demo"));
        assert_eq!(body, "Do the thing.\n");
    }

    #[test]
    fn a_file_without_frontmatter_is_refused() {
        assert!(parse("demo", "Just instructions.\n").is_err());
        assert!(parse("demo", "---\nname: demo\nno closing fence\n").is_err());
    }

    #[test]
    fn the_declared_name_has_to_match_the_directory() {
        let mismatched = "---\nname: other\ndescription: A thing.\n---\n\nBody.\n";
        let error = parse("demo", mismatched).unwrap_err().to_string();
        assert!(error.contains("requires them to match"), "got {error}");
    }

    #[test]
    fn both_required_fields_are_enforced() {
        assert!(parse("demo", "---\ndescription: A thing.\n---\n\nBody.\n").is_err());
        assert!(parse("demo", "---\nname: demo\n---\n\nBody.\n").is_err());
        assert!(parse("demo", "---\nname: demo\ndescription: \"\"\n---\n\nBody.\n").is_err());
    }

    #[test]
    fn a_skill_with_no_instructions_is_refused() {
        assert!(
            parse(
                "demo",
                "---\nname: demo\ndescription: A thing.\n---\n\n   \n"
            )
            .is_err()
        );
    }

    #[test]
    fn an_unquoted_colon_is_named_as_the_problem() {
        let broken = "---\nname: demo\ndescription: Does a thing: and another.\n---\n\nBody.\n";
        let error = format!("{:#}", parse("demo", broken).unwrap_err());
        assert!(error.contains("has to be quoted"), "got {error}");

        // Quoted, the same description is fine.
        let quoted = "---\nname: demo\ndescription: \"Does a thing: and another.\"\n---\n\nBody.\n";
        assert!(parse("demo", quoted).is_ok());
    }

    #[test]
    fn names_follow_the_standard() {
        for good in ["demo", "pdf-processing", "a", "x9-y"] {
            assert!(validname(good).is_ok(), "{good} should be valid");
        }
        for bad in [
            "",
            "PDF",
            "-lead",
            "trail-",
            "two--hyphens",
            "under_score",
            "has space",
        ] {
            assert!(validname(bad).is_err(), "{bad} should be refused");
        }
        assert!(validname(&"x".repeat(65)).is_err());
        assert!(validname(&"x".repeat(64)).is_ok());
    }

    #[test]
    fn an_overlong_description_is_refused() {
        let long = format!(
            "---\nname: demo\ndescription: {}\n---\n\nBody.\n",
            "x".repeat(1025)
        );
        assert!(parse("demo", &long).is_err());
    }

    #[test]
    fn reading_a_directory_collects_its_files_and_digest() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("demo");
        std::fs::create_dir_all(root.join("references")).unwrap();
        std::fs::write(root.join(ENTRY), GOOD).unwrap();
        std::fs::write(root.join("references").join("more.md"), "detail").unwrap();

        let skill = read(&root, Shelf::Global).unwrap();

        assert_eq!(skill.name, "demo");
        assert_eq!(skill.files, ["SKILL.md", "references/more.md"]);
        assert_eq!(skill.digest.len(), 64);
    }

    #[test]
    fn the_digest_follows_any_edit_anywhere_in_the_skill() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("demo");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(ENTRY), GOOD).unwrap();
        let first = read(&root, Shelf::Global).unwrap().digest;

        std::fs::write(root.join("extra.md"), "detail").unwrap();
        let second = read(&root, Shelf::Global).unwrap().digest;
        assert_ne!(first, second, "a new file must change the digest");

        std::fs::write(root.join("extra.md"), "different").unwrap();
        assert_ne!(
            second,
            read(&root, Shelf::Global).unwrap().digest,
            "so must an edit"
        );
    }

    #[test]
    fn backups_and_hidden_files_are_not_part_of_the_skill() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("demo");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(ENTRY), GOOD).unwrap();
        let clean = read(&root, Shelf::Global).unwrap();

        std::fs::write(root.join("SKILL.md.synapsebackup"), "old").unwrap();
        std::fs::write(root.join(".DS_Store"), "junk").unwrap();

        let after = read(&root, Shelf::Global).unwrap();
        assert_eq!(after.files, clean.files);
        assert_eq!(after.digest, clean.digest);
    }

    #[test]
    fn a_composed_skill_is_valid_and_its_description_survives_punctuation() {
        let content = compose(
            "cut-a-release",
            "Use this when: shipping a build, including the notarization step.",
            "1. Bump the version.\n2. Tag it.",
        )
        .unwrap();

        let (name, description) = parse("cut-a-release", &content).unwrap();
        assert_eq!(name, "cut-a-release");
        assert!(
            description.contains("Use this when: shipping"),
            "got {description}"
        );
        assert!(content.ends_with("2. Tag it.\n"));
    }

    #[test]
    fn composing_refuses_what_would_not_be_a_skill() {
        assert!(compose("Bad Name", "Fine.", "Steps.").is_err());
        assert!(compose("mine", "   ", "Steps.").is_err());
        assert!(compose("mine", "Fine.", "  \n ").is_err());
        // A quote in the description is text, not a broken fence.
        let quoted = compose("mine", "Say \"hello\" first.", "Steps.").unwrap();
        assert_eq!(parse("mine", &quoted).unwrap().1, "Say \"hello\" first.");
    }

    #[test]
    fn a_shelf_slug_is_readable_and_specific_to_the_path() {
        let left = slug(Path::new("/one/api"));
        let right = slug(Path::new("/two/api"));
        assert!(left.starts_with("api-"), "got {left}");
        assert_ne!(left, right);
        assert_eq!(left, slug(Path::new("/one/api")));
        assert!(slug(Path::new("/")).starts_with("project-"));
    }
}
