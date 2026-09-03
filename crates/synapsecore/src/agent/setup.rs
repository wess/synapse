use crate::agent::{Agent, Detection, Kind};
use crate::files;
use anyhow::{Context, Result};
use std::path::Path;

/// Whether a registration that is already there is left alone or written
/// again.
///
/// Detection can only see whether *a* registration points at this binary with
/// the right arguments. It cannot see the rest of what a descriptor says, so a
/// release that changes `connect.add` — a flag added, a name changed — reaches
/// nobody who is already connected under [`Apply::IfNeeded`]. That is the whole
/// reason [`Apply::Force`] exists, and why refreshing a connection is a
/// deliberate act rather than something `connect` quietly does every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    /// Register the server only when it is not already registered correctly.
    IfNeeded,
    /// Register it again whatever detection found, running `connect.remove`
    /// first so the tool's own CLI is never asked to add a name it already has.
    Force,
}

/// Connect a tool, leaving an existing registration alone.
pub fn setup(agent: &Agent, detection: &Detection, server: &Path, soul: &Path) -> Result<()> {
    setupwith(agent, detection, server, soul, Apply::IfNeeded)
}

/// Connect a tool, writing the registration again even when one is already
/// there — so a descriptor that moved in a release reaches a machine that is
/// already set up.
pub fn reapply(agent: &Agent, detection: &Detection, server: &Path, soul: &Path) -> Result<()> {
    setupwith(agent, detection, server, soul, Apply::Force)
}

pub fn setupwith(
    agent: &Agent,
    detection: &Detection,
    server: &Path,
    soul: &Path,
    apply: Apply,
) -> Result<()> {
    let integration = files::Snapshot::capture(&agent.integration)?;
    let instructions = files::Snapshot::capture(&agent.instructions)?;
    let settings = files::Snapshot::capture(&agent.settings)?;
    let shared = files::Snapshot::capture(soul)?;
    if !detection.configured || apply == Apply::Force {
        integration.backup()?;
    }
    crate::instructions::ensure(soul)?;

    if let Err(error) = runsetup(agent, detection, server, soul, apply) {
        if let Err(rollback) = settings
            .restore()
            .and_then(|_| instructions.restore())
            .and_then(|_| integration.restore())
            .and_then(|_| shared.restore())
        {
            return Err(error).context(format!("setup rollback also failed: {rollback:#}"));
        }
        return Err(error);
    }
    Ok(())
}

