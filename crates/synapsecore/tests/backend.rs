use serde_json::json;
use synapsecore::backend::{
    Backend, Decision, EventDraft, EventFilter, Level, RunStatus, StepStatus,
};

async fn backend() -> (tempfile::TempDir, Backend) {
    let folder = tempfile::tempdir().unwrap();
    let backend = Backend::open(folder.path().join("brain.db")).await.unwrap();
    (folder, backend)
}

#[tokio::test]
async fn events_are_redacted_filtered_and_cursor_readable() {
    let (_folder, backend) = backend().await;
    let first = backend
        .record(EventDraft {
            source: "harness".to_owned(),
            project: "/work".to_owned(),
            agent: "builder".to_owned(),
            level: Level::Info,
            kind: "tool.start".to_owned(),
            message: "starting TOKEN=secret-value".to_owned(),
            fields: json!({"authorization": "Bearer abcdefghijklmnopqrstuvwxyz"}),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(first.message.contains("[redacted]"));
    assert!(!first.fields.contains("abcdefghijklmnopqrstuvwxyz"));

    let second = backend
        .record(EventDraft {
            source: "worker".to_owned(),
            project: "/work".to_owned(),
            agent: "reviewer".to_owned(),
            level: Level::Error,
            kind: "worker.exit".to_owned(),
            message: "process failed".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let rows = backend
        .events(&EventFilter {
            after: first.id,
            project: Some("/work".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rows, [second]);
}

#[tokio::test]
async fn runs_steps_artifacts_replay_and_evaluations_share_identity() {
    let (folder, backend) = backend().await;
    let run = backend
        .createrun("/work", "test", json!({"task": "build"}))
        .await
        .unwrap();
    let step = backend
        .addstep(&run.id, "compile", json!({}))
        .await
        .unwrap();
    backend
        .updatestep(step.id, StepStatus::Running, "", "")
        .await
        .unwrap();
    backend
        .updatestep(step.id, StepStatus::Succeeded, "clean", "")
        .await
        .unwrap();
    backend
        .updaterun(&run.id, RunStatus::Succeeded, "done", "")
        .await
        .unwrap();

    let artifact = backend
        .putartifact("TOKEN=secret-value", "report", "text/plain", b"result")
        .await
        .unwrap();
    assert!(artifact.name.contains("[redacted]"));
    backend
        .attach(&run.id, &artifact.digest, "output")
        .await
        .unwrap();
    assert!(
        folder
            .path()
            .join("artifacts")
            .join(&artifact.digest[..2])
            .join(&artifact.digest)
            .is_file()
    );

    let replay = backend.replay(&run.id).await.unwrap();
    assert_eq!(replay.replayof, run.id);
    assert_ne!(replay.id, replay.replayof);
    let evaluation = backend
        .evaluate(&replay.replayof, "done", "done", 1.0, "matches")
        .await
        .unwrap();
    assert_eq!(evaluation.score, 1.0);
}

#[tokio::test]
async fn capabilities_default_deny_and_digest_bind_approval() {
    let (_folder, backend) = backend().await;
    assert!(
        !backend
            .allowed("agent", "command", "cargo test", "v1")
            .await
            .unwrap()
    );
    backend
        .decide("agent", "command", "cargo test", Decision::Allow, "v1")
        .await
        .unwrap();
    assert!(
        backend
            .allowed("agent", "command", "cargo test", "v1")
            .await
            .unwrap()
    );
    assert!(
        !backend
            .allowed("agent", "command", "cargo test", "v2")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn triggers_routes_and_selection_are_persisted() {
    let (_folder, backend) = backend().await;
    backend
        .puttrigger("nightly", "schedule", json!({"every": 86400}), 10)
        .await
        .unwrap();
    assert_eq!(backend.due(10).await.unwrap()[0].name, "nightly");
    backend.marktrigger("nightly", 20).await.unwrap();
    assert!(backend.due(10).await.unwrap().is_empty());

    backend
        .putroute("local", "http://127.0.0.1:11434", "default", 10, true)
        .await
        .unwrap();
    backend
        .putroute(
            "remote",
            "https://models.example.test",
            "fallback",
            20,
            false,
        )
        .await
        .unwrap();
    assert_eq!(
        backend.selectroute(Some(true)).await.unwrap().unwrap().name,
        "local"
    );
    assert!(
        backend
            .putroute("bad", "http://example.test", "", 0, false)
            .await
            .is_err()
    );
    assert!(
        backend
            .putroute("secret", "https://user:secret@example.test", "", 0, false)
            .await
            .is_err()
    );
    assert!(
        backend
            .putroute("fragment", "https://example.test/#token", "", 0, false)
            .await
            .is_err()
    );
    assert!(
        backend
            .putroute("query", "https://example.test/?token=secret", "", 0, false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn runs_can_be_claimed_renewed_retried_and_cancelled() {
    let (_folder, backend) = backend().await;
    let run = backend
        .createrun("/work", "worker", json!({}))
        .await
        .unwrap();
    let claimed = backend.claim("worker-1", 30).await.unwrap().unwrap();
    assert_eq!(claimed.id, run.id);
    assert_eq!(claimed.status, RunStatus::Running);
    assert_eq!(claimed.attempts, 1);
    assert!(backend.renew(&run.id, "worker-1", 30).await.unwrap());
    backend
        .updaterun(&run.id, RunStatus::Failed, "", "failed")
        .await
        .unwrap();
    assert_eq!(
        backend.retry(&run.id).await.unwrap().status,
        RunStatus::Queued
    );
    assert_eq!(
        backend.cancel(&run.id).await.unwrap().status,
        RunStatus::Cancelled
    );
}

#[tokio::test]
async fn tick_creates_runs_and_artifacts_verify_on_read() {
    let (_folder, backend) = backend().await;
    backend
        .puttrigger(
            "scheduled",
            "schedule",
            json!({"project":"/work","harness":"test","input":{"task":"check"},"every":60}),
            100,
        )
        .await
        .unwrap();
    let runs = backend.tick(100).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        backend.trigger("scheduled").await.unwrap().unwrap().next,
        160
    );
    let artifact = backend
        .putartifact("output", "result", "application/octet-stream", b"verified")
        .await
        .unwrap();
    backend
        .attach(&runs[0].id, &artifact.digest, "output")
        .await
        .unwrap();
    assert_eq!(
        backend.readartifact(&artifact.digest).await.unwrap(),
        b"verified"
    );
    assert_eq!(backend.artifacts(&runs[0].id).await.unwrap(), [artifact]);
}

#[tokio::test]
async fn artifact_reads_reject_files_that_grow_past_the_limit() {
    let (_folder, backend) = backend().await;
    let artifact = backend
        .putartifact("output", "result", "application/octet-stream", b"small")
        .await
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&artifact.path)
        .unwrap()
        .set_len(128 * 1024 * 1024 + 1)
        .unwrap();

    let error = backend.readartifact(&artifact.digest).await.unwrap_err();
    assert!(error.to_string().contains("too large"));
}

#[tokio::test]
async fn event_and_dependency_triggers_dispatch_once() {
    let (_folder, backend) = backend().await;
    backend
        .puttrigger(
            "on-failure",
            "event",
            json!({"eventkind":"worker.failed","project":"/work","harness":"repair"}),
            0,
        )
        .await
        .unwrap();
    backend
        .record(EventDraft {
            source: "worker".to_owned(),
            project: "/work".to_owned(),
            kind: "worker.failed".to_owned(),
            message: "failed".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(backend.tick(100).await.unwrap().len(), 1);
    assert!(backend.tick(101).await.unwrap().is_empty());

    let parent = backend
        .createrun("/work", "build", json!({}))
        .await
        .unwrap();
    backend
        .updaterun(&parent.id, RunStatus::Succeeded, "done", "")
        .await
        .unwrap();
    backend
        .puttrigger(
            "after-build",
            "dependency",
            json!({"run":parent.id,"status":"succeeded","project":"/work","harness":"publish"}),
            0,
        )
        .await
        .unwrap();
    assert_eq!(backend.tick(102).await.unwrap().len(), 1);
    assert!(backend.tick(103).await.unwrap().is_empty());
}

#[tokio::test]
async fn concurrent_ticks_claim_one_trigger_once() {
    let (_folder, backend) = backend().await;
    backend
        .puttrigger("once", "schedule", json!({"harness":"test"}), 100)
        .await
        .unwrap();
    let left = backend.clone();
    let right = backend.clone();
    let (left, right) = tokio::join!(left.tick(100), right.tick(100));
    assert_eq!(left.unwrap().len() + right.unwrap().len(), 1);
    assert_eq!(backend.runs(10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn routes_apply_the_configured_model_and_bound_chunked_responses() {
    let (_folder, routed) = backend().await;
    let (endpoint, received) = jsonserver();
    routed
        .putroute("local", &endpoint, "configured", 1, true)
        .await
        .unwrap();
    assert_eq!(
        routed
            .callroute(Some(true), json!({"prompt":"hello"}))
            .await
            .unwrap(),
        json!({"ok":true})
    );
    let request = received.recv().unwrap();
    assert!(request.contains("\"model\":\"configured\""));

    let (_folder, routed) = backend().await;
    let endpoint = oversizedserver();
    routed
        .putroute("large", &endpoint, "", 1, true)
        .await
        .unwrap();
    let error = routed.callroute(Some(true), json!({})).await.unwrap_err();
    assert!(error.to_string().contains("response is too large"));
}

fn jsonserver() -> (String, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..read]);
            let Some(headers) = request.windows(4).position(|part| part == b"\r\n\r\n") else {
                continue;
            };
            let header = String::from_utf8_lossy(&request[..headers]);
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if request.len() >= headers + 4 + length {
                break;
            }
        }
        send.send(String::from_utf8_lossy(&request).into_owned())
            .unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}")
            .unwrap();
    });
    (endpoint, receive)
}

fn oversizedserver() -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let _ = stream.read(&mut request);
        if stream
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n")
            .is_err()
        {
            return;
        }
        let chunk = vec![b' '; 64 * 1024];
        for _ in 0..257 {
            if write!(stream, "{:x}\r\n", chunk.len()).is_err()
                || stream.write_all(&chunk).is_err()
                || stream.write_all(b"\r\n").is_err()
            {
                return;
            }
        }
        let _ = stream.write_all(b"0\r\n\r\n");
    });
    endpoint
}
