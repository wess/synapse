//! A repository's identity, independent of where it is checked out.
//!
//! A project used to be keyed by its absolute path, which is only an identity
//! on one machine. The same repository at `/Users/a/code/app` and
//! `/home/b/src/app` was two projects, so a memory synced from one recalled
//! nothing on the other, and a second clone on the same machine started from
//! empty. The `origin` remote is the name the repository already has
//! everywhere, so it is the key whenever there is one.

use std::path::{Path, PathBuf};

/// The normalized `origin` remote of the repository rooted at `root`, or `None`
/// when it has no `.git`, no `origin`, or one that is not a network address.
///
/// Read from the config file rather than by running `git`: this runs on every
/// recall, a hook has a timeout, and a machine without git installed still has
/// repositories it cloned some other way.
pub fn origin(root: &Path) -> Option<String> {
    let config = std::fs::read_to_string(common(root)?.join("config")).ok()?;
    normalize(&url(&config)?)
}

/// Whether `value` is already a stable key rather than a path on this disk:
/// `host/owner/repo`, with a host that has a dot in it. A key read back from a
/// memory and handed in again — by the desktop editor, or by an update that
/// keeps a memory's project — has to stay what it is, and it never resolves as
/// a folder.
pub fn stable(value: &str) -> bool {
    let mut parts = value.split('/');
    let host = parts.next().unwrap_or_default();
    !value.starts_with(['/', '.', '~'])
        && host.contains('.')
        && !host.contains([' ', '\\', ':'])
        && parts.filter(|part| !part.is_empty()).count() >= 1
}

/// The directory holding the repository's shared config. A plain checkout has
/// it at `.git`; a worktree or submodule has a `.git` file naming its git
/// directory, and a worktree's git directory names the common one in turn.
fn common(root: &Path) -> Option<PathBuf> {
    let dotgit = root.join(".git");
    if dotgit.is_dir() {
        return Some(dotgit);
    }
    let pointer = std::fs::read_to_string(&dotgit).ok()?;
    let gitdir = root.join(pointer.trim().strip_prefix("gitdir:")?.trim());
    match std::fs::read_to_string(gitdir.join("commondir")) {
        Ok(commondir) => Some(gitdir.join(commondir.trim())),
        Err(_) => Some(gitdir),
    }
}

/// The `url` of `[remote "origin"]`, the first one if there are several.
fn url(config: &str) -> Option<String> {
    let mut inside = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            let header: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            inside = header == "[remote\"origin\"]" || header == "[remote.origin]";
            continue;
        }
        if !inside {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("url") {
            let value = value.trim().trim_matches('"').trim();
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

/// `host/path`, from any of the spellings git accepts for the same remote.
///
/// Credentials are dropped, never kept. A remote written as
/// `https://user:token@host/...` is common, and this string is stored beside
/// every memory and sent to every machine the store syncs with.
///
/// A local or `file://` remote is not an identity — it names a folder on one
/// disk, which is the problem this exists to solve — so it has none.
pub fn normalize(url: &str) -> Option<String> {
    let url = url.trim();
    let (authority, path) = match url.split_once("://") {
        Some((scheme, _)) if scheme.eq_ignore_ascii_case("file") => return None,
        Some((_, rest)) => rest.split_once('/')?,
        // scp-like: `user@host:owner/repo`. A colon after a slash is a path,
        // and so is one after a single letter, which is a Windows drive.
        None => {
            let (authority, path) = url.split_once(':')?;
            if authority.contains('/') || authority.chars().count() < 2 {
                return None;
            }
            (authority, path)
        }
    };
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?.to_lowercase();
    let path = path.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{host}/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_of_one_remote_is_one_key() {
        for url in [
            "https://github.com/wess/synapse.git",
            "https://github.com/wess/synapse",
            "https://github.com/wess/synapse/",
            "git@github.com:wess/synapse.git",
            "ssh://git@github.com/wess/synapse.git",
            "ssh://git@GitHub.com:22/wess/synapse",
            "git://github.com/wess/synapse.git",
        ] {
            assert_eq!(
                normalize(url).as_deref(),
                Some("github.com/wess/synapse"),
                "{url}"
            );
        }
    }

    /// The key travels to every synced machine, so a token in the remote must
    /// not travel with it.
    #[test]
    fn credentials_in_a_remote_are_dropped() {
        assert_eq!(
            normalize("https://wess:ghp_abc123@github.com/wess/synapse.git").as_deref(),
            Some("github.com/wess/synapse")
        );
    }

    #[test]
    fn a_local_remote_is_not_an_identity() {
        assert_eq!(normalize("/srv/git/app.git"), None);
        assert_eq!(normalize("../app"), None);
        assert_eq!(normalize("file:///srv/git/app.git"), None);
        assert_eq!(normalize("C:/repos/app"), None);
    }

    #[test]
    fn origin_is_read_from_the_config_and_other_remotes_are_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let git = directory.path().join(".git");
        std::fs::create_dir(&git).unwrap();
        std::fs::write(
            git.join("config"),
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@github.com:other/app.git\n\
             [remote \"origin\"]\n\turl = git@github.com:wess/app.git\n\tfetch = +refs/heads/*\n",
        )
        .unwrap();

        assert_eq!(
            origin(directory.path()).as_deref(),
            Some("github.com/wess/app")
        );
    }

    #[test]
    fn a_worktree_reads_the_config_of_the_repository_it_belongs_to() {
        let directory = tempfile::tempdir().unwrap();
        let main = directory.path().join("main");
        let git = main.join(".git");
        std::fs::create_dir_all(git.join("worktrees").join("feature")).unwrap();
        std::fs::write(
            git.join("config"),
            "[remote \"origin\"]\n\turl = https://gitlab.com/team/app\n",
        )
        .unwrap();
        std::fs::write(git.join("worktrees/feature/commondir"), "../..\n").unwrap();
        let tree = directory.path().join("feature");
        std::fs::create_dir(&tree).unwrap();
        std::fs::write(
            tree.join(".git"),
            format!("gitdir: {}\n", git.join("worktrees/feature").display()),
        )
        .unwrap();

        assert_eq!(origin(&tree).as_deref(), Some("gitlab.com/team/app"));
    }

    #[test]
    fn a_repository_without_origin_has_no_key() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join(".git")).unwrap();
        std::fs::write(directory.path().join(".git/config"), "[core]\n").unwrap();
        assert_eq!(origin(directory.path()), None);
    }

    #[test]
    fn keys_and_paths_are_told_apart() {
        assert!(stable("github.com/wess/synapse"));
        assert!(!stable("/Users/wess/synapse"));
        assert!(!stable("./synapse"));
        assert!(!stable("synapse"));
        assert!(!stable("github.com"));
        assert!(!stable("~/code/app"));
    }
}
