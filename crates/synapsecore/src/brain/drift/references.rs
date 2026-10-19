//! File paths mentioned in a memory. Deliberately conservative: a token needs a
//! slash plus a file extension, or a slash inside a code span. "read/write" and
//! "CI/CD" aren't paths, and false alarms train people to ignore the check.

/// Top-level folders an absolute path can start with. Anything else with a
/// leading slash is a slash command (`/loop`), a route, or a container path.
const ROOTS: &[&str] = &[
    "Users",
    "home",
    "etc",
    "var",
    "tmp",
    "private",
    "opt",
    "usr",
    "Library",
    "System",
    "Applications",
    "Volumes",
    "mnt",
    "srv",
    "root",
    "nix",
];

/// Globs, placeholders, and shell syntax.
const NOTPATH: &[char] = &[
    '*', '?', '{', '}', '<', '>', '[', ']', '$', '|', '&', ';', '=', '@', '\\', '%', '#', '"',
];

/// Paths in `body`, deduplicated, in order.
pub fn references(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (piece, spanned) in spans(body) {
        let candidates: Vec<String> = match spanned {
            // A span with spaces is a path only if it's rooted, like
            // `~/Library/Application Support/x`. Otherwise it's a command.
            true if piece.contains(char::is_whitespace) => rooted(&piece).into_iter().collect(),
            _ => prose(&piece, spanned),
        };
        for candidate in candidates {
            if !found.contains(&candidate) {
                found.push(candidate);
            }
        }
    }
    found
}

/// Splits `body` into prose and quoted spans (backticks or double quotes),
/// since either can hold a path with a space in it. An unclosed quote leaves
/// the rest as prose.
fn spans(body: &str) -> Vec<(String, bool)> {
    let mut spans = Vec::new();
    for (code, quoted) in delimited(body, '`') {
        if quoted {
            spans.push((code, true));
            continue;
        }
        let straightened = code.replace(['“', '”'], "\"");
        spans.extend(delimited(&straightened, '"'));
    }
    spans
}

fn delimited(text: &str, mark: char) -> Vec<(String, bool)> {
    let pieces: Vec<&str> = text.split(mark).collect();
    let closed = pieces.len() % 2 == 1;
    pieces
        .iter()
        .enumerate()
        .map(|(index, piece)| {
            let inside = index % 2 == 1 && (closed || index + 1 < pieces.len());
            ((*piece).to_owned(), inside)
        })
        .collect()
}

/// Paths among whitespace-separated tokens. When a rooted path ends without a
/// file name, the next token is probably its continuation after a space (a
/// bare `~/Library/Application Support/x.md`), so it's skipped.
fn prose(piece: &str, spanned: bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut broken = false;
    for token in piece.split_whitespace() {
        let continuing = broken;
        let bare = trimmed(token);
        broken = (bare.starts_with('/') || bare.starts_with("~/"))
            && !bare.ends_with('/')
            && !bare.rsplit('/').next().is_some_and(extension);
        if continuing {
            continue;
        }
        if let Some(path) = path(token, spanned) {
            found.push(path);
        }
    }
    found
}

fn rooted(span: &str) -> Option<String> {
    let span = span.trim();
    let starts = span.starts_with('/') || span.starts_with("~/");
    (starts && !span.contains('\n') && !span.contains(NOTPATH))
        .then(|| trimmed(span))
        .filter(|path| grounded(path))
}

/// Under home, or under a known root with at least one more component.
fn grounded(path: &str) -> bool {
    if let Some(rest) = path.strip_prefix("~/") {
        return !rest.trim_matches('/').is_empty();
    }
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    parts.next().is_some_and(|first| ROOTS.contains(&first)) && parts.next().is_some()
}

