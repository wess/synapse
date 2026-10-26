use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_synapse-cli"));
    command
        .current_dir(root.join("project"))
        .env("SYNAPSE_DATA", root.join("data"))
        .env("SYNAPSE_HOME", root.join("home"))
        .env("SYNAPSE_PROJECT_DIR", root.join("project"));
    command
}

fn run(root: &Path, args: &[&str]) -> String {
    let output = command(root).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

struct Client {
    process: Child,
    input: ChildStdin,
    output: Receiver<Value>,
    next: u32,
}

impl Client {
    fn open(root: &Path) -> Self {
        let mut process = command(root)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();
        let (send, output) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = serde_json::from_str(&line)
                    && send.send(value).is_err()
                {
                    break;
                }
            }
        });
        let mut client = Self {
            process,
            input,
            output,
            next: 0,
        };
        let initialized = client.call(
            "initialize",
            json!({
                "protocolVersion":"2025-03-26", "capabilities":{},
                "clientInfo":{"name":"synapseknowledgecheck","version":"1"}
            }),
        );
        assert!(initialized["result"]["instructions"].is_string());
        client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        client
    }

    fn send(&mut self, value: Value) {
        writeln!(self.input, "{value}").unwrap();
        self.input.flush().unwrap();
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        loop {
            let value = self
                .output
                .recv_timeout(Duration::from_secs(15))
                .expect("MCP response timed out");
            if value["id"] == id {
                return value;
            }
        }
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.call("tools/call", json!({"name":name,"arguments":arguments}));
        assert!(response.get("error").is_none(), "{response}");
        assert_ne!(response["result"]["isError"], true, "{response}");
        response["result"]["structuredContent"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[test]
fn cli_mcp_and_session_hooks_share_the_knowledge_setting_and_guides() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let project = root.join("project");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::write(project.join("Cargo.toml"), "[package]\nname = 'fixture'\n").unwrap();
    assert!(run(root, &["settings", "show"]).contains("knowledge\toff"));
    {
        let mut client = Client::open(root);
        let listed = client.call("tools/list", json!({}));
        assert!(!listed.to_string().contains("writeknowledge"));
        assert!(
            client
                .tool("recall", json!({"query":"", "project":project}))
                .get("knowledge")
                .is_none()
        );
    }
    assert!(!project.join(".synapse").exists());
    run(root, &["settings", "knowledge", "on"]);
    let mut client = Client::open(root);
    let listed = client.call("tools/list", json!({}));
    assert!(listed.to_string().contains("writeknowledge"));
    let index = client.tool("knowledge", json!({"project":project}));
    assert_eq!(index["catalog"]["guides"].as_array().unwrap().len(), 0);
    let outside = root.join("outside");
    std::fs::create_dir_all(outside.join(".git")).unwrap();
    std::fs::write(outside.join("source.txt"), "outside").unwrap();
    let rejected = client.call(
        "tools/call",
        json!({"name":"writeknowledge","arguments":{"project":outside,
            "name":"overview", "description":"outside", "content":"outside",
            "sources":["source.txt"]}}),
    );
    assert_eq!(rejected["result"]["isError"], true);
    assert!(!outside.join(".synapse").exists());
    let guide = client.tool("writeknowledge", json!({"project":project,
        "name":"overview", "description":"Rust fixture", "content":"# Fixture\nUses Rust; see Cargo.toml.",
        "sources":["Cargo.toml"]}));
    assert_eq!(guide["status"], "current");
    let page = client.tool("knowledge", json!({"project":project,"name":"overview"}));
    assert!(
        page["page"]["content"]
            .as_str()
            .unwrap()
            .contains("Uses Rust")
    );
    assert_eq!(page["page"]["guide"]["revision"], guide["revision"]);
    let recalled = client.tool("recall", json!({"query":"", "project":project}));
    assert_eq!(recalled["knowledge"]["guides"][0]["name"], "overview");
    let startup: Value = serde_json::from_str(&run(root, &["session"])).unwrap();
    let context = startup["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("Project Knowledge"));
    assert!(context.contains("Uses Rust"));
    assert!(
        !startup["systemMessage"]
            .as_str()
            .unwrap()
            .contains("Knowledge")
    );
    std::fs::write(project.join("Cargo.toml"), "changed").unwrap();
    let stale = client.tool("knowledge", json!({"project":project}));
    assert_eq!(stale["catalog"]["guides"][0]["status"], "needsreview");
    let path = project.join(".synapse/knowledge/overview.md");
    let original = std::fs::read_to_string(&path).unwrap();
    let edited = format!("{original}\nManual edit.");
    std::fs::write(&path, &edited).unwrap();
    let rejected = client.call(
        "tools/call",
        json!({"name":"writeknowledge","arguments":{
        "project":project, "name":"overview", "description":"replacement", "content":"replacement",
        "sources":["Cargo.toml"], "revision":guide["revision"]}}),
    );
    assert_eq!(rejected["result"]["isError"], true);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
    run(root, &["settings", "knowledge", "off"]);
    let rejected = client.call(
        "tools/call",
        json!({"name":"knowledge","arguments":{"project":project}}),
    );
    assert_eq!(rejected["result"]["isError"], true);
    let rejected = client.call("tools/call", json!({"name":"writeknowledge","arguments":{
        "project":project,"name":"database","description":"db","content":"db", "sources":["Cargo.toml"]}}));
    assert_eq!(rejected["result"]["isError"], true);
    assert!(
        client
            .tool("recall", json!({"query":"", "project":project}))
            .get("knowledge")
            .is_none()
    );
    let startup: Value = serde_json::from_str(&run(root, &["session"])).unwrap();
    assert!(
        !startup["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("Project Knowledge")
    );
    assert!(path.exists());
    drop(client);
    let mut disabled = Client::open(root);
    let listed = disabled.call("tools/list", json!({}));
    assert!(!listed.to_string().contains("writeknowledge"));
}
