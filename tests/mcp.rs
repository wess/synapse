use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
fn mcp_stdio_lists_and_calls_every_tool() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_synapse"))
        .arg("mcp")
        .env("SYNAPSE_HOME", root.path().join("home"))
        .env("SYNAPSE_DATA", root.path().join("data"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let initialized = call(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "synapsetest", "version": "1"}
            }
        }),
        1,
    );
    assert_eq!(initialized["result"]["serverInfo"]["name"], "synapse");
    let instructions = initialized["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("At the start of every session"));
    assert!(instructions.contains("the `lean` budget first"));
    assert!(instructions.contains("call `remember` without waiting to be asked"));
    assert!(instructions.contains("instead of ad hoc memory Markdown files"));
    assert!(instructions.contains("never returns secret values"));
    assert!(instructions.contains("Synapse connected · <count> memories recalled"));
    assert!(instructions.contains("Synapse unavailable · <short reason>"));
    assert_eq!(instructions.matches("## Connection notice").count(), 1);
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    )
    .unwrap();
    let tools = call(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        2,
    );
    let remembered = call(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "remember",
                "arguments": {
                    "content": "mcp durable marker",
                    "source": "protocoltest",
                    "scope": "project",
                    "project": root.path()
                }
            }
        }),
        3,
    );
    let recalled = call(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "recall",
                "arguments": {
                    "query": "durable marker",
                    "limit": 4,
                    "budget": "lean",
                    "project": root.path()
                }
            }
        }),
        4,
    );
    let vault = call(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {"name": "vaultstatus", "arguments": {"path": root.path()}}
        }),
        5,
    );
    drop(stdin);
    drop(stdout);

    let status = child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "MCP failed: {stderr}");

    let names = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["recall", "remember", "vaultstatus"]);
    assert!(remembered.to_string().contains("stored"));
    assert_eq!(
        recalled["result"]["structuredContent"]["optimization"],
        "lean"
    );
    assert!(recalled.to_string().contains("mcp durable marker"));
    assert_eq!(
        recalled["result"]["structuredContent"]["memories"][0]["scope"],
        "project"
    );
    assert!(vault.to_string().contains("Values stay in Keychain"));
}

#[test]
fn existing_guidance_still_announces_the_connection() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("SOUL.md"), "# Mine\n\nKeep this.\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_synapse"))
        .arg("mcp")
        .env("SYNAPSE_HOME", root.path().join("home"))
        .env("SYNAPSE_DATA", &data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let initialized = call(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "synapsetest", "version": "1"}
            }
        }),
        1,
    );
    drop(stdin);
    drop(stdout);
    child.wait().unwrap();

    let instructions = initialized["result"]["instructions"].as_str().unwrap();
    assert!(instructions.starts_with("# Mine\n\nKeep this."));
    assert!(instructions.contains("Synapse connected · <count> memories recalled"));
    assert_eq!(
        std::fs::read_to_string(data.join("SOUL.md")).unwrap(),
        "# Mine\n\nKeep this.\n"
    );
}

fn call(stdin: &mut impl Write, stdout: &mut impl BufRead, request: Value, id: i64) -> Value {
    writeln!(stdin, "{request}").unwrap();
    stdin.flush().unwrap();
    loop {
        let mut line = String::new();
        assert_ne!(stdout.read_line(&mut line).unwrap(), 0, "MCP ended early");
        let response: Value = serde_json::from_str(&line).unwrap();
        if response["id"] == id {
            return response;
        }
    }
}
