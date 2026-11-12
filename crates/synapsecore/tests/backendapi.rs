use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};
use synapsecore::backend::Backend;
use synapsecore::backend::api::{Request, handle};

#[tokio::test]
async fn jsonl_api_correlates_requests_and_keeps_errors_on_the_stream() {
    let root = tempfile::tempdir().unwrap();
    let backend = Backend::open(root.path().join("brain.db")).await.unwrap();
    let requests = [
        "not json".to_owned(),
        json!({
          "version": 1,
          "id": "create",
          "method": "runs.create",
          "params": { "project": "demo", "harness": "test", "input": { "x": 1 } }
        })
        .to_string(),
        json!({
          "version": 1,
          "id": "claim",
          "method": "runs.claim",
          "params": { "worker": "worker-one", "lease": 30 }
        })
        .to_string(),
        json!({
          "version": 9,
          "id": "future",
          "method": "summary",
          "params": {}
        })
        .to_string(),
    ]
    .join("\n");
    let mut output = Vec::new();

    synapsecore::backend::api::serve(&backend, std::io::Cursor::new(requests), &mut output)
        .await
        .unwrap();

    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 4);
    assert!(
        responses[0]["error"]
            .as_str()
            .unwrap()
            .contains("invalid request")
    );
    assert_eq!(responses[1]["id"], "create");
    assert_eq!(responses[1]["result"]["status"], "queued");
    assert_eq!(responses[2]["id"], "claim");
    assert_eq!(responses[2]["result"]["status"], "running");
    assert_eq!(responses[2]["result"]["leaseowner"], "worker-one");
    assert!(
        responses[3]["error"]
            .as_str()
            .unwrap()
            .contains("unsupported API version")
    );
}

#[tokio::test]
async fn jsonl_rejects_an_oversized_line_and_reads_the_next_request() {
    let root = tempfile::tempdir().unwrap();
    let backend = Backend::open(root.path().join("brain.db")).await.unwrap();
    let mut requests = vec![b'x'; 1024 * 1024];
    requests.push(b'\n');
    requests.extend(std::iter::repeat_n(b'y', 1024 * 1024 + 1));
    requests.push(b'\n');
    requests.extend_from_slice(br#"{"version":1,"id":"after","method":"summary","params":{}}"#);
    let mut output = Vec::new();
    synapsecore::backend::api::serve(&backend, std::io::Cursor::new(requests), &mut output)
        .await
        .unwrap();
    let responses = String::from_utf8(output).unwrap();
    let rows = responses.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 3);
    assert!(rows[0].contains("request line is too large"));
    assert!(rows[1].contains("request line is too large"));
    assert_eq!(
        serde_json::from_str::<Value>(rows[2]).unwrap()["id"],
        "after"
    );
}

#[tokio::test]
async fn jsonl_refuses_to_expand_a_large_artifact() {
    let root = tempfile::tempdir().unwrap();
    let backend = Backend::open(root.path().join("brain.db")).await.unwrap();
    let artifact = backend
        .putartifact(
            "large",
            "output",
            "application/octet-stream",
            &vec![1; 4 * 1024 * 1024 + 1],
        )
        .await
        .unwrap();
    let response = handle(
        &backend,
        Request {
            version: 1,
            id: json!("read"),
            method: "artifacts.read".to_owned(),
            params: json!({"digest": artifact.digest}),
        },
    )
    .await;
    assert!(response.error.unwrap().contains("too large for JSONL"));
}

#[tokio::test]
async fn jsonl_exposes_collaboration_and_runner_records() {
    let root = tempfile::tempdir().unwrap();
    let backend = Backend::open(root.path().join("brain.db")).await.unwrap();
    let run = backend
        .createrun("demo", "test", json!({"task": "build"}))
        .await
        .unwrap();
    let requests = [
        json!({
          "version": 1,
          "id": "change",
          "method": "changes.record",
          "params": {
            "runid": run.id,
            "path": "src/main.rs",
            "operation": "modify",
            "summary": "Handle the empty state"
          }
        }),
        json!({
          "version": 1,
          "id": "review",
          "method": "reviews.submit",
          "params": {
            "runid": run.id,
            "reviewer": "reviewer",
            "verdict": "approved",
            "summary": "Ready",
            "checks": [{"name": "test", "status": "passed"}]
          }
        }),
        json!({
          "version": 1,
          "id": "runner",
          "method": "runners.put",
          "params": {
            "id": "local-one",
            "name": "Local runner",
            "capabilities": ["shell", "preview"]
          }
        }),
    ]
    .map(|request| request.to_string())
    .join("\n");
    let mut output = Vec::new();
    synapsecore::backend::api::serve(&backend, std::io::Cursor::new(requests), &mut output)
        .await
        .unwrap();
    let responses = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses[0]["result"]["path"], "src/main.rs");
    assert_eq!(responses[1]["result"]["verdict"], "approved");
    assert_eq!(responses[2]["result"]["status"], "online");
}

#[test]
fn backend_cli_emits_and_queries_events() {
    let root = tempfile::tempdir().unwrap();
    let draft = json!({
      "source": "test",
      "project": "demo",
      "agent": "worker-one",
      "session": "session-one",
      "level": "info",
      "kind": "test.event",
      "message": "ready",
      "fields": { "ok": true },
      "runid": "",
      "traceid": ""
    });
    let emitted = command(root.path(), &["backend", "emit"], Some(&draft.to_string()));
    assert!(
        emitted.status.success(),
        "{}",
        String::from_utf8_lossy(&emitted.stderr)
    );

    let listed = command(
        root.path(),
        &["backend", "events", "--project", "demo"],
        None,
    );
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let events: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(events.as_array().unwrap().len(), 1);
    assert_eq!(events[0]["kind"], "test.event");
    assert_eq!(events[0]["message"], "ready");
}

fn command(
    root: &std::path::Path,
    arguments: &[&str],
    input: Option<&str>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_synapse-cli"));
    command
        .env("SYNAPSE_HOME", root.join("home"))
        .env("SYNAPSE_DATA", root.join("data"))
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