fn path(token: &str, spanned: bool) -> Option<String> {
    let token = trimmed(token);
    if !token.contains('/') || token.contains("://") || token.contains("..") {
        return None;
    }
    if token.contains(NOTPATH) || token.starts_with("www.") || token == "~/" || token == "/" {
        return None;
    }
    // Route parameters (`/:id`) and drive letters (`C:/`).
    if token.split('/').any(|part| part.contains(':')) {
        return None;
    }
    if token.starts_with('/') && !grounded(&token) {
        return None;
    }
    let segments: Vec<&str> = token.split('/').filter(|part| !part.is_empty()).collect();
    if segments.is_empty() {
        return None;
    }
    // Fractions and dates.
    if segments
        .iter()
        .all(|part| part.chars().all(|character| character.is_ascii_digit()))
    {
        return None;
    }
    // A dot in the first part means a host or a name like `Node.js`, unless
    // it's a dotfolder like `.github`.
    let first = segments[0];
    let relative = !token.starts_with('/') && !token.starts_with("~/");
    if relative && first != "." && !first.starts_with('.') && first.contains('.') {
        return None;
    }
    let last = segments[segments.len() - 1];
    let named = extension(last);
    // In prose a path must end in a file name. In a code span a folder with
    // at least two parts is fine.
    let accepted = named || (spanned && (segments.len() >= 2 || !relative));
    accepted.then(|| token.trim_start_matches("./").to_owned())
}

/// A dot followed by 1-10 alphanumerics, at least one a letter.
fn extension(name: &str) -> bool {
    let Some((stem, suffix)) = name.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && (1..=10).contains(&suffix.len())
        && suffix
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        && suffix
            .chars()
            .any(|character| character.is_ascii_alphabetic())
}

