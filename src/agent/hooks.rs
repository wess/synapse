//! The Claude Code settings Synapse manages: a SessionStart hook and, when
//! nothing else claims it, the status line.
//!
//! The hook is the only way to state the connection *before* the model has
//! written anything, so it lands beside the welcome box rather than partway
//! through the first reply. It runs `synapse session`, whose `systemMessage`
//! Claude Code displays to the user.
//!
//! Settings files carry no comments, so there is no managed block to hide
//! behind. Synapse's own entries are recognised by the command they run, and
//! everything else in the file is left exactly as it was found. A status line
//! somebody else configured is never replaced: it is reported instead, so the
//! choice stays with the user.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;

/// Which session starts the notice is worth printing for. A compaction is not
/// one of them: the user is mid-session and has already seen it.
const MATCHER: &str = "startup|resume|clear";

/// A hook that has to finish before the welcome box is drawn cannot be allowed
/// to hang the session on a slow disk.
const TIMEOUT: u64 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct State {
    /// The SessionStart hook is present and points at this binary.
    pub notice: bool,
    /// The status line is present and points at this binary.
    pub statusline: bool,
    /// Something other than Synapse owns the status line, so Synapse left it
    /// alone.
    pub borrowed: bool,
}

pub fn state(settings: &Path, binary: &Path) -> State {
    let Some(value) = read(settings).ok() else {
        return State::default();
    };
    let existing = value.get("statusLine").and_then(|line| command(line));
    State {
        notice: sessionhooks(&value)
            .iter()
            .any(|group| ours(group, binary, "session")),
        statusline: existing
            .as_deref()
            .is_some_and(|command| points(command, binary, "statusline")),
        borrowed: existing
            .as_deref()
            .is_some_and(|command| !points(command, binary, "statusline")),
    }
}

/// Install or refresh both settings, returning what the file ended up with.
pub fn apply(settings: &Path, binary: &Path) -> Result<State> {
    let mut value = read(settings)?;

    // Drop any SessionStart group Synapse wrote before, including one that
    // points at a binary that has since moved, then add the current one. Groups
    // belonging to anything else are carried through untouched.
    let mut groups: Vec<Value> = sessionhooks(&value)
        .into_iter()
        .filter(|group| !ours(group, binary, "session") && !stale(group, "session"))
        .collect();
    groups.push(json!({
        "matcher": MATCHER,
        "hooks": [{
            "type": "command",
            "command": format!("{} session", quoted(binary)),
            "timeout": TIMEOUT,
        }],
    }));
    let object = value
        .as_object_mut()
        .context("the settings file is not a JSON object")?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("the `hooks` setting is not a JSON object")?;
    hooks.insert("SessionStart".to_owned(), Value::Array(groups));

    // A status line somebody else configured stays theirs.
    let existing = object.get("statusLine").and_then(command);
    let claimable = existing.as_deref().is_none_or(|command| {
        points(command, binary, "statusline") || stalecommand(command, "statusline")
    });
    if claimable {
        object.insert(
            "statusLine".to_owned(),
            json!({
                "type": "command",
                "command": format!("{} statusline", quoted(binary)),
                "padding": 0,
            }),
        );
    }

    write(settings, &value)?;
    Ok(state(settings, binary))
}

/// Take both settings back out, leaving anything Synapse did not write.
pub fn remove(settings: &Path, binary: &Path) -> Result<()> {
    if !settings.exists() {
        return Ok(());
    }
    let mut value = read(settings)?;
    let groups: Vec<Value> = sessionhooks(&value)
        .into_iter()
        .filter(|group| !ours(group, binary, "session") && !stale(group, "session"))
        .collect();
    let object = value
        .as_object_mut()
        .context("the settings file is not a JSON object")?;
    if let Some(hooks) = object.get_mut("hooks").and_then(Value::as_object_mut) {
        if groups.is_empty() {
            hooks.remove("SessionStart");
        } else {
            hooks.insert("SessionStart".to_owned(), Value::Array(groups));
        }
        if hooks.is_empty() {
            object.remove("hooks");
        }
    }
    // Only a status line Synapse put there is taken away again.
    if object
        .get("statusLine")
        .and_then(command)
        .as_deref()
        .is_some_and(|command| {
            points(command, binary, "statusline") || stalecommand(command, "statusline")
        })
    {
        object.remove("statusLine");
    }
    write(settings, &value)
}

