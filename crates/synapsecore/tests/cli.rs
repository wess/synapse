use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_synapse-cli"));
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

/// A correction used to be a second memory contradicting the first, with
/// nothing on the machine to say which one was current.
#[test]
fn superseding_hides_a_memory_from_recall_without_losing_it() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();

    let old = stored(run(
        root,
        &["memory", "add", "deploys", "--global"],
        Some("Deploys run from the main branch"),
    ));
    let new = stored(run(
        root,
        &["memory", "add", "deploys", "--global"],
        Some("Deploys run from the release branch"),
    ));

    let done = success(run(root, &["memory", "supersede", &old, &new], None));
    assert!(
        done.contains(&format!("superseded by #{new}")),
        "got {done}"
    );
    assert!(
        done.contains("memory restore"),
        "it has to say how to undo it"
    );

    // Recall no longer returns it, and the session hook counts what recall can
    // see rather than what the table holds.
    let session: Value =
        serde_json::from_str(&success(run(root, &["session", "--json"], None))).unwrap();
    assert_eq!(session["memories"], 1);
    let bodies: Vec<String> = session["recalled"]
        .as_array()
        .unwrap()
        .iter()
        .map(|memory| memory["body"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(bodies, vec!["Deploys run from the release branch"]);

    // Still there, still readable, still says what replaced it.
    let shown = success(run(root, &["memory", "show", &old], None));
    assert!(shown.contains("Deploys run from the main branch"));
    assert!(
        shown.contains(&format!("Superseded by: #{new}")),
        "got {shown}"
    );
    let listed = success(run(root, &["memory", "list"], None));
    assert!(
        listed.contains(&format!("(superseded by #{new})")),
        "browsing has to show it or it cannot be restored: {listed}"
    );

    success(run(root, &["memory", "restore", &old], None));
    let restored: Value =
        serde_json::from_str(&success(run(root, &["session", "--json"], None))).unwrap();
    assert_eq!(restored["memories"], 2);
}

/// The ranked search drops function words and cannot be asked for an exact
/// string, and it never says which words it actually used.
#[test]
fn grep_finds_an_exact_string_and_explain_says_what_the_search_did() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();

    success(run(
        root,
        &["memory", "add", "release", "--global"],
        Some("Pass --no-verify only on the release commit"),
    ));

    let literal = success(run(root, &["memory", "grep", "--no-verify"], None));
    assert!(literal.contains("--no-verify"), "got {literal}");
    assert_eq!(
        success(run(root, &["memory", "grep", "--", "--no-verify"], None)),
        literal,
        "a bare separator makes a flag-shaped pattern the pattern"
    );
    assert!(
        success(run(root, &["memory", "grep", "nothing-like-this"], None))
            .trim()
            .is_empty()
    );

    let explained = success(run(
        root,
        &[
            "memory",
            "list",
            "where",
            "are",
            "the",
            "credentials",
            "--explain",
        ],
        None,
    ));
    assert!(
        explained.contains("Searched:   credentials"),
        "got {explained}"
    );
    assert!(
        explained.contains("Dropped:    where, are, the"),
        "the dropped words are the answer to why nothing came back: {explained}"
    );

    let json: Value = serde_json::from_str(&success(run(
        root,
        &["memory", "list", "the", "and", "of", "--explain", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(json["expression"], Value::Null);
    assert_eq!(json["kept"], Value::Array(Vec::new()));
}

/// Compaction is where a session loses what it learned. The hook is the only
/// thing that asks for it back before it goes.
#[test]
fn the_compaction_hook_asks_for_what_the_session_learned() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    success(run(
        root,
        &["memory", "add", "conventions", "--global"],
        Some("Identifiers are lowercase with no underscores"),
    ));

    let payload: Value = serde_json::from_str(&success(run(root, &["compact"], None))).unwrap();

    assert_eq!(payload["hookSpecificOutput"]["hookEventName"], "PreCompact");
    let context = payload["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("about to be compacted"), "got {context}");
    assert!(context.contains("`remember`"));
    assert!(
        context.contains("`supersedes`"),
        "a correction made this session is the case this exists for"
    );
    assert!(
        !context.contains("Identifiers are lowercase"),
        "recalling into a window that is being reclaimed is the opposite of the point"
    );
}

fn stored(output: Output) -> String {
    success(output)
        .trim()
        .strip_prefix("Stored memory #")
        .expect("expected a stored memory id")
        .to_owned()
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
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    let root = tempfile::tempdir().unwrap();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let vault = format!("synapsetest{suffix}");
    // The encrypted store is the default now, so this test has to ask for the
    // backend it is about. Nothing is stored yet, so choosing is allowed.
    success(run(root.path(), &["vault", "backend", "keychain"], None));
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

    // The value comes back out the one way it is allowed to: onto the
    // pasteboard, without the process that put it there ever printing it.
    let pasteboard = root.path().join("pasteboard");
    let stub = root.path().join("clipboard.sh");
    fs::write(
        &stub,
        format!("#!/bin/sh\ncat > \"{}\"\n", pasteboard.display()),
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    let copied = command(root.path())
        .env("SYNAPSE_CLIPBOARD", &stub)
        .args(["secret", "copy", &reference])
        .output()
        .unwrap();

    let forgotten = run(root.path(), &["secret", "forget", &reference], None);
    let deleted = run(root.path(), &["vault", "delete", &vault], None);
    let saved = success(saved);
    let listed = success(listed);
    let launched = success(launched);
    let ambient = success(ambient);
    let copied = success(copied);
    success(forgotten);
    success(deleted);
    assert!(!saved.contains("keychainvalue"));
    assert!(saved.contains("keychain vault"));
    assert!(listed.contains("SYNAPSE_TEST_VALUE"));
    assert!(!listed.contains("keychainvalue"));
    assert_eq!(launched.trim(), "keychainvalue");
    assert!(ambient.contains("export SYNAPSE_TEST_VALUE='keychainvalue'"));
    assert!(ambient.contains("__synapse_state='active'"));
    assert!(!copied.contains("keychainvalue"));
    assert_eq!(fs::read_to_string(&pasteboard).unwrap(), "keychainvalue");
}

/// The encrypted store, which is the one every fresh machine gets and the only
/// one that exists off macOS. Not gated on the platform on purpose: this is the
/// test that says the vault works on a Linux box.
#[test]
fn the_encrypted_vault_holds_a_value_that_is_nowhere_on_disk_in_the_clear() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");

    assert_eq!(
        success(run(root.path(), &["vault", "backend"], None)).trim(),
        "encrypted"
    );
    success(run(root.path(), &["vault", "create", "work"], None));
    let saved = success(run(
        root.path(),
        &[
            "secret",
            "set",
            "work",
            "token",
            "SYNAPSE_TEST_VALUE",
            "--global",
        ],
        Some("sealedvalue\n"),
    ));
    assert!(saved.contains("encrypted vault"));
    assert!(!saved.contains("sealedvalue"));

    // It reaches a child process, which is the whole point of holding it.
    let launched = success(run(
        root.path(),
        &["run", "--", "printenv", "SYNAPSE_TEST_VALUE"],
        None,
    ));
    assert_eq!(launched.trim(), "sealedvalue");

    let listed = success(run(root.path(), &["secret", "list", "work"], None));
    assert!(listed.contains("SYNAPSE_TEST_VALUE"));
    assert!(!listed.contains("sealedvalue"));

    let status = success(run(root.path(), &["status", "--json"], None));
    let status: Value = serde_json::from_str(&status).unwrap();
    assert_eq!(status["backend"], "encrypted");

    // The value is in its own file, sealed, and `brain.db` still holds none of
    // it: `data export` hands somebody memory and not credentials.
    assert!(data.join("vault.db").is_file());
    assert!(data.join("vault.key").is_file());
    for file in fs::read_dir(&data).unwrap() {
        let file = file.unwrap().path();
        if file.is_file() {
            let bytes = fs::read(&file).unwrap();
            assert!(
                !String::from_utf8_lossy(&bytes).contains("sealedvalue"),
                "{} holds the value in the clear",
                file.display()
            );
        }
    }

    let pasteboard = root.path().join("pasteboard");
    let stub = root.path().join("clipboard.sh");
    fs::write(
        &stub,
        format!("#!/bin/sh\ncat > \"{}\"\n", pasteboard.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let copied = success(
        command(root.path())
            .env("SYNAPSE_CLIPBOARD", &stub)
            .args(["secret", "copy", "work.token"])
            .output()
            .unwrap(),
    );
    assert!(!copied.contains("sealedvalue"));
    assert_eq!(fs::read_to_string(&pasteboard).unwrap(), "sealedvalue");

    success(run(root.path(), &["secret", "forget", "work.token"], None));
    let gone = run(
        root.path(),
        &["run", "--", "printenv", "SYNAPSE_TEST_VALUE"],
        None,
    );
    assert!(!gone.status.success() || String::from_utf8_lossy(&gone.stdout).trim().is_empty());
}

#[cfg(unix)]
#[test]
fn the_encrypted_vault_and_its_key_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    success(run(root.path(), &["vault", "create", "work"], None));
    success(run(
        root.path(),
        &["secret", "set", "work", "token", "TOKEN", "--global"],
        Some("sealedvalue\n"),
    ));
    for name in ["vault.db", "vault.key"] {
        let mode = fs::metadata(root.path().join("data").join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name} is readable by somebody else");
    }
}

/// Both directions, because a person who tries the encrypted store has to be
/// able to go back to the Keychain without losing anything.
#[cfg(target_os = "macos")]
#[test]
fn a_value_moves_between_the_two_stores_and_leaves_the_old_one_empty() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let root = tempfile::tempdir().unwrap();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let vault = format!("synapsemove{suffix}");
    let reference = format!("{vault}.token");
    success(run(root.path(), &["vault", "create", &vault], None));
    success(run(
        root.path(),
        &[
            "secret",
            "set",
            &vault,
            "token",
            "SYNAPSE_TEST_VALUE",
            "--global",
        ],
        Some("movedvalue\n"),
    ));

    let out = run(root.path(), &["vault", "migrate", "keychain"], None);
    let backend = run(root.path(), &["vault", "backend"], None);
    let resolved = run(
        root.path(),
        &["run", "--", "/usr/bin/printenv", "SYNAPSE_TEST_VALUE"],
        None,
    );
    let back = run(root.path(), &["vault", "migrate", "encrypted"], None);
    let afterwards = run(
        root.path(),
        &["run", "--", "/usr/bin/printenv", "SYNAPSE_TEST_VALUE"],
        None,
    );
    // Clean up the Keychain before asserting, so a failure does not leave an
    // item behind on the machine that ran the suite.
    let forgotten = run(root.path(), &["secret", "forget", &reference], None);

    let out = success(out);
    assert!(out.contains("moved"));
    assert_eq!(success(backend).trim(), "keychain");
    assert_eq!(success(resolved).trim(), "movedvalue");
    success(back);
    assert_eq!(success(afterwards).trim(), "movedvalue");
    success(forgotten);

    // The value left the Keychain when it moved back out of it.
    let found = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "app.synapse.vault",
            "-a",
            &reference,
        ])
        .output()
        .unwrap();
    assert!(
        !found.status.success(),
        "the value is still in the Keychain"
    );
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
    // The memory itself, not a count of it. Asking the model to call `recall`
    // is guidance it may skip; handing it over is not.
    assert!(
        context.contains("Ship the beta on Fridays."),
        "the hook should carry the memory, not only its number: {context}"
    );

    // Another project's memory stays out of it. The hook recalls without being
    // asked, so the scope rule has to hold here or a session opens holding
    // somebody else's decisions.
    let other = root.path().join("other");
    fs::create_dir_all(other.join(".git")).unwrap();
    success(runfrom(
        root.path(),
        Some(&other),
        &[
            "memory",
            "add",
            "meshtest",
            "--project",
            &other.display().to_string(),
        ],
        Some("Never deploy on Fridays."),
    ));
    let scoped: Value = serde_json::from_str(&success(runfrom(
        root.path(),
        Some(&project),
        &["session"],
        Some(&format!("{{\"cwd\":\"{folder}\"}}")),
    )))
    .unwrap();
    let scoped = scoped["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(scoped.contains("Ship the beta on Fridays."));
    assert!(
        !scoped.contains("Never deploy on Fridays."),
        "another project's memory must not reach this session: {scoped}"
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
        before.contains("synapse-mesh\tglobal\tCodex\tnot installed"),
        "got {before}"
    );
    assert!(
        before.contains("synapse-mesh\tglobal\tClaude Code\tnot installed"),
        "got {before}"
    );
    assert!(
        before.contains("synapse-mesh\tglobal\tpi\tnot installed"),
        "got {before}"
    );

    success(run(root.path(), &["skill", "install"], None));

    // Each tool reads personal skills from its own folder, and all of them got
    // a copy.
    let claude = home.join(".claude/skills/synapse-mesh/SKILL.md");
    let codex = home.join(".agents/skills/synapse-mesh/SKILL.md");
    let pi = home.join(".pi/agent/skills/synapse-mesh/SKILL.md");
    assert!(claude.is_file(), "Claude Code did not get the skill");
    assert!(codex.is_file(), "Codex did not get the skill");
    assert!(pi.is_file(), "pi did not get the skill");
    assert_eq!(
        fs::read_to_string(&claude).unwrap(),
        fs::read_to_string(&codex).unwrap(),
        "the whole point is that the copies cannot drift"
    );
    assert_eq!(
        fs::read_to_string(&claude).unwrap(),
        fs::read_to_string(&pi).unwrap(),
        "the whole point is that the copies cannot drift"
    );

    let after = success(run(root.path(), &["skill", "status"], None));
    assert!(
        after.contains("synapse-mesh\tglobal\tCodex\tinstalled"),
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
        stale.contains("synapse-mesh\tglobal\tCodex\tupdate available"),
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
        status.contains("mine\tglobal\tClaude Code\tnot in the library"),
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
        adopted.contains("mine\tglobal\tClaude Code\tinstalled"),
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
        changed.contains("mine\tglobal\tCodex\tchanged in place"),
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
fn launching_pi_writes_the_extension_that_carries_the_tools() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let tools = root.path().join("tools");
    faketool(&tools, "pi");

    let mut command = command(root.path());
    command
        .current_dir(&project)
        .env("PATH", format!("{}:{}", tools.display(), env!("PATH")))
        .args(["launch", "pi", "--print"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let printed = success(command.spawn().unwrap().wait_with_output().unwrap());

    // pi has no MCP client, so the extension is what `--mcp-config` is for the
    // other two: everything it needs for this run, and nothing written into its
    // own configuration.
    assert!(printed.contains("--extension"), "got {printed}");
    let entry = root.path().join("data/relay/pi/synapse/index.ts");
    assert!(entry.is_file(), "the extension was not written out");
    assert!(
        fs::read_to_string(entry)
            .unwrap()
            .contains("export default"),
        "the entry point has to be loadable by pi"
    );
    assert!(
        fs::read_to_string(root.path().join("data/relay/pi/synapse/client.ts"))
            .unwrap()
            .contains("tools/list"),
        "the client the extension imports has to be written out beside it"
    );
    assert!(
        !root.path().join("home/.pi").exists(),
        "starting a tool must not touch that tool's own configuration"
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

/// A tool Synapse has never heard of, described by the person who uses it and
/// then carried the whole way: detected, connected through its own CLI,
/// pointed at the shared guidance, given the skill library, launched, and taken
/// back out again. Nothing in the binary knows the word `hermes`.
#[test]
#[cfg(unix)]
fn a_tool_nobody_shipped_connects_the_same_way_the_built_ins_do() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bin = root.path().join("toolbin");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(home.join(".hermes")).unwrap();

    // A stand-in for the tool's own CLI: it records what it was asked to do and
    // writes its own settings, exactly as `claude mcp add` would.
    let settings = home.join(".hermes/settings.json");
    let log = root.path().join("hermes.log");
    let executable = bin.join("hermes");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n\
             if [ \"$1\" = mcp ] && [ \"$2\" = add ]; then\n\
               printf '{{\"servers\":{{\"synapse\":{{\"command\":\"%s\",\"args\":[\"mcp\"]}}}}}}' \"$5\" > '{}'\n\
             fi\n\
             if [ \"$1\" = mcp ] && [ \"$2\" = remove ]; then printf '{{}}' > '{}'; fi\nexit 0\n",
            log.display(),
            settings.display(),
            settings.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();

    // The descriptor, in the layer a person owns.
    let tools = root.path().join("data/tools");
    fs::create_dir_all(&tools).unwrap();
    fs::write(
        tools.join("hermes.toml"),
        "name = \"Hermes\"\ncommand = \"hermes\"\n\
         [home]\ndefault = \".hermes\"\n\
         [paths]\ninstructions = \"{home}/AGENTS.md\"\nsettings = \"{home}/settings.json\"\n\
         integration = \"{home}/settings.json\"\nskills = \"{home}/skills\"\n\
         [connect]\nadd = [\"mcp\", \"add\", \"synapse\", \"--\", \"{server}\", \"mcp\"]\n\
         remove = [\"mcp\", \"remove\", \"synapse\"]\n\
         [detect]\nformat = \"json\"\nat = [\"servers\", \"synapse\"]\nargs = [\"mcp\"]\n\
         [launch]\nprompt = [\"{prompt}\"]\nconfig = [\"--mcp-config\", \"{config}\"]\n",
    )
    .unwrap();

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let hermes = |arguments: &[&str]| -> Output {
        let mut command = command(root.path());
        command
            .args(arguments)
            .env("PATH", &path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.spawn().unwrap().wait_with_output().unwrap()
    };

    // It is listed beside the built-ins, and labelled as the user's.
    let listed = success(hermes(&["tool", "list"]));
    assert!(listed.contains("hermes\tuser"), "got {listed}");
    assert!(listed.contains("codex\tbuilt-in"), "got {listed}");

    success(hermes(&["connect", "hermes"]));

    // Synapse asked the tool's own CLI to register the server. It never wrote
    // that entry into the settings file itself.
    let asked = fs::read_to_string(&log).unwrap();
    assert!(
        asked.contains("mcp add synapse --"),
        "the tool's own CLI should have registered the server: {asked}"
    );
    assert!(
        fs::read_to_string(&settings).unwrap().contains("synapse"),
        "the tool's settings should now name the server"
    );
    // And the same managed guidance pointer every built-in gets.
    let guidance = fs::read_to_string(home.join(".hermes/AGENTS.md")).unwrap();
    assert!(
        guidance.contains("<!-- synapse:begin -->"),
        "got {guidance}"
    );

    // The skill library reaches it by name.
    success(hermes(&["skill", "install", "--tool", "hermes"]));
    assert!(
        home.join(".hermes/skills/synapse-mesh/SKILL.md").is_file(),
        "the skill library should install into a described tool too"
    );

    // And it launches, through the argv slots its own descriptor declares.
    let preview = success(hermes(&["launch", "hermes", "--print"]));
    assert!(preview.contains("hermes"), "got {preview}");
    assert!(preview.contains("--mcp-config"), "got {preview}");

    // Everything Synapse wrote, it can take back.
    let removed = success(hermes(&["disconnect", "hermes"]));
    assert!(removed.contains("Hermes MCP registration"), "got {removed}");
    assert!(removed.contains("Hermes guidance pointer"), "got {removed}");
    assert!(
        !home.join(".hermes/skills/synapse-mesh").exists(),
        "the skill it installed should have come back out"
    );
    let guidance = fs::read_to_string(home.join(".hermes/AGENTS.md")).unwrap();
    assert!(
        !guidance.contains("<!-- synapse:begin -->"),
        "the managed block should be gone: {guidance}"
    );
}

/// A descriptor that moves in a release has to be able to reach a machine that
/// is already connected. Detection cannot see that on its own — it compares the
/// stored command against this binary, so an entry written under an older
/// descriptor reads as perfectly healthy — which is why `--refresh` exists and
/// why the connection record exists to say when it is worth pressing.
#[test]
#[cfg(unix)]
fn a_descriptor_that_moves_reaches_a_tool_that_is_already_connected() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bin = root.path().join("toolbin");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(home.join(".hermes")).unwrap();

    let settings = home.join(".hermes/settings.json");
    let log = root.path().join("hermes.log");
    let executable = bin.join("hermes");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n\
             if [ \"$1\" = mcp ] && [ \"$2\" = add ]; then\n\
               printf '{{\"servers\":{{\"synapse\":{{\"command\":\"%s\",\"args\":[\"mcp\"]}}}}}}' \"$5\" > '{}'\n\
             fi\n\
             if [ \"$1\" = mcp ] && [ \"$2\" = remove ]; then printf '{{}}' > '{}'; fi\nexit 0\n",
            log.display(),
            settings.display(),
            settings.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();

    let tools = root.path().join("data/tools");
    fs::create_dir_all(&tools).unwrap();
    let descriptor = |extra: &str| {
        format!(
            "name = \"Hermes\"\ncommand = \"hermes\"\n\
             [home]\ndefault = \".hermes\"\n\
             [paths]\ninstructions = \"{{home}}/AGENTS.md\"\nsettings = \"{{home}}/settings.json\"\n\
             integration = \"{{home}}/settings.json\"\nskills = \"{{home}}/skills\"\n\
             [connect]\nadd = [\"mcp\", \"add\", \"synapse\"{extra}, \"--\", \"{{server}}\", \"mcp\"]\n\
             remove = [\"mcp\", \"remove\", \"synapse\"]\n\
             [detect]\nformat = \"json\"\nat = [\"servers\", \"synapse\"]\nargs = [\"mcp\"]\n"
        )
    };
    fs::write(tools.join("hermes.toml"), descriptor("")).unwrap();

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let hermes = |arguments: &[&str]| -> Output {
        let mut command = command(root.path());
        command
            .args(arguments)
            .env("PATH", &path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.spawn().unwrap().wait_with_output().unwrap()
    };
    let registrations = || {
        fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("mcp add synapse"))
            .count()
    };

    success(hermes(&["connect", "hermes"]));
    assert_eq!(registrations(), 1);

    // Connecting again leaves a working registration alone. That is the whole
    // reason a descriptor change cannot ride in on an ordinary connect.
    success(hermes(&["connect", "hermes"]));
    assert_eq!(
        registrations(),
        1,
        "an ordinary connect should not re-register"
    );

    // Nothing has moved yet, so nothing claims an update is available.
    let before = success(hermes(&["doctor"]));
    assert!(before.contains("Hermes"), "got {before}");
    assert!(
        !before.contains("update available"),
        "an unchanged descriptor is not an update: {before}"
    );

    // The descriptor gains a flag — the shape of the Ainz `--required` change,
    // which detection has no way to notice.
    fs::write(tools.join("hermes.toml"), descriptor(", \"--required\"")).unwrap();

    let moved = success(hermes(&["doctor"]));
    assert!(
        moved.contains("connected · update available"),
        "a moved descriptor should be reported: {moved}"
    );

    // Refreshing writes the registration again, with the flag this time.
    success(hermes(&["connect", "hermes", "--refresh"]));
    assert_eq!(registrations(), 2, "--refresh should re-register");
    let asked = fs::read_to_string(&log).unwrap();
    assert!(
        asked.lines().any(|line| line.contains("--required")),
        "the new descriptor's flag should have reached the tool: {asked}"
    );

    // And the row stops asking, because the record now matches what resolves.
    let after = success(hermes(&["doctor"]));
    assert!(
        !after.contains("update available"),
        "refreshing should settle it: {after}"
    );

    // Reset is the bigger hammer: out first, then in again.
    success(hermes(&["connect", "hermes", "--reset"]));
    assert_eq!(registrations(), 3, "--reset should register again too");
    let asked = fs::read_to_string(&log).unwrap();
    assert!(
        asked
            .lines()
            .any(|line| line.starts_with("mcp remove synapse")),
        "a reset should have disconnected first: {asked}"
    );

    // Disconnecting forgets the record with the connection. A record that
    // outlived its connection would report the next one as current the moment
    // it was made, whatever descriptor it actually used.
    success(hermes(&["disconnect", "hermes"]));
    fs::write(tools.join("hermes.toml"), descriptor("")).unwrap();
    let gone = success(hermes(&["doctor"]));
    assert!(
        !gone.contains("update available"),
        "a disconnected tool has nothing to compare: {gone}"
    );
}

/// What Synapse costs a session, counted rather than guessed at.
///
/// The point of the command is that the surfaces with no budget — the guidance,
/// the tool schemas, every skill description — are the ones nobody was watching,
/// so the test's job is that they are all present and that the settings which
/// move them are reflected.
#[test]
fn the_context_cost_is_reported_and_moves_with_the_settings() {
    let root = tempfile::tempdir().unwrap();

    let report = success(run(root.path(), &["tokens"], None));
    for section in [
        "Guidance",
        "Tool schemas",
        "Skill library",
        "Startup recall",
    ] {
        assert!(report.contains(section), "missing {section}: {report}");
    }
    // The estimate says it is one, every time it is shown.
    assert!(report.contains("not a tokenizer"), "got {report}");
    // Mesh off is the default, and the report says what that is worth rather
    // than leaving the setting as a preference with no number on it.
    assert!(report.contains("mesh off, saving"), "got {report}");

    let quiet: serde_json::Value =
        serde_json::from_str(&success(run(root.path(), &["tokens", "--json"], None))).unwrap();
    let before = quiet["total"]["tokens"].as_u64().unwrap();
    assert!(before > 0);
    assert_eq!(quiet["estimated"], serde_json::json!(true));

    // Turning the mesh on adds sixteen tool definitions to every session, which
    // is the single largest lever there is and now has a number beside it.
    success(run(root.path(), &["settings", "mesh", "on"], None));
    let loud: serde_json::Value =
        serde_json::from_str(&success(run(root.path(), &["tokens", "--json"], None))).unwrap();
    let after = loud["total"]["tokens"].as_u64().unwrap();
    assert!(
        after > before,
        "the mesh should cost something visible: {before} then {after}"
    );

    // And the same numbers reach the report somebody actually pastes into an
    // issue, rather than living only in a command they would have to know about.
    let doctor = success(run(root.path(), &["doctor"], None));
    assert!(doctor.contains("Context cost per session"), "got {doctor}");
    assert!(doctor.contains("synapse tokens"), "got {doctor}");
}

/// A skill about one repository belongs to that repository, and installs beside
/// it rather than into every session on the machine.
#[test]
fn a_project_skill_stays_with_its_project() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("repo");
    fs::create_dir_all(&project).unwrap();
    let folder = project.display().to_string();

    success(run(
        root.path(),
        &["skill", "create", "release", "--project", &folder],
        None,
    ));

    // A global skill of the same name is a different skill, not a collision.
    success(run(root.path(), &["skill", "create", "release"], None));
    let listed = success(run(root.path(), &["skill", "list"], None));
    assert_eq!(listed.matches("release\t").count(), 2, "got {listed}");
    assert!(listed.contains("release\tproject"), "got {listed}");
    assert!(listed.contains("release\tglobal"), "got {listed}");

    success(run(
        root.path(),
        &["skill", "install", "release", "--project", &folder],
        None,
    ));

    assert!(
        project.join(".claude/skills/release/SKILL.md").is_file(),
        "a project skill installs beside its project"
    );
    assert!(
        !home.join(".claude/skills/release").exists(),
        "and never into the personal skills folder"
    );

    // Codex keeps project skills in the shared location inside the repository.
    assert!(project.join(".agents/skills/release/SKILL.md").is_file());

    let status = success(run(root.path(), &["skill", "status", "release"], None));
    assert!(
        status.contains("release\tproject\tClaude Code\tinstalled"),
        "got {status}"
    );
    assert!(
        status.contains("release\tglobal\tClaude Code\tnot installed"),
        "got {status}"
    );

    // Disconnecting takes back what went into the project, not just the home.
    success(run(root.path(), &["disconnect", "claude"], None));
    assert!(!project.join(".claude/skills/release").exists());
    assert!(
        project.join(".agents/skills/release").exists(),
        "Codex keeps its own"
    );
}

/// The learn setting is off until it is asked for, and says so.
#[test]
fn self_improvement_is_off_until_it_is_switched_on() {
    let root = tempfile::tempdir().unwrap();

    let settings = success(run(root.path(), &["settings", "show"], None));
    assert!(settings.contains("learn\toff"), "got {settings}");

    let switched = success(run(root.path(), &["settings", "learn", "on"], None));
    assert!(
        switched.contains("waits in `synapse skill proposed`"),
        "got {switched}"
    );
    assert!(success(run(root.path(), &["settings", "show"], None)).contains("learn\ton"));

    let waiting = success(run(root.path(), &["skill", "proposed"], None));
    assert!(waiting.contains("Nothing is waiting"), "got {waiting}");

    // A skill nobody proposed cannot be turned down; that is `delete`, which
    // says what it does.
    success(run(root.path(), &["skill", "create", "mine"], None));
    let refused = run(root.path(), &["skill", "reject", "mine", "--confirm"], None);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("not waiting for review"),
        "got {}",
        String::from_utf8_lossy(&refused.stderr)
    );
}
