use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_synaps"));
    command
        .env("SYNAPS_HOME", root.join("home"))
        .env("SYNAPS_DATA", root.join("data"))
        .env("SYNAPS_BIN", root.join("bin").join("synaps"));
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
    assert!(inactive.contains("__synaps_state='inactive'"));

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
    assert!(active.contains("__synaps_state='active'"));
    assert!(active.contains(&format!(
        "__synaps_scope='{}'",
        project.canonicalize().unwrap().display()
    )));

    fs::write(
        project.join(".synaps.yaml"),
        "version: 1\nscope: project\nenv: {}\ndeny: []\n\n",
    )
    .unwrap();
    let blocked = success(runfrom(
        root.path(),
        Some(&nested),
        &["export", "zsh"],
        None,
    ));
    assert!(blocked.contains("__synaps_state='blocked'"));

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
    assert!(hook.contains("add-zsh-hook chpwd __synaps_hook"));
    assert!(hook.contains("SYNAPS_SHELL_ACTIVE=zsh"));
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
    let scope = fs::read_to_string(project.join(".synaps.yaml")).unwrap();
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
    let target = root.path().join("bin").join("synaps");
    success(run(root.path(), &["install"], None));

    let metadata = fs::symlink_metadata(&target).unwrap();
    assert!(!metadata.file_type().is_symlink());
    assert_ne!(metadata.permissions().mode() & 0o111, 0);
    let version = Command::new(&target).arg("version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("synaps "));
    success(run(root.path(), &["install"], None));

    let conflict = tempfile::tempdir().unwrap();
    let conflictpath = conflict.path().join("bin").join("synaps");
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
    let vault = format!("synapstest{suffix}");
    success(run(root.path(), &["vault", "create", &vault], None));
    let reference = format!("{vault}.token");

    let saved = run(
        root.path(),
        &[
            "secret",
            "set",
            &vault,
            "token",
            "SYNAPS_TEST_VALUE",
            "--global",
        ],
        Some("keychainvalue\n"),
    );
    let listed = run(root.path(), &["secret", "list", &vault], None);
    let launched = run(
        root.path(),
        &["run", "--", "/usr/bin/printenv", "SYNAPS_TEST_VALUE"],
        None,
    );

    let project = root.path().join("ambient");
    fs::create_dir(&project).unwrap();
    fs::write(
        project.join(".synaps.yaml"),
        format!("version: 1\nscope: project\nenv:\n  SYNAPS_TEST_VALUE: {reference}\ndeny: []\n"),
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
    assert!(listed.contains("SYNAPS_TEST_VALUE"));
    assert!(!listed.contains("keychainvalue"));
    assert_eq!(launched.trim(), "keychainvalue");
    assert!(ambient.contains("export SYNAPS_TEST_VALUE='keychainvalue'"));
    assert!(ambient.contains("__synaps_state='active'"));
}