fn read(settings: &Path) -> Result<Value> {
    let raw = match std::fs::read_to_string(settings) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".to_owned(),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", settings.display()));
        }
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(raw).with_context(|| format!("could not read {}", settings.display()))
}

fn write(settings: &Path, value: &Value) -> Result<()> {
    crate::files::write(
        settings,
        &format!("{}\n", serde_json::to_string_pretty(value)?),
    )
}

fn sessionhooks(value: &Value) -> Vec<Value> {
    value
        .pointer("/hooks/SessionStart")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Whether every command in a hook group is this binary running `subcommand`.
fn ours(group: &Value, binary: &Path, subcommand: &str) -> bool {
    let entries = group.get("hooks").and_then(Value::as_array);
    entries.is_some_and(|entries| {
        !entries.is_empty()
            && entries.iter().all(|entry| {
                command(entry).is_some_and(|command| points(&command, binary, subcommand))
            })
    })
}

/// Whether a group is a Synapse hook left by an older install whose binary has
/// since moved. Recognising it is what keeps setup from stacking up duplicates.
fn stale(group: &Value, subcommand: &str) -> bool {
    let entries = group.get("hooks").and_then(Value::as_array);
    entries.is_some_and(|entries| {
        !entries.is_empty()
            && entries.iter().all(|entry| {
                command(entry).is_some_and(|command| stalecommand(&command, subcommand))
            })
    })
}

fn command(entry: &Value) -> Option<String> {
    entry
        .get("command")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// Whether a configured command is this binary running `subcommand`.
fn points(command: &str, binary: &Path, subcommand: &str) -> bool {
    let expected = format!("{} {subcommand}", quoted(binary));
    command.trim() == expected || command.trim() == format!("{} {subcommand}", binary.display())
}

/// Whether a configured command is *some* Synapse binary running `subcommand`.
/// A path that has moved still has to be recognised, or every setup would add
/// another copy beside the last one.
fn stalecommand(command: &str, subcommand: &str) -> bool {
    let command = command.trim().trim_end_matches('\'');
    let Some(program) = command.strip_suffix(subcommand).map(str::trim_end) else {
        return false;
    };
    Path::new(program.trim_matches('\''))
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "synapse")
}

/// A path with a space in it has to survive being run through a shell.
fn quoted(binary: &Path) -> String {
    let path = binary.display().to_string();
    if path.contains(|item: char| item.is_whitespace() || item == '\'') {
        return format!("'{}'", path.replace('\'', "'\\''"));
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("settings.json");
        let binary = directory.path().join("synapse");
        (directory, settings, binary)
    }

    #[test]
    fn a_fresh_install_adds_both_settings() {
        let (_directory, settings, binary) = setup();

        let state = apply(&settings, &binary).unwrap();

        assert_eq!(
            state,
            State {
                notice: true,
                statusline: true,
                borrowed: false
            }
        );
        let value = read(&settings).unwrap();
        assert_eq!(value["hooks"]["SessionStart"][0]["matcher"], MATCHER);
        assert!(
            value["hooks"]["SessionStart"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .ends_with("synapse session")
        );
    }

    #[test]
    fn user_settings_and_other_hooks_are_carried_through_untouched() {
        let (_directory, settings, binary) = setup();
        crate::files::write(
            &settings,
            &serde_json::to_string_pretty(&json!({
                "model": "opus",
                "hooks": {
                    "SessionStart": [
                        {"matcher": "startup", "hooks": [{"type": "command", "command": "mine.sh"}]}
                    ],
                    "Stop": [{"hooks": [{"type": "command", "command": "done.sh"}]}]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        apply(&settings, &binary).unwrap();

        let value = read(&settings).unwrap();
        assert_eq!(value["model"], "opus");
        assert_eq!(value["hooks"]["Stop"][0]["hooks"][0]["command"], "done.sh");
        let groups = value["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "the user's own hook must survive");
        assert_eq!(groups[0]["hooks"][0]["command"], "mine.sh");
    }

    #[test]
    fn applying_twice_leaves_one_hook_rather_than_two() {
        let (_directory, settings, binary) = setup();

        apply(&settings, &binary).unwrap();
        apply(&settings, &binary).unwrap();

        let value = read(&settings).unwrap();
        assert_eq!(value["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_hook_left_by_a_binary_that_moved_is_replaced_rather_than_duplicated() {
        let (_directory, settings, binary) = setup();
        crate::files::write(
            &settings,
            &serde_json::to_string_pretty(&json!({
                "hooks": {"SessionStart": [
                    {"matcher": MATCHER, "hooks": [
                        {"type": "command", "command": "/old/place/synapse session"}
                    ]}
                ]}
            }))
            .unwrap(),
        )
        .unwrap();

        apply(&settings, &binary).unwrap();

        let value = read(&settings).unwrap();
        let groups = value["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert!(
            groups[0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .starts_with(binary.parent().unwrap().to_str().unwrap())
        );
    }

    #[test]
    fn a_status_line_somebody_else_configured_is_never_replaced() {
        let (_directory, settings, binary) = setup();
        crate::files::write(
            &settings,
            &serde_json::to_string_pretty(&json!({
                "statusLine": {"type": "command", "command": "my-prompt.sh"}
            }))
            .unwrap(),
        )
        .unwrap();

        let state = apply(&settings, &binary).unwrap();

        assert!(state.notice, "the notice still installs");
        assert!(!state.statusline);
        assert!(state.borrowed, "the caller has to be able to say why");
        assert_eq!(
            read(&settings).unwrap()["statusLine"]["command"],
            "my-prompt.sh"
        );
    }

    #[test]
    fn removal_leaves_everything_synapse_did_not_write() {
        let (_directory, settings, binary) = setup();
        crate::files::write(
            &settings,
            &serde_json::to_string_pretty(&json!({
                "model": "opus",
                "statusLine": {"type": "command", "command": "my-prompt.sh"},
                "hooks": {"SessionStart": [
                    {"matcher": "startup", "hooks": [{"type": "command", "command": "mine.sh"}]}
                ]}
            }))
            .unwrap(),
        )
        .unwrap();
        apply(&settings, &binary).unwrap();

        remove(&settings, &binary).unwrap();

        let value = read(&settings).unwrap();
        assert_eq!(value["model"], "opus");
        assert_eq!(
            value["statusLine"]["command"], "my-prompt.sh",
            "a status line Synapse never claimed must survive removal"
        );
        let groups = value["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["hooks"][0]["command"], "mine.sh");
    }

    #[test]
    fn removing_the_last_hook_takes_the_empty_containers_with_it() {
        let (_directory, settings, binary) = setup();
        apply(&settings, &binary).unwrap();

        remove(&settings, &binary).unwrap();

        let value = read(&settings).unwrap();
        assert!(
            value.get("hooks").is_none(),
            "no empty scaffolding left behind"
        );
        assert!(value.get("statusLine").is_none());
        assert_eq!(state(&settings, &binary), State::default());
    }

    #[test]
    fn removing_when_nothing_was_installed_is_not_an_error() {
        let (_directory, settings, binary) = setup();
        assert!(remove(&settings, &binary).is_ok());
    }

    #[test]
    fn a_binary_path_with_a_space_is_quoted_for_the_shell() {
        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("settings.json");
        let binary = directory.path().join("Synapse App").join("synapse");

        let state = apply(&settings, &binary).unwrap();

        assert!(state.notice, "a quoted command must still be recognised");
        let value = read(&settings).unwrap();
        let command = value["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(command.starts_with('\''), "got {command}");
        assert!(command.ends_with("' session"), "got {command}");
    }

    #[test]
    fn an_empty_or_missing_settings_file_is_a_starting_point_not_an_error() {
        let (_directory, settings, binary) = setup();
        assert_eq!(state(&settings, &binary), State::default());
        std::fs::write(&settings, "   \n").unwrap();
        assert!(apply(&settings, &binary).unwrap().notice);
    }

    #[test]
    fn malformed_settings_are_reported_rather_than_overwritten() {
        let (_directory, settings, binary) = setup();
        std::fs::write(&settings, "{ not json").unwrap();

        assert!(apply(&settings, &binary).is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ not json");
    }
}
