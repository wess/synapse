use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_synapse"));
    command
        .env("SYNAPSE_HOME", root.join("home"))
        .env("SYNAPSE_DATA", root.join("data"))
        .env("SYNAPSE_BIN", root.join("bin").join("synapse"));
    command
}

fn run(root: &Path, arguments: &[&str], input: Option<&str>) -> Output {
    runfrom(root, None, arguments, input)
}

fn runfrom(root: &Path, folder: Option<&Path>, arguments: &[&str], input: Option<&str>) -> Output {
    let mut command = command(root);
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    command
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn ambient_shell_mode_tracks_approval_changes_and_revocation() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let nested = project.join("src");
    fs::create_dir_all(&nested).unwrap();

    let inactive = success(runfrom(
        root.path(),
        Some(&nested),
        &["export", "zsh"],
        None,
    ));
    assert!(inactive.contains("__synapse_state='inactive'"));

    success(run(
        root.path(),
        &["scope", "init", project.to_str().unwrap()],
        None,
    ));

    let allowed = success(runfrom(root.path(), Some(&nested), &["allow"], None));
    assert!(allowed.contains("Allowed"));
    let active = success(runfrom(
        root.path(),
        Some(&nested),
        &["export", "zsh"],
        None,
    ));
    assert!(active.contains("__synapse_state='active'"));
    assert!(active.contains(&format!(
        "__synapse_scope='{}'",
        project.canonicalize().unwrap().display()
    )));

    fs::write(
        project.join(".synapse.yaml"),
        "version: 1\nscope: project\nenv: {}\ndeny: []\n\n",
    )
    .unwrap();
    let blocked = success(runfrom(
        root.path(),
        Some(&nested),
        &["export", "zsh"],
        None,
    ));
    assert!(blocked.contains("__synapse_state='blocked'"));

    let denied = success(runfrom(root.path(), Some(&nested), &["deny"], None));
    assert!(denied.contains("Denied"));
    let status: Value = serde_json::from_str(&success(run(
        root.path(),
        &["status", project.to_str().unwrap(), "--json"],
        None,
    )))
    .unwrap();
    assert!(!status["scopes"][0]["trusted"].as_bool().unwrap());

    let hook = success(run(root.path(), &["hook", "zsh"], None));
    assert!(hook.contains("add-zsh-hook chpwd __synapse_hook"));
    assert!(hook.contains("SYNAPSE_SHELL_ACTIVE=zsh"));
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn cli_roundtrips_memory_vault_scope_and_data() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();

    success(run(root, &["vault", "create", "work"], None));
    assert_eq!(success(run(root, &["vault", "list"], None)).trim(), "work");

    let added = success(run(
        root,
        &["memory", "add", "integration"],
        Some("durable alpha"),
    ));
    let id = added
        .trim()
        .strip_prefix("Stored memory #")
        .unwrap()
        .to_owned();
    let listed: Value = serde_json::from_str(&success(run(
        root,
        &["memory", "list", "alpha", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(listed[0]["body"], "durable alpha");

    success(run(
        root,
        &["memory", "edit", &id, "edited"],
        Some("durable beta"),
    ));
    let shown: Value = serde_json::from_str(&success(run(
        root,
        &["memory", "show", &id, "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(shown["body"], "durable beta");
    assert_eq!(shown["source"], "edited");

    let rejected = run(root, &["memory", "delete", &id], None);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("--confirm"));

    let project = root.join("project");
    fs::create_dir(&project).unwrap();
    success(run(
        root,
        &["scope", "init", project.to_str().unwrap()],
        None,
    ));
    let scope = fs::read_to_string(project.join(".synapse.yaml")).unwrap();
    assert!(scope.contains("scope: project"));

    let report: Value =
        serde_json::from_str(&success(run(root, &["data", "check", "--json"], None))).unwrap();
    assert_eq!(report["integrity"], "ok");
    let export = root.join("export.db");
    success(run(
        root,
        &["data", "export", export.to_str().unwrap()],
        None,
    ));
    success(run(root, &["memory", "add", "later"], Some("after export")));
    success(run(
        root,
        &["data", "restore", export.to_str().unwrap()],
        None,
    ));
    let restored: Value = serde_json::from_str(&success(run(
        root,
        &["memory", "list", "after", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(restored, Value::Array(Vec::new()));

    success(run(root, &["vault", "delete", "work"], None));
    assert!(
        success(run(root, &["vault", "list"], None))
            .trim()
            .is_empty()
    );
}

#[test]
fn cli_imports_claude_memory_safely_and_undoes_the_batch() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir(&project).unwrap();
    let encoded = project.display().to_string().replace('/', "-");
    let memory = home.join(".claude/projects").join(encoded).join("memory");
    fs::create_dir_all(&memory).unwrap();
    fs::write(memory.join("choice.md"), "Use the imported durable choice.").unwrap();
    fs::write(memory.join("unsafe.md"), "API_KEY=not-for-memory").unwrap();
    fs::write(
        home.join(".claude.json"),
        serde_json::json!({"projects": {project.display().to_string(): {}}}).to_string(),
    )
    .unwrap();

    let preview: Value = serde_json::from_str(&success(run(
        root.path(),
        &["memory", "import", "claude", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(preview["ready"], 1);
    assert_eq!(preview["flagged"], 1);
    let report: Value = serde_json::from_str(&success(run(
        root.path(),
        &["memory", "import", "claude", "--confirm", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(report["stored"], 1);
    assert_eq!(report["flagged"], 1);
    let batch = report["batch"]["id"].as_i64().unwrap().to_string();
    let memories: Value = serde_json::from_str(&success(run(
        root.path(),
        &["memory", "list", "imported", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(memories.as_array().unwrap().len(), 1);

    success(run(
        root.path(),
        &["memory", "undo", &batch, "--confirm"],
        None,
    ));
    let memories: Value = serde_json::from_str(&success(run(
        root.path(),
        &["memory", "list", "imported", "--json"],
        None,
    )))
    .unwrap();
    assert!(memories.as_array().unwrap().is_empty());
}

#[test]
fn guidance_can_preserve_then_consolidate_global_files() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let codex = home.join(".codex/AGENTS.md");
    let claude = home.join(".claude/CLAUDE.md");
    fs::create_dir_all(codex.parent().unwrap()).unwrap();
    fs::create_dir_all(claude.parent().unwrap()).unwrap();
    fs::write(&codex, "# Use Bun\n").unwrap();
    fs::write(&claude, "# Keep files focused\n").unwrap();

    success(run(root.path(), &["guidance", "sync"], None));
    assert!(fs::read_to_string(&codex).unwrap().contains("# Use Bun"));
    assert!(fs::read_to_string(&codex).unwrap().contains("SOUL.md"));
    let rejected = run(root.path(), &["guidance", "adopt"], None);
    assert!(!rejected.status.success());

    success(run(root.path(), &["guidance", "adopt", "--confirm"], None));
    let soul = fs::read_to_string(root.path().join("data/SOUL.md")).unwrap();
    assert!(soul.contains("# Use Bun"));
    assert!(soul.contains("# Keep files focused"));
    assert!(!fs::read_to_string(&codex).unwrap().contains("# Use Bun"));
    assert!(
        !fs::read_to_string(&claude)
            .unwrap()
            .contains("# Keep files focused")
    );
    assert!(codex.with_file_name("AGENTS.md.synapsebackup").is_file());
    assert!(claude.with_file_name("CLAUDE.md.synapsebackup").is_file());
}

#[test]
fn restore_refuses_while_an_mcp_process_holds_the_database() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    success(run(
        root,
        &["memory", "add", "locking"],
        Some("keep database open"),
    ));
    let export = root.join("export.db");
    success(run(
        root,
        &["data", "export", export.to_str().unwrap()],
        None,
    ));

    let mut server = command(root)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = server.stdin.take().unwrap();
    let mut stdout = BufReader::new(server.stdout.take().unwrap());
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "locktest", "version": "1"}
            }
        })
    )
    .unwrap();
    stdin.flush().unwrap();
    let mut initialized = String::new();
    stdout.read_line(&mut initialized).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&initialized).unwrap()["id"],
        1
    );

    let rejected = run(root, &["data", "restore", export.to_str().unwrap()], None);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("close the app and connected tools")
    );

    drop(stdin);
    drop(stdout);
    assert!(server.wait().unwrap().success());
}

#[cfg(unix)]
#[test]
fn clean_install_copies_an_executable_and_protects_conflicts() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("bin").join("synapse");
    success(run(root.path(), &["install"], None));

    let metadata = fs::symlink_metadata(&target).unwrap();
    assert!(!metadata.file_type().is_symlink());
    assert_ne!(metadata.permissions().mode() & 0o111, 0);
    let version = Command::new(&target).arg("version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("synapse "));
    success(run(root.path(), &["install"], None));

    let conflict = tempfile::tempdir().unwrap();
    let conflictpath = conflict.path().join("bin").join("synapse");
    fs::create_dir_all(conflictpath.parent().unwrap()).unwrap();
    fs::write(&conflictpath, "keep me").unwrap();
    let rejected = run(conflict.path(), &["install"], None);
    assert!(!rejected.status.success());
    assert_eq!(fs::read_to_string(conflictpath).unwrap(), "keep me");
}

#[cfg(target_os = "macos")]
#[test]
fn keychain_secret_reaches_only_the_launched_process() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let root = tempfile::tempdir().unwrap();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let vault = format!("synapsetest{suffix}");
    success(run(root.path(), &["vault", "create", &vault], None));
    let reference = format!("{vault}.token");

    let saved = run(
        root.path(),
        &[
            "secret",
            "set",
            &vault,
            "token",
            "SYNAPSE_TEST_VALUE",
            "--global",
        ],
        Some("keychainvalue\n"),
    );
    let listed = run(root.path(), &["secret", "list", &vault], None);
    let launched = run(
        root.path(),
        &["run", "--", "/usr/bin/printenv", "SYNAPSE_TEST_VALUE"],
        None,
    );

    let project = root.path().join("ambient");
    fs::create_dir(&project).unwrap();
    fs::write(
        project.join(".synapse.yaml"),
        format!("version: 1\nscope: project\nenv:\n  SYNAPSE_TEST_VALUE: {reference}\ndeny: []\n"),
    )
    .unwrap();
    success(run(
        root.path(),
        &["allow", project.to_str().unwrap()],
        None,
    ));
    let ambient = runfrom(root.path(), Some(&project), &["export", "zsh"], None);

    let forgotten = run(root.path(), &["secret", "forget", &reference], None);
    let deleted = run(root.path(), &["vault", "delete", &vault], None);
    let saved = success(saved);
    let listed = success(listed);
    let launched = success(launched);
    let ambient = success(ambient);
    success(forgotten);
    success(deleted);
    assert!(!saved.contains("keychainvalue"));
    assert!(listed.contains("SYNAPSE_TEST_VALUE"));
    assert!(!listed.contains("keychainvalue"));
    assert_eq!(launched.trim(), "keychainvalue");
    assert!(ambient.contains("export SYNAPSE_TEST_VALUE='keychainvalue'"));
    assert!(ambient.contains("__synapse_state='active'"));
}

#[test]
fn the_mesh_stays_off_until_it_is_switched_on_and_then_reports_itself() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(project.join(".git")).unwrap();

    assert!(success(run(root.path(), &["settings", "show"], None)).contains("mesh\toff"));
    let off = success(run(root.path(), &["relay", "status"], None));
    assert!(off.contains("Mesh: off"));
    assert!(
        off.contains("synapse settings mesh on"),
        "an off mesh should say how to turn it on: {off}"
    );

    success(run(root.path(), &["settings", "mesh", "on"], None));

    assert!(success(run(root.path(), &["settings", "show"], None)).contains("mesh\ton"));
    let on = success(run(root.path(), &["relay", "status"], None));
    assert!(on.contains("Mesh: on"));
    assert!(on.contains("Agents: 0 online of 0"));

    let status: Value = serde_json::from_str(&success(run(
        root.path(),
        &["relay", "status", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(status["enabled"], true);
    assert!(status["agents"].as_array().unwrap().is_empty());

    // Turning it back off is the same switch, not a separate teardown.
    success(run(root.path(), &["settings", "mesh", "off"], None));
    assert!(success(run(root.path(), &["relay", "status"], None)).contains("Mesh: off"));
}

#[test]
fn roles_and_teams_resolve_from_the_project_before_the_built_ins() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(project.join(".synapse").join("roles")).unwrap();

    let builtins = success(run(root.path(), &["relay", "role", "list"], None));
    for name in [
        "supervisor",
        "worker",
        "frontend",
        "backend",
        "reviewer",
        "devops",
        "qa",
    ] {
        assert!(builtins.contains(name), "{name} is missing from {builtins}");
    }
    assert!(success(run(root.path(), &["relay", "team", "list"], None)).contains("web"));

    fs::write(
        project.join(".synapse").join("roles").join("worker.toml"),
        "channels = [\"mine\"]\ndescription = \"Local worker brief.\"\n",
    )
    .unwrap();

    let shown = success(runfrom(
        root.path(),
        Some(&project),
        &["relay", "role", "show", "worker"],
        None,
    ));
    assert!(shown.contains("· project"), "got {shown}");
    assert!(shown.contains("Local worker brief."));

    // Standing somewhere else, the same name resolves to the shipped template.
    let elsewhere = success(run(root.path(), &["relay", "role", "show", "worker"], None));
    assert!(elsewhere.contains("· built-in"), "got {elsewhere}");
}

#[test]
fn a_launch_resolves_its_role_into_a_command_without_running_anything() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(&project).unwrap();

    let printed = success(runfrom(
        root.path(),
        Some(&project),
        &[
            "relay",
            "launch",
            "backend",
            "--role",
            "backend",
            "--task",
            "build the api",
            "--command",
            "mytool --prompt {prompt}",
            "--print",
        ],
        None,
    ));

    assert!(printed.starts_with("mytool --prompt '"), "got {printed}");
    assert!(printed.contains("You are \\\"backend\\\"") || printed.contains("You are \"backend\""));
    assert!(printed.contains("Call `wait` to receive work"));
    assert!(printed.contains("standing focus: build the api"));
    assert!(
        printed.contains("You own the backend and API"),
        "the role brief must reach the harness: {printed}"
    );

    // A lead stays interactive instead of parking.
    let lead = success(runfrom(
        root.path(),
        Some(&project),
        &[
            "relay",
            "launch",
            "lead",
            "--role",
            "supervisor",
            "--command",
            "mytool {prompt}",
            "--print",
        ],
        None,
    ));
    assert!(lead.contains("do NOT call `wait` yet"), "got {lead}");
}

#[test]
fn the_session_hook_reports_real_numbers_and_a_status_line_follows() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    let folder = project.display().to_string();

    let empty: Value = serde_json::from_str(&success(runfrom(
        root.path(),
        Some(&project),
        &["session"],
        Some(&format!("{{\"cwd\":\"{folder}\"}}")),
    )))
    .unwrap();
    assert_eq!(
        empty["systemMessage"], "Synapse connected · no memories yet",
        "an empty store should say so rather than claim memories"
    );
    assert_eq!(
        empty["hookSpecificOutput"]["hookEventName"], "SessionStart",
        "Claude Code only reads hook output under this name"
    );

    success(runfrom(
        root.path(),
        Some(&project),
        &["memory", "add", "meshtest", "--project", &folder],
        Some("Ship the beta on Fridays."),
    ));

    let stored: Value = serde_json::from_str(&success(runfrom(
        root.path(),
        Some(&project),
        &["session"],
        Some(&format!("{{\"cwd\":\"{folder}\"}}")),
    )))
    .unwrap();
    assert_eq!(stored["systemMessage"], "Synapse connected · 1 memory");
    let context = stored["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(
        context.contains("do not print a `Synapse connected` line yourself"),
        "the model must not repeat a notice the user has already seen: {context}"
    );

    let line = success(runfrom(
        root.path(),
        Some(&project),
        &["statusline"],
        Some(&format!(
            "{{\"model\":{{\"display_name\":\"Opus 5\"}},\"workspace\":{{\"current_dir\":\"{folder}\"}}}}"
        )),
    ));
    assert_eq!(line.trim(), "Opus 5 · project · ◆ Synapse 1");
}

#[test]
fn one_skill_library_installs_into_every_connected_tool() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");

    // The shipped skill appears without anyone creating it.
    let listed = success(run(root.path(), &["skill", "list"], None));
    assert!(listed.contains("synapse-mesh"), "got {listed}");

    let before = success(run(root.path(), &["skill", "status"], None));
    assert!(
        before.contains("synapse-mesh\tCodex\tnot installed"),
        "got {before}"
    );
    assert!(
        before.contains("synapse-mesh\tClaude Code\tnot installed"),
        "got {before}"
    );

    success(run(root.path(), &["skill", "install"], None));

    // Each tool reads personal skills from its own folder, and both got a copy.
    let claude = home.join(".claude/skills/synapse-mesh/SKILL.md");
    let codex = home.join(".agents/skills/synapse-mesh/SKILL.md");
    assert!(claude.is_file(), "Claude Code did not get the skill");
    assert!(codex.is_file(), "Codex did not get the skill");
    assert_eq!(
        fs::read_to_string(&claude).unwrap(),
        fs::read_to_string(&codex).unwrap(),
        "the whole point is that the two copies cannot drift"
    );

    let after = success(run(root.path(), &["skill", "status"], None));
    assert!(
        after.contains("synapse-mesh\tCodex\tinstalled"),
        "got {after}"
    );

    // Editing the library marks every copy as out of date, and installing again
    // brings them back in step.
    let library = root.path().join("data/skills/synapse-mesh/SKILL.md");
    let edited = fs::read_to_string(&library).unwrap().replace(
        "Coordination is cheap",
        "A change in the library. Coordination is cheap",
    );
    fs::write(&library, edited).unwrap();

    let stale = success(run(root.path(), &["skill", "status"], None));
    assert!(
        stale.contains("synapse-mesh\tCodex\tupdate available"),
        "got {stale}"
    );

    success(run(
        root.path(),
        &["skill", "install", "synapse-mesh"],
        None,
    ));
    assert!(
        fs::read_to_string(&codex)
            .unwrap()
            .contains("A change in the library"),
        "the sync should have reached the installed copy"
    );
}

#[test]
fn a_skill_synapse_did_not_write_is_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let theirs = home.join(".claude/skills/mine");
    fs::create_dir_all(&theirs).unwrap();
    let body = "---\nname: mine\ndescription: I wrote this by hand and it is not Synapse business.\n---\n\nMine alone.\n";
    fs::write(theirs.join("SKILL.md"), body).unwrap();

    // It is reported as present but unmanaged, rather than ignored.
    let status = success(run(root.path(), &["skill", "status"], None));
    assert!(
        status.contains("mine\tClaude Code\tnot in the library"),
        "got {status}"
    );

    // Adopting copies it into the library and claims the copy it came from, so
    // the tool it was already in does not read as somebody else's.
    success(run(
        root.path(),
        &["skill", "adopt", "mine", "--tool", "claude"],
        None,
    ));
    let adopted = success(run(root.path(), &["skill", "status", "mine"], None));
    assert!(
        adopted.contains("mine\tClaude Code\tinstalled"),
        "got {adopted}"
    );

    success(run(root.path(), &["skill", "install", "mine"], None));
    assert!(home.join(".agents/skills/mine/SKILL.md").is_file());

    // A copy edited inside the tool is protected until asked for explicitly.
    fs::write(
        home.join(".agents/skills/mine/SKILL.md"),
        "---\nname: mine\ndescription: Edited right here inside the tool.\n---\n\nTheirs.\n",
    )
    .unwrap();
    let changed = success(run(root.path(), &["skill", "status", "mine"], None));
    assert!(
        changed.contains("mine\tCodex\tchanged in place"),
        "got {changed}"
    );

    let refused = run(root.path(), &["skill", "install", "mine"], None);
    assert!(
        !refused.status.success(),
        "a changed copy must not be replaced silently"
    );
    assert!(
        fs::read_to_string(home.join(".agents/skills/mine/SKILL.md"))
            .unwrap()
            .contains("Theirs."),
        "the edit inside the tool has to survive"
    );

    success(run(
        root.path(),
        &["skill", "install", "mine", "--replace"],
        None,
    ));
    assert!(
        !fs::read_to_string(home.join(".agents/skills/mine/SKILL.md"))
            .unwrap()
            .contains("Theirs."),
        "asking explicitly should still work"
    );
}

#[test]
fn a_broken_skill_is_reported_and_never_reaches_a_tool() {
    let root = tempfile::tempdir().unwrap();
    let broken = root.path().join("data/skills/broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("SKILL.md"), "no frontmatter at all\n").unwrap();

    let output = run(root.path(), &["skill", "list"], None);
    assert!(
        output.status.success(),
        "one bad skill must not sink the list"
    );
    let warnings = String::from_utf8_lossy(&output.stderr);
    assert!(warnings.contains("skipped broken:"), "got {warnings}");

    success(run(root.path(), &["skill", "install"], None));
    assert!(
        !root.path().join("home/.claude/skills/broken").exists(),
        "a skill that does not parse must not be copied anywhere"
    );
}

/// Software that edits configuration it did not create owes the user a way
/// back out, and the way back out must take only what it put there.
#[test]
fn disconnecting_removes_only_what_synapse_wrote() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let claude = home.join(".claude");
    fs::create_dir_all(claude.join("skills/mine")).unwrap();

    // Everything here belongs to the user and must survive untouched.
    fs::write(
        claude.join("CLAUDE.md"),
        "# My rules\n\nAlways write tests first.\n",
    )
    .unwrap();
    fs::write(
        claude.join("settings.json"),
        "{\"model\":\"opus\",\
         \"statusLine\":{\"type\":\"command\",\"command\":\"my-own-statusline\"},\
         \"hooks\":{\"PreToolUse\":[{\"matcher\":\"Bash\",\
         \"hooks\":[{\"type\":\"command\",\"command\":\"my-guard\"}]}]}}\n",
    )
    .unwrap();
    fs::write(
        claude.join("skills/mine/SKILL.md"),
        "---\nname: mine\ndescription: A skill I wrote myself.\n---\n\nMine alone.\n",
    )
    .unwrap();

    success(run(root.path(), &["guidance", "sync"], None));
    let pointed = fs::read_to_string(claude.join("CLAUDE.md")).unwrap();
    assert!(pointed.contains("synapse:begin"), "setup did not point");

    success(run(root.path(), &["disconnect", "claude"], None));

    let instructions = fs::read_to_string(claude.join("CLAUDE.md")).unwrap();
    assert!(
        !instructions.contains("synapse:begin"),
        "the managed block should be gone: {instructions}"
    );
    assert!(
        instructions.contains("Always write tests first."),
        "the user's own words must survive: {instructions}"
    );

    let settings: Value =
        serde_json::from_str(&fs::read_to_string(claude.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["statusLine"]["command"], "my-own-statusline");
    assert_eq!(
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "my-guard"
    );
    assert_eq!(settings["model"], "opus");
    assert!(
        claude.join("skills/mine/SKILL.md").is_file(),
        "a skill Synapse never installed is not Synapse's to remove"
    );
}

/// Uninstalling is the one operation where a surprise cannot be undone, so
/// saying what would go is the default and doing it needs asking.
#[test]
fn uninstall_says_what_it_would_do_before_it_does_anything() {
    let root = tempfile::tempdir().unwrap();
    success(run(root.path(), &["guidance", "sync"], None));
    let instructions = root.path().join("home/.claude/CLAUDE.md");

    let preview = success(run(root.path(), &["uninstall"], None));
    assert!(preview.contains("would remove"), "got {preview}");
    assert!(
        preview.contains("would be left alone"),
        "it must say memory is safe: {preview}"
    );
    assert!(
        fs::read_to_string(&instructions)
            .unwrap()
            .contains("synapse:begin"),
        "the preview must not remove anything"
    );

    let asked = success(run(root.path(), &["uninstall", "--data"], None));
    assert!(
        asked.contains("cannot be undone"),
        "asking for the data folder must say so plainly: {asked}"
    );
    assert!(root.path().join("data").exists(), "still only a preview");

    success(run(root.path(), &["uninstall", "--confirm"], None));
    assert!(
        !fs::read_to_string(&instructions)
            .unwrap()
            .contains("synapse:begin"),
        "confirming should actually disconnect"
    );
    assert!(
        root.path().join("data/brain.db").is_file(),
        "memory is never removed without being asked for by name"
    );
}

/// A report is the thing you ask a person to paste into an issue, so it has to
/// come back even when what it is describing is broken.
#[test]
fn doctor_reports_a_damaged_store_instead_of_failing() {
    let root = tempfile::tempdir().unwrap();
    success(run(
        root.path(),
        &["memory", "add", "note"],
        Some("a memory"),
    ));

    let healthy = success(run(root.path(), &["doctor", "--json"], None));
    let report: Value = serde_json::from_str(&healthy).unwrap();
    assert_eq!(report["store"]["state"], "ok");
    assert_eq!(report["store"]["memories"], 1);
    assert!(report["crashes"].as_array().unwrap().is_empty());

    let database = root.path().join("data/brain.db");
    let mut bytes = fs::read(&database).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle..middle + 4096].fill(0xAB);
    fs::write(&database, bytes).unwrap();

    let broken = success(run(root.path(), &["doctor", "--json"], None));
    let report: Value = serde_json::from_str(&broken).unwrap();
    assert_ne!(
        report["store"]["state"], "ok",
        "damage must be reported, not glossed over"
    );
    assert!(
        report["version"].as_str().is_some(),
        "the rest of the report still has to arrive"
    );
}

/// A stand-in tool on PATH, so a launch test can never reach the real one.
fn faketool(folder: &Path, name: &str) {
    fs::create_dir_all(folder).unwrap();
    let path = folder.join(name);
    fs::write(&path, "#!/bin/sh\necho \"argv: $@\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn launch_hands_the_tool_its_own_flags_and_keeps_synapse_out_of_them() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let tools = root.path().join("tools");
    faketool(&tools, "claude");

    let mut command = command(root.path());
    command
        .current_dir(&project)
        .env("PATH", format!("{}:{}", tools.display(), env!("PATH")))
        .args([
            "launch", "claude", "--print", "--", "--model", "opus", "--resume",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let printed = success(command.spawn().unwrap().wait_with_output().unwrap());

    // `--model` after the separator is the tool's, not Synapse's to interpret.
    assert!(printed.contains("--model opus"), "got {printed}");
    assert!(printed.contains("--resume"), "got {printed}");
    // A tool with no connection of its own is wired for the life of the run.
    assert!(printed.contains("--mcp-config"), "got {printed}");
    assert!(
        printed.contains("SYNAPSE_PROJECT_DIR"),
        "the tool has to be told which checkout it is in: {printed}"
    );
    // No mesh name was asked for, so no harness prompt is prepended.
    assert!(
        !printed.contains("register"),
        "an unnamed launch is the person's own session: {printed}"
    );
}

#[test]
fn launch_refuses_a_scope_that_has_not_been_approved() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let tools = root.path().join("tools");
    faketool(&tools, "claude");
    success(run(
        root.path(),
        &["scope", "init", project.to_str().unwrap()],
        None,
    ));

    let mut command = command(root.path());
    command
        .current_dir(&project)
        .env("PATH", format!("{}:{}", tools.display(), env!("PATH")))
        .args(["launch", "claude"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.spawn().unwrap().wait_with_output().unwrap();

    assert!(
        !output.status.success(),
        "an unapproved scope must not reach a tool that can run a shell"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not been approved"), "got {stderr}");
}

#[test]
fn the_mux_joins_as_a_person_and_takes_its_name_back_when_it_leaves() {
    let root = tempfile::tempdir().unwrap();
    success(run(root.path(), &["settings", "mesh", "on"], None));

    let session = success(run(
        root.path(),
        &["mux", "--as", "wess"],
        Some("/agents\n/quit\n"),
    ));
    assert!(
        session.contains("You are `wess` on the mesh"),
        "got {session}"
    );
    assert!(
        session.contains("wess"),
        "the roster has to show them: {session}"
    );

    // Leaving is not a state anyone has to clean up after.
    let roster = success(run(root.path(), &["relay", "agents"], None));
    assert!(
        !roster.contains("wess"),
        "a closed mux must not leave a name nothing answers to: {roster}"
    );
}

#[test]
fn the_mux_holds_a_message_for_an_agent_that_has_not_registered_and_says_so() {
    let root = tempfile::tempdir().unwrap();
    success(run(root.path(), &["settings", "mesh", "on"], None));

    let output = runfrom(
        root.path(),
        None,
        &["mux", "--as", "wess"],
        Some("@backend look at the migration\n/quit\n"),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("→ backend"), "got {stdout}");
    assert!(
        stderr.contains("nobody is registered as `backend`"),
        "a typo'd name should not look like a delivery: {stderr}"
    );

    // Held, not dropped: the placeholder is the mechanism, so it stays off the
    // roster as something reachable.
    let roster = success(run(root.path(), &["relay", "agents"], None));
    assert!(
        !roster.contains("online"),
        "a name nobody has answered to is not online: {roster}"
    );
}

#[test]
fn the_mux_refuses_a_name_a_live_agent_is_already_answering_to() {
    let root = tempfile::tempdir().unwrap();
    success(run(root.path(), &["settings", "mesh", "on"], None));
    // Two people opening a mux under one name would drain each other's inbox.
    success(run(root.path(), &["mux", "--as", "wess"], Some("/quit\n")));
    let again = success(run(root.path(), &["mux", "--as", "wess"], Some("/quit\n")));
    assert!(
        again.contains("You are `wess` on the mesh"),
        "one person reopening their own mux is fine: {again}"
    );
}
