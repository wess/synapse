use crate::vault::Shell;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const START: &str = "# synaps:shell:begin";
const END: &str = "# synaps:shell:end";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegrationState {
    Missing,
    Installed,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Integration {
    pub shell: &'static str,
    pub path: PathBuf,
    pub state: IntegrationState,
}

pub fn status(command: &Path) -> Result<Integration> {
    let (shell, path) = target()?;
    inspect(shell, path, command)
}

pub fn install(command: &Path) -> Result<Integration> {
    let (shell, path) = target()?;
    installat(shell, path, command)
}

pub fn remove(command: &Path) -> Result<Integration> {
    let (shell, path) = target()?;
    removeat(shell, path, command)
}

fn installat(shell: Shell, path: PathBuf, command: &Path) -> Result<Integration> {
    let current = read(&path)?;
    let block = managed(shell, command)?;
    let content = merge(&current, &block)
        .with_context(|| format!("could not update the Synaps block in {}", path.display()))?;
    crate::files::write(&path, &content)
        .with_context(|| format!("could not install the shell hook in {}", path.display()))?;
    inspect(shell, path, command)
}

fn removeat(shell: Shell, path: PathBuf, command: &Path) -> Result<Integration> {
    let current = read(&path)?;
    let content = strip(&current)
        .with_context(|| format!("could not remove the Synaps block in {}", path.display()))?;
    if content != current {
        crate::files::write(&path, &content)
            .with_context(|| format!("could not remove the shell hook from {}", path.display()))?;
    }
    inspect(shell, path, command)
}

fn target() -> Result<(Shell, PathBuf)> {
    let value = std::env::var_os("SHELL").context("the default shell is not available")?;
    let name = Path::new(&value)
        .file_name()
        .and_then(|name| name.to_str())
        .context("the default shell name is not valid UTF-8")?;
    let shell = match name {
        "zsh" => Shell::Zsh,
        "bash" => Shell::Bash,
        "fish" => Shell::Fish,
        _ => anyhow::bail!("{name} shell integration is not supported"),
    };
    let home = crate::files::home()?;
    let path = match shell {
        Shell::Zsh => std::env::var_os("ZDOTDIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or(home)
            .join(".zshrc"),
        Shell::Bash if cfg!(target_os = "macos") => home.join(".bash_profile"),
        Shell::Bash => home.join(".bashrc"),
        Shell::Fish => home.join(".config").join("fish").join("config.fish"),
    };
    Ok((shell, path))
}

fn inspect(shell: Shell, path: PathBuf, command: &Path) -> Result<Integration> {
    let current = read(&path)?;
    let range = managedrange(&current)
        .with_context(|| format!("could not inspect the Synaps block in {}", path.display()))?;
    let state = match range {
        None => IntegrationState::Missing,
        Some((start, end)) if current[start..end] == managed(shell, command)? => {
            IntegrationState::Installed
        }
        Some(_) => IntegrationState::Modified,
    };
    Ok(Integration {
        shell: shellname(shell),
        path,
        state,
    })
}

fn read(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
    }
}

fn managed(shell: Shell, command: &Path) -> Result<String> {
    let command = command
        .to_str()
        .context("the installed CLI path is not valid UTF-8")?;
    let quoted = match shell {
        Shell::Bash | Shell::Zsh => posixquote(command),
        Shell::Fish => fishquote(command),
    };
    let setup = match shell {
        Shell::Bash | Shell::Zsh => format!(
            "eval \"$(env SYNAPS_SHELL_COMMAND={quoted} {quoted} hook {})\"",
            shellname(shell)
        ),
        Shell::Fish => format!("env SYNAPS_SHELL_COMMAND={quoted} {quoted} hook fish | source"),
    };
    Ok(format!(
        "{START}\n# Managed by Synaps Settings. Changes inside this block may be replaced.\n{setup}\n{END}"
    ))
}

fn merge(current: &str, block: &str) -> Result<String> {
    if let Some((start, end)) = managedrange(current)? {
        return Ok(format!("{}{}{}", &current[..start], block, &current[end..]));
    }
    if current.is_empty() {
        return Ok(format!("{block}\n"));
    }
    let separator = if current.ends_with("\n\n") {
        ""
    } else if current.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    Ok(format!("{current}{separator}{block}\n"))
}

fn strip(current: &str) -> Result<String> {
    let Some((start, mut end)) = managedrange(current)? else {
        return Ok(current.to_owned());
    };
    if current.as_bytes().get(end) == Some(&b'\n') {
        end += 1;
    }
    let content = format!("{}{}", &current[..start], &current[end..]);
    Ok(if content.trim().is_empty() {
        String::new()
    } else {
        content
    })
}

fn managedrange(content: &str) -> Result<Option<(usize, usize)>> {
    let starts = content.match_indices(START).collect::<Vec<_>>();
    let ends = content.match_indices(END).collect::<Vec<_>>();
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([(start, _)], [(end, _)]) if end > start => Ok(Some((*start, *end + END.len()))),
        _ => anyhow::bail!(
            "the shell file contains an incomplete or duplicate Synaps hook block; repair the markers manually before using Settings"
        ),
    }
}

fn shellname(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "bash",
        Shell::Fish => "fish",
        Shell::Zsh => "zsh",
    }
}

fn posixquote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn fishquote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_idempotent_and_preserves_user_content() {
        let command = Path::new("/Applications/Synaps App/synaps");
        let block = managed(Shell::Zsh, command).unwrap();
        let original = "export EDITOR=vim\n";
        let first = merge(original, &block).unwrap();
        let second = merge(&first, &block).unwrap();

        assert_eq!(first, second);
        assert!(first.starts_with(original));
        assert!(first.contains("'/Applications/Synaps App/synaps' hook zsh"));
    }

    #[test]
    fn modified_managed_block_is_repaired_without_touching_its_neighbors() {
        let block = managed(Shell::Bash, Path::new("/new/synaps")).unwrap();
        let current = format!("before\n{START}\nold\n{END}\nafter\n");
        let merged = merge(&current, &block).unwrap();

        assert!(merged.starts_with("before\n"));
        assert!(merged.ends_with("\nafter\n"));
        assert!(!merged.contains("\nold\n"));
    }

    #[test]
    fn removal_keeps_content_outside_the_managed_block() {
        let block = managed(Shell::Fish, Path::new("/opt/synaps")).unwrap();
        let current = format!("set -gx EDITOR vim\n\n{block}\nset -gx PAGER less\n");
        let stripped = strip(&current).unwrap();

        assert!(stripped.contains("set -gx EDITOR vim"));
        assert!(stripped.contains("set -gx PAGER less"));
        assert!(!stripped.contains(START));
    }

    #[test]
    fn malformed_or_duplicate_blocks_are_never_rewritten() {
        assert!(merge(START, "replacement").is_err());
        assert!(strip(&format!("{START}\n{END}\n{START}\n{END}")).is_err());
    }

    #[test]
    fn file_install_and_remove_are_backed_up_and_detected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("zshrc");
        let command = Path::new("/opt/synaps");
        fs::write(&path, "export EDITOR=vim\n").unwrap();

        let installed = installat(Shell::Zsh, path.clone(), command).unwrap();
        assert_eq!(installed.state, IntegrationState::Installed);
        assert_eq!(
            fs::read_to_string(directory.path().join("zshrc.synapsbackup")).unwrap(),
            "export EDITOR=vim\n"
        );

        let removed = removeat(Shell::Zsh, path.clone(), command).unwrap();
        assert_eq!(removed.state, IntegrationState::Missing);
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains("export EDITOR=vim")
        );
    }
}
