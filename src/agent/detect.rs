use crate::agent::{Agent, Detection};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn detect(agent: &Agent, server: Option<&Path>) -> Detection {
    let Some(executable) = executable(agent.command) else {
        return Detection::missing();
    };
    let version = Command::new(&executable)
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
    let mut directories = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Ok(home) = crate::files::home() {
        directories.extend([
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".asdf/shims"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]);
    }
    directories
        .into_iter()
        .map(|directory| directory.join(command))
        .find(|path| path.is_file())
}