fn runsetup(
    agent: &Agent,
    detection: &Detection,
    server: &Path,
    soul: &Path,
    apply: Apply,
) -> Result<()> {
    let executable = detection
        .executable
        .as_deref()
        .context("the tool is not installed or is not on PATH")?;

    let forced = apply == Apply::Force;
    if !detection.configured || forced {
        // A registration pointing at a binary that has moved has to come out
        // before the replacement goes in, or the tool's own CLI is asked to
        // write a name it already has. A forced re-apply is the same problem
        // whatever the descriptor says about `replace`: the entry is known to
        // be there, and it is being written again on purpose.
        if (agent.connect.replace || forced)
            && detection.registered
            && !agent.connect.remove.is_empty()
        {
            let output = crate::agent::command(executable)
                .args(argv(&agent.connect.remove, agent, server))
                .output();
            // A descriptor that says `replace` is describing a tool that cannot
            // overwrite its own entry, so a removal that fails there has to
            // stop the run. A forced re-apply makes no such claim: most CLIs
            // overwrite happily, and refusing to re-apply because the tool had
            // nothing to remove would break the one path that exists to fix a
            // connection.
            let failure = match &output {
                Ok(output) if output.status.success() => None,
                Ok(output) => Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
                Err(error) => Some(error.to_string()),
            };
            if let Some(failure) = failure
                && agent.connect.replace
            {
                anyhow::bail!(
                    "could not replace the stale {} connection: {failure}",
                    agent.name
                );
            }
        }
        anyhow::ensure!(
            !agent.connect.add.is_empty(),
            "the `{}` tool does not say how to connect it; give its descriptor a `connect.add`",
            agent.slug
        );
        // The tool's own CLI, writing the tool's own settings. Synapse never
        // edits a connection into somebody else's config file directly.
        let output = crate::agent::command(executable)
            .args(argv(&agent.connect.add, agent, server))
            .output()
            .with_context(|| format!("could not run the {} setup command", agent.name))?;
        anyhow::ensure!(
            output.status.success(),
            "{} setup failed: {}",
            agent.name,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    writeinstructions(&agent.instructions, soul)?;

    // Only Claude Code can show the connection before the model has written
    // anything. Everywhere else the notice rides on the guidance pointer.
    if agent.kind == Kind::Claude {
        crate::agent::hooks::apply(&agent.settings, server)
            .context("could not add the Synapse session notice to Claude Code")?;
    }
    Ok(())
}

pub fn writeinstructions(path: &Path, soul: &Path) -> Result<()> {
    crate::agent::guidance::writepointer(path, soul, false)
}

/// Fill a descriptor's argv template. `{server}` is this binary and `{package}`
/// is what a package-style connection installs; a token holding neither is
/// passed through as written.
pub(super) fn argv(template: &[String], agent: &crate::agent::Agent, server: &Path) -> Vec<String> {
    let server = server.display().to_string();
    let package = agent.package();
    template
        .iter()
        .map(|token| {
            token
                .replace("{server}", &server)
                .replace("{package}", &package)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn appends_without_replacing_user_content() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("agents.md");
        let soul = directory.path().join("SOUL.md");
        fs::write(&file, "# My rules\n\nKeep this.").unwrap();
        writeinstructions(&file, &soul).unwrap();
        let merged = fs::read_to_string(file).unwrap();
        assert!(merged.starts_with("# My rules\n\nKeep this."));
        assert!(merged.contains(soul.to_str().unwrap()));
        assert_eq!(merged.matches("<!-- synapse:begin -->").count(), 1);
    }

    #[test]
    fn instruction_updates_leave_a_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agents.md");
        let soul = directory.path().join("SOUL.md");
        fs::write(&path, "# My rules\n").unwrap();

        writeinstructions(&path, &soul).unwrap();

        assert_eq!(
            fs::read_to_string(directory.path().join("agents.md.synapsebackup")).unwrap(),
            "# My rules\n"
        );
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains("<!-- synapse:begin -->")
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_external_setup_restores_integration_store() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("config.toml");
        let instructions = directory.path().join("agents.md");
        let executable = directory.path().join("fake");
        fs::write(&settings, "[user]\nname = \"kept\"\n").unwrap();
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf 'changed = true\\n' > '{}'\nexit 1\n",
                settings.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let agent = testagent(settings.clone(), instructions);
        let detection = Detection {
            executable: Some(executable),
            version: None,
            registered: false,
            configured: false,
            hooks: crate::agent::HookState::default(),
        };

        assert!(
            setup(
                &agent,
                &detection,
                Path::new("/synapse"),
                &directory.path().join("SOUL.md")
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(&settings).unwrap(),
            "[user]\nname = \"kept\"\n"
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("config.toml.synapsebackup")).unwrap(),
            "[user]\nname = \"kept\"\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn instruction_failure_rolls_back_successful_external_setup() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("config.toml");
        let blocker = directory.path().join("blocked");
        let instructions = blocker.join("agents.md");
        let executable = directory.path().join("fake");
        fs::write(&settings, "enabled = false\n").unwrap();
        fs::write(&blocker, "not a directory").unwrap();
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf 'enabled = true\\n' > '{}'\nexit 0\n",
                settings.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let agent = testagent(settings.clone(), instructions);
        let detection = Detection {
            executable: Some(executable),
            version: None,
            registered: false,
            configured: false,
            hooks: crate::agent::HookState::default(),
        };

        assert!(
            setup(
                &agent,
                &detection,
                Path::new("/synapse"),
                &directory.path().join("SOUL.md")
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(settings).unwrap(), "enabled = false\n");
    }

    #[cfg(unix)]
    #[test]
    fn stale_claude_connection_is_removed_before_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let integration = directory.path().join("claude.json");
        let instructions = directory.path().join("claude.md");
        let executable = directory.path().join("fake");
        let log = directory.path().join("commands");
        fs::write(&integration, "{\"mcpServers\":{\"synapse\":{}}}").unwrap();
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let mut agent = described("claude", integration, instructions);
        agent.settings = directory.path().join("settings.json");
        let detection = Detection {
            executable: Some(executable),
            version: None,
            registered: true,
            configured: false,
            hooks: crate::agent::HookState::default(),
        };

        setup(
            &agent,
            &detection,
            Path::new("/synapse"),
            &directory.path().join("SOUL.md"),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(log).unwrap(),
            "mcp remove --scope user synapse\nmcp add --scope user synapse -- /synapse mcp\n"
        );
        // Claude Code is the one tool that can print the notice at startup, so
        // connecting it also installs the session hook.
        let settings = fs::read_to_string(&agent.settings).unwrap();
        assert!(settings.contains("SessionStart"), "got {settings}");
        assert!(settings.contains("/synapse session"), "got {settings}");
    }

    #[cfg(unix)]
    #[test]
    fn connecting_pi_installs_the_package_and_points_it_at_shared_guidance() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("settings.json");
        let instructions = directory.path().join("APPEND_SYSTEM.md");
        let executable = directory.path().join("fake");
        let log = directory.path().join("commands");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let agent = described("pi", settings, instructions.clone());
        let detection = Detection {
            executable: Some(executable),
            version: None,
            registered: false,
            configured: false,
            hooks: crate::agent::HookState::default(),
        };
        let soul = directory.path().join("SOUL.md");

        setup(&agent, &detection, Path::new("/synapse"), &soul).unwrap();

        // pi's own package manager, not an edit to its settings, and no MCP
        // server to register: the package is what carries the tools.
        assert_eq!(fs::read_to_string(log).unwrap(), "install npm:synapse-pi\n");
        // And the same managed pointer every other connected tool gets, in the
        // file pi appends to every system prompt.
        let appended = fs::read_to_string(instructions).unwrap();
        assert!(
            appended.contains("<!-- synapse:begin -->"),
            "got {appended}"
        );
        assert!(appended.contains(soul.to_str().unwrap()), "got {appended}");
    }

    /// The desktop app is started by the Finder with a four-entry PATH, so what
    /// detection finds is usually a version manager's shim — and a shim execs
    /// its manager by name. Setup has to hand the child somewhere to find it, or
    /// connecting fails with `exec: asdf: not found` on a machine where the tool
    /// works perfectly from a terminal.
    #[cfg(unix)]
    #[test]
    fn the_tool_is_run_with_a_path_wide_enough_for_a_version_manager_shim() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("fake");
        let log = directory.path().join("path");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s' \"$PATH\" > '{}'\nexit 0\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let agent = testagent(
            directory.path().join("config.toml"),
            directory.path().join("agents.md"),
        );
        let detection = Detection {
            executable: Some(executable),
            version: None,
            registered: false,
            configured: false,
            hooks: crate::agent::HookState::default(),
        };

        setup(
            &agent,
            &detection,
            Path::new("/synapse"),
            &directory.path().join("SOUL.md"),
        )
        .unwrap();

        let path = fs::read_to_string(log).unwrap();
        assert!(path.contains(".asdf/shims"), "got {path}");
        assert!(path.contains(".asdf/bin"), "got {path}");
        assert!(path.contains("/opt/homebrew/bin"), "got {path}");
    }

    /// A real descriptor with its paths pointed at a tempdir, so these tests
    /// exercise the connect argv each tool actually declares.
    fn testagent(integration: PathBuf, instructions: PathBuf) -> Agent {
        described("codex", integration, instructions)
    }

    fn described(slug: &str, integration: PathBuf, instructions: PathBuf) -> Agent {
        let mut agent = crate::agent::tool::resolve(Path::new("/users/test"), None, slug)
            .unwrap()
            .unwrap();
        agent.instructions = instructions;
        agent.settings = integration.clone();
        agent.integration = integration;
        agent.skills = PathBuf::new();
        agent
    }
}
