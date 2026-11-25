use fs2::FileExt;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use synapsecore::brain::{Brain, MemoryScope};

#[tokio::test]
async fn devcontainer_mcp_is_bound_to_the_enrolled_project_and_revocable() {
    let fixture = tempfile::tempdir().unwrap();
    let home = fixture.path().join("host's home");
    let data = fixture.path().join("data");
    let project = fixture.path().join("host checkout");
    let other = fixture.path().join("another checkout");
    let key = fixture.path().join("hostkey");
    fs::create_dir_all(home.join(".ssh")).unwrap();
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::create_dir_all(other.join(".git")).unwrap();
    fs::write(
        project.join(".git/config"),
        "[remote \"origin\"]\nurl=git@example.com:team/app\n",
    )
    .unwrap();
    fs::write(
        other.join(".git/config"),
        "[remote \"origin\"]\nurl=git@example.com:team/other\n",
    )
    .unwrap();
    let authorized = home.join(".ssh/authorized_keys");
    let existing = "# user-owned key entry\n";
    fs::write(&authorized, existing).unwrap();
    assert!(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&key)
            .status()
            .expect("the devcontainer setup requires OpenSSH on the host")
            .success()
    );
    let cli = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_synapse-cli"));
        command
            .current_dir(&project)
            .env("SYNAPSE_HOME", &home)
            .env("SYNAPSE_DATA", &data);
        command
    };
    let setup = || {
        let output = cli()
            .args([
                "devcontainer",
                "setup",
                "--host",
                "localhost",
                "--port",
                "2222",
                "--host-key",
            ])
            .arg(key.with_extension("pub"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let snippet = setup();
    let mounted = snippet["mounts"][0].as_str().unwrap();
    let (source, target) = mounted
        .strip_prefix("source=")
        .unwrap()
        .split_once(",target=")
        .unwrap();
    let bundle = Path::new(source);
    let directory = bundle.parent().unwrap();
    let id = directory.file_name().unwrap().to_str().unwrap();
    let grantpath = directory.join("grant.json");
    let grant: Value = serde_json::from_slice(&fs::read(&grantpath).unwrap()).unwrap();
    let generation = grant["generation"].as_str().unwrap();
    assert!(target.ends_with(",type=bind,readonly"));
    assert_eq!(
        fs::metadata(bundle.join("identity"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(!bundle.join("grant.json").exists());
    assert!(
        fs::read_to_string(bundle.join("stdio"))
            .unwrap()
            .contains("StrictHostKeyChecking=yes")
    );
    let keys = fs::read_to_string(&authorized).unwrap();
    assert!(keys.starts_with(existing));
    assert!(keys.contains("restrict,command=\""));
    assert_eq!(setup(), snippet);
    assert_eq!(fs::read_to_string(&authorized).unwrap(), keys);

    let brain = Brain::open(data.join("brain.db")).await.unwrap();
    let mine = brain
        .rememberscoped(
            "devcontainer cipher alpha",
            None,
            MemoryScope::Project,
            Some(&project),
        )
        .await
        .unwrap();
    let outside = brain
        .rememberscoped(
            "devcontainer cipher beta",
            None,
            MemoryScope::Project,
            Some(&other),
        )
        .await
        .unwrap();
    let global = brain
        .rememberscoped(
            "devcontainer cipher global",
            None,
            MemoryScope::Global,
            None,
        )
        .await
        .unwrap();
    let mut server = cli()
        .args(["devcontainer", "serve"])
        .arg(&grantpath)
        .arg(generation)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = server.stdin.take().unwrap();
    let mut stdout = BufReader::new(server.stdout.take().unwrap());
    let initialized = super::call(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                       "clientInfo": {"name": "test", "version": "1"}}
        }),
        1,
    );
    assert_eq!(initialized["result"]["serverInfo"]["name"], "synapse");
    assert!(
        initialized["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("focused query and a small Lean budget")
    );
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    )
    .unwrap();
    let tools = super::call(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        2,
    );
    let names = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, ["readmemory", "recall", "remember"].into());

    let slots = directory.parent().unwrap().join("sessions");
    let other_slots: Vec<_> = (1..8)
        .map(|slot| {
            let file = fs::File::create(slots.join(slot.to_string())).unwrap();
            FileExt::try_lock_exclusive(&file).unwrap();
            file
        })
        .collect();
    let refused = cli()
        .args(["devcontainer", "serve"])
        .arg(&grantpath)
        .arg(generation)
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("all 8 devcontainer sessions"));
    drop(other_slots);

    // An oversized frame is discarded, and the original session can still work.
    writeln!(stdin, "{}", " ".repeat(2 * 1024 * 1024)).unwrap();

    // The container can rename origin in a shared checkout or send a path
    // parameter. Neither lets the key move to a different project.
    fs::copy(other.join(".git/config"), project.join(".git/config")).unwrap();
    let mut tool = |id, name, arguments| {
        super::call(
            &mut stdin,
            &mut stdout,
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                   "params": {"name": name, "arguments": arguments}}),
            id,
        )["result"]
            .clone()
    };
    for (id, query) in [(3, "cipher"), (4, "")] {
        let recall = tool(id, "recall", json!({"query": query, "project": other}));
        let memories = recall["structuredContent"]["memories"].as_array().unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0]["id"], mine);
    }
    for (id, memory) in [(5, outside), (6, global)] {
        let read = tool(id, "readmemory", json!({"id": memory, "project": other}));
        assert!(read["structuredContent"]["memory"].is_null());
    }
    let oversized = tool(10, "recall", json!({"query": "x".repeat(4097)}));
    assert_eq!(oversized["isError"], true);
    for (id, arguments) in [
        (
            11,
            json!({"content": "skip this", "source": "x".repeat(2049)}),
        ),
        (
            12,
            json!({"content": "skip this", "supersedes": vec![mine; 65]}),
        ),
    ] {
        assert_eq!(tool(id, "remember", arguments)["isError"], true);
    }
    let corrected = tool(
        7,
        "remember",
        json!({
            "content": "devcontainer cipher correction",
            "scope": "global", "project": other,
            "supersedes": [mine, outside, global]
        }),
    );
    let new = corrected["structuredContent"]["id"].as_i64().unwrap();
    assert_eq!(corrected["structuredContent"]["superseded"], json!([mine]));
    assert_eq!(
        brain.memory(new).await.unwrap().unwrap().project,
        "example.com/team/app"
    );
    assert_eq!(brain.memory(mine).await.unwrap().unwrap().superseded, new);
    assert_eq!(brain.memory(outside).await.unwrap().unwrap().superseded, 0);
    assert_eq!(brain.memory(global).await.unwrap().unwrap().superseded, 0);

    let revoked = cli()
        .args(["devcontainer", "revoke", "--id", id])
        .output()
        .unwrap();
    assert!(
        revoked.status.success(),
        "{}",
        String::from_utf8_lossy(&revoked.stderr)
    );
    assert_eq!(fs::read_to_string(&authorized).unwrap(), existing);
    assert!(!bundle.exists());
    let denied = tool(8, "recall", json!({"query": "cipher"}));
    assert_eq!(denied["isError"], true);
    assert_eq!(setup(), snippet);
    assert_ne!(
        serde_json::from_slice::<Value>(&fs::read(&grantpath).unwrap()).unwrap()["generation"],
        generation
    );
    assert_eq!(
        tool(9, "recall", json!({"query": "cipher"}))["isError"],
        true
    );
    drop(stdin);
    drop(stdout);
    assert!(server.wait().unwrap().success());
}