/// Strips surrounding punctuation and quotes, and any `:line:column` suffix.
fn trimmed(token: &str) -> String {
    let token = token
        .trim_start_matches(['(', '[', '\'', '"', '“', '‘'])
        .trim_end_matches([')', ']', '\'', '"', ',', ';', ':', '.', '!', '?', '”', '’']);
    let mut parts: Vec<&str> = token.split(':').collect();
    while parts.len() > 1
        && parts.last().is_some_and(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
    {
        parts.pop();
    }
    parts.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(body: &str) -> Vec<String> {
        references(body)
    }

    #[test]
    fn a_path_with_a_file_name_is_found_in_prose() {
        assert_eq!(
            named("Secret entry guards live in crates/synapsecore/src/vault/entry.rs today."),
            vec!["crates/synapsecore/src/vault/entry.rs"]
        );
        assert_eq!(
            named("See docs/retrieval.md, then (website/build.ts)."),
            vec!["docs/retrieval.md", "website/build.ts"]
        );
    }

    #[test]
    fn a_folder_counts_only_inside_a_code_span() {
        assert_eq!(
            named("Components live in `src/components/`."),
            vec!["src/components/"]
        );
        assert!(named("Components live in src/components somewhere.").is_empty());
    }

    #[test]
    fn home_and_absolute_paths_are_found() {
        assert_eq!(
            named("Response doc written to ~/Desktop/db_response.md."),
            vec!["~/Desktop/db_response.md"]
        );
        assert_eq!(
            named("Logs go to /var/log/synapse.log"),
            vec!["/var/log/synapse.log"]
        );
        assert_eq!(
            named("Read `~/Library/Application Support/synapse/SOUL.md` first."),
            vec!["~/Library/Application Support/synapse/SOUL.md"]
        );
    }

    /// Seen in a real store, where these were reported as missing paths.
    #[test]
    fn slash_commands_and_routes_are_not_paths() {
        for body in [
            "Type `/loop` then `/mcp` and `/trust allow` in the prompt.",
            "Run `/agent N` to switch.",
            "The proxy forwards `/v1/messages` and `/v1/chat/completions`.",
            "Rosters load from `/:eventId/roster` and `/item/:itemId`.",
            "The search hits `/rest/api/3/search` with a JQL query.",
            "Deploy the container's `/workspace/app/main.py` entry point.",
            "Windows keeps it in `C:/Users/me/app.toml`.",
        ] {
            assert!(named(body).is_empty(), "{body} gave {:?}", named(body));
        }
        assert_eq!(
            named("Policy lives in `/etc/solar/policy.toml`."),
            vec!["/etc/solar/policy.toml"]
        );
        assert_eq!(named("Config is `~/.claude.json`."), vec!["~/.claude.json"]);
        assert_eq!(
            named("Projects sit in `~/Desktop/Dev/asylum`."),
            vec!["~/Desktop/Dev/asylum"]
        );
    }

    #[test]
    fn line_numbers_and_leading_dot_slash_are_dropped() {
        assert_eq!(
            named("Fails at src/brain/rank.rs:154:9."),
            vec!["src/brain/rank.rs"]
        );
        assert_eq!(
            named("Run ./scripts/release.sh"),
            vec!["scripts/release.sh"]
        );
        assert_eq!(
            named("CI lives in .github/workflows/ci.yml"),
            vec![".github/workflows/ci.yml"]
        );
    }

    #[test]
    fn words_with_slashes_are_not_paths() {
        for body in [
            "Handles read/write and on/off and and/or cases.",
            "We do CI/CD over TCP/IP at 60 km/h with I/O.",
            "Half of it, 1/2, shipped on 10/19/2026.",
            "The repo is github.com/wess/synapse and docs.rs/serde explains it.",
            "Fetch https://example.com/a/b.html for the spec.",
            "Install @anthropic-ai/sdk and @types/node.",
            "Glob `crates/*/src/**/*.rs` and pick `src/{a,b}.ts`.",
            "Use `src/user/{index.ts, profile.ts}` rather than one file.",
            "Supersede with `synapse memory supersede <old> <new>`.",
            "Run `cd crates/synapsecore && cargo build` from the root.",
            "We moved from Node.js/Deno to Bun.",
            "Version v1.2/v1.3 changed the format.",
            "Paths like ../shared/config.toml escape the root.",
            "Nothing here: / and ~/ alone.",
        ] {
            assert!(named(body).is_empty(), "{body} gave {:?}", named(body));
        }
    }

    #[test]
    fn each_path_is_named_once_in_order() {
        assert_eq!(
            named("Edit src/a.rs, then src/b.rs, then src/a.rs again."),
            vec!["src/a.rs", "src/b.rs"]
        );
    }

    /// From a real memory: the tail after the space was looked up as a repo path.
    #[test]
    fn a_double_quoted_path_with_a_space_is_one_path() {
        assert_eq!(
            named(
                "(expects \"~/Library/Application Support/ainz/AGENTS.md\", gets \"~/.ainz/AGENTS.md\")."
            ),
            vec![
                "~/Library/Application Support/ainz/AGENTS.md",
                "~/.ainz/AGENTS.md"
            ]
        );
        assert_eq!(
            named("Kept in “~/Library/Application Support/synapse/brain.db” on macOS."),
            vec!["~/Library/Application Support/synapse/brain.db"]
        );
        assert!(named("He said \"use the release branch\" twice.").is_empty());
    }

    #[test]
    fn a_bare_path_broken_by_a_space_is_not_looked_up_in_pieces() {
        assert!(named("Lives at ~/Library/Application Support/ainz/AGENTS.md now.").is_empty());
        assert!(named("Under /Volumes/My Drive/backups/brain.db nightly.").is_empty());
        // A complete path before a space does not swallow the next one.
        assert_eq!(
            named("Copy ~/notes/a.md then docs/b.md"),
            vec!["~/notes/a.md", "docs/b.md"]
        );
    }

    #[test]
    fn an_unclosed_backtick_is_read_as_prose() {
        assert_eq!(
            named("Broken `span src/a.rs and src/folder"),
            vec!["src/a.rs"]
        );
    }

    #[test]
    fn an_extension_needs_a_letter_and_a_stem() {
        assert!(extension("entry.rs"));
        assert!(extension("archive.tar.gz"));
        assert!(extension("track.mp3"));
        assert!(!extension("v1.2"));
        assert!(!extension(".gitignore"));
        assert!(!extension("folder"));
        assert!(!extension("file.toolongextension"));
    }
}
