use crate::agent::{Agent, Detection};
use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn detect(agent: &Agent, server: Option<&Path>) -> Detection {
    let Some(executable) = executable(&agent.command) else {
        return Detection::missing();
    };
    let version = command(&executable)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|output| !output.is_empty());
    let (registered, configured) = crate::agent::config::state(agent, server);
    // Only Claude Code has settings for a session notice and a status line.
    let hooks = match (agent.kind, server) {
        (crate::agent::Kind::Claude, Some(server)) => {
            crate::agent::hooks::state(&agent.settings, server)
        }
        _ => crate::agent::HookState::default(),
    };
    Detection {
        executable: Some(executable),
        version,
        registered,
        configured,
        hooks,
    }
}

fn executable(command: &str) -> Option<PathBuf> {
    let candidate = Path::new(command);
    if candidate.components().count() > 1 && candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    directories()
        .into_iter()
        .map(|directory| directory.join(command))
        .find(|path| path.is_file())
}

/// Everywhere a tool's binary may live, inherited PATH first so a person's own
/// PATH stays authoritative and the fallbacks only fill in what it is missing.
///
/// The desktop app is launched by the Finder, which hands it
/// `/usr/bin:/bin:/usr/sbin:/sbin` and nothing a person ever configured — so the
/// fallbacks are the only reason a GUI connection finds anything at all.
fn directories() -> Vec<PathBuf> {
    let mut directories = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Ok(home) = crate::files::home() {
        directories.extend([
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".asdf/shims"),
            // Where a version manager itself lives, which is not the same place
            // as its shims: asdf's own installer puts it here, Homebrew puts it
            // in the two below, and the same two carry mise and friends.
            home.join(".asdf/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]);
    }
    let mut seen = HashSet::new();
    directories.retain(|directory| seen.insert(directory.clone()));
    directories
}

/// The PATH a tool Synapse starts is given.
///
/// Finding the binary is only half the problem: what detection finds is often a
/// version manager's shim, and a shim is a script that execs its manager by
/// name. `~/.asdf/shims/codex` is three lines ending in `exec asdf exec codex`,
/// so under the Finder's PATH it resolves, runs, and fails with
/// `exec: asdf: not found`. Handing the child the same directories detection
/// searched is what makes the shim work.
///
/// `None` when the directories cannot be joined — a PATH entry containing the
/// separator — so the child simply inherits ours instead.
pub fn searchpath() -> Option<OsString> {
    env::join_paths(directories()).ok()
}

/// Run a detected tool with a PATH wide enough for whatever it turns out to be.
pub fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    if let Some(path) = searchpath() {
        command.env("PATH", path);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_path_keeps_the_inherited_one_first_and_adds_each_place_once() {
        let mut seen = HashSet::new();
        let inherited: Vec<PathBuf> = env::var_os("PATH")
            .map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|directory| seen.insert(directory.clone()))
            .collect();
        let directories = directories();

        assert_eq!(directories[..inherited.len()], inherited[..]);
        let home = crate::files::home().unwrap();
        for expected in [home.join(".asdf/shims"), home.join(".asdf/bin")] {
            assert_eq!(
                directories
                    .iter()
                    .filter(|entry| **entry == expected)
                    .count(),
                1,
                "{} should appear exactly once in {directories:?}",
                expected.display()
            );
        }
    }
}
