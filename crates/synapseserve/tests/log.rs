//! The server driven through its real HTTP surface.
//!
//! Most of these go through the router directly, which is faster and just as
//! honest about the handlers. The last one binds a real ephemeral port,
//! because `serve` taking an already-bound listener is the thing that lets a
//! test do that at all.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;
use synapseserve::{State, auth::Token, http::router, store::Store};
use synapsesync::wire::PROTOCOL;
use synapsesync::{Key, Op, Record, Scope, opid, seal};
use tempfile::TempDir;
use tower::ServiceExt;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

async fn server() -> (Router, TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("log.db")).await.unwrap();
    let state = State {
        store: Arc::new(store),
        token: Arc::new(Token::new(TOKEN).unwrap()),
    };
    (router(state), directory)
}

async fn call(
    router: &Router,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn key() -> Key {
    Key::new([7; 32])
}

fn memory(body: &str) -> Record {
    Record::Put {
        body: body.into(),
        source: "session".into(),
        scope: Scope::Project,
        project: "github.com/wess/synapse".into(),
        created: 1_700_000_000,
    }
}

fn sealed(record: &Record) -> Op {
    let opid = opid(record);
    let envelope = seal(&key(), &opid, record).unwrap();
    Op { opid, envelope }
}

fn push(ops: &[Op]) -> Value {
    json!({ "protocol": PROTOCOL, "ops": ops })
}

fn pull(since: i64, limit: u32) -> Value {
    json!({ "protocol": PROTOCOL, "since": since, "limit": limit })
}

#[tokio::test]
async fn a_pushed_operation_comes_back_on_the_next_pull() {
    let (router, _directory) = server().await;
    let op = sealed(&memory("uses bun"));

    let (status, body) = call(
        &router,
        "/push",
        Some(TOKEN),
        push(std::slice::from_ref(&op)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], 1);

    let (status, body) = call(&router, "/pull", Some(TOKEN), pull(0, 10)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ops"].as_array().unwrap().len(), 1);
    assert_eq!(body["ops"][0]["opid"], op.opid);
    assert_eq!(body["more"], false);

    // And it opens again on the far side, which is the only thing that matters.
    let envelope = body["ops"][0]["envelope"].as_str().unwrap();
    use base64::{Engine, engine::general_purpose::STANDARD};
    let bytes = STANDARD.decode(envelope).unwrap();
    assert_eq!(
        synapsesync::open(&key(), &op.opid, &bytes).unwrap(),
        memory("uses bun")
    );
}

#[tokio::test]
async fn a_repeated_push_accepts_nothing_the_second_time() {
    let (router, _directory) = server().await;
    let op = sealed(&memory("uses bun"));

    let (_, first) = call(
        &router,
        "/push",
        Some(TOKEN),
        push(std::slice::from_ref(&op)),
    )
    .await;
    let (_, second) = call(&router, "/push", Some(TOKEN), push(&[op])).await;
    assert_eq!(first["accepted"], 1);
    assert_eq!(second["accepted"], 0, "a retry must not add a second row");
    assert_eq!(first["head"], second["head"], "and must not move the head");

    let (_, body) = call(&router, "/pull", Some(TOKEN), pull(0, 10)).await;
    assert_eq!(body["ops"].as_array().unwrap().len(), 1);
}

/// The hazard the operation identity exists to close. A client whose push
/// response was lost repeats it; that repeat must not resurrect a memory
/// somebody deleted in the meantime.
#[tokio::test]
async fn a_replayed_put_cannot_overwrite_a_delete() {
    let (router, _directory) = server().await;
    let stored = memory("uses bun");
    let removed = Record::Del {
        uid: synapsesync::uid(&stored),
        at: 1_700_000_900,
    };

    call(&router, "/push", Some(TOKEN), push(&[sealed(&stored)])).await;
    call(&router, "/push", Some(TOKEN), push(&[sealed(&removed)])).await;
    // The client never saw the first response and sends it again.
    call(&router, "/push", Some(TOKEN), push(&[sealed(&stored)])).await;

    let (_, body) = call(&router, "/pull", Some(TOKEN), pull(0, 10)).await;
    let ops = body["ops"].as_array().unwrap();
    assert_eq!(ops.len(), 2, "the put and the delete are separate rows");

    // Both are still readable, so the client can settle them by timestamp.
    use base64::{Engine, engine::general_purpose::STANDARD};
    let kinds: Vec<Record> = ops
        .iter()
        .map(|op| {
            let opid = op["opid"].as_str().unwrap();
            let bytes = STANDARD.decode(op["envelope"].as_str().unwrap()).unwrap();
            synapsesync::open(&key(), opid, &bytes).unwrap()
        })
        .collect();
    assert!(kinds.iter().any(|item| matches!(item, Record::Put { .. })));
    assert!(kinds.iter().any(|item| matches!(item, Record::Del { .. })));
    // The deletion is the later one, so it wins on the client.
    let put = kinds
        .iter()
        .find(|item| matches!(item, Record::Put { .. }))
        .unwrap();
    let del = kinds
        .iter()
        .find(|item| matches!(item, Record::Del { .. }))
        .unwrap();
    assert!(del.at() > put.at());
}

#[tokio::test]
async fn a_pull_pages_and_says_when_more_remain() {
    let (router, _directory) = server().await;
    let ops: Vec<Op> = (0..5)
        .map(|index| sealed(&memory(&format!("memory {index}"))))
        .collect();
    call(&router, "/push", Some(TOKEN), push(&ops)).await;

    let (_, first) = call(&router, "/pull", Some(TOKEN), pull(0, 2)).await;
    assert_eq!(first["ops"].as_array().unwrap().len(), 2);
    assert_eq!(first["more"], true);

    let cursor = first["ops"][1]["seq"].as_i64().unwrap();
    let (_, second) = call(&router, "/pull", Some(TOKEN), pull(cursor, 2)).await;
    assert_eq!(second["ops"].as_array().unwrap().len(), 2);
    assert_eq!(second["more"], true);

    let cursor = second["ops"][1]["seq"].as_i64().unwrap();
    let (_, third) = call(&router, "/pull", Some(TOKEN), pull(cursor, 2)).await;
    assert_eq!(third["ops"].as_array().unwrap().len(), 1);
    assert_eq!(third["more"], false, "the last page says so");

    // A cursor at the head returns nothing rather than repeating the tail.
    let cursor = third["ops"][0]["seq"].as_i64().unwrap();
    let (_, empty) = call(&router, "/pull", Some(TOKEN), pull(cursor, 2)).await;
    assert!(empty["ops"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_request_without_a_usable_token_is_refused() {
    let (router, _directory) = server().await;
    let op = sealed(&memory("uses bun"));

    for token in [
        None,
        Some("wrong"),
        Some("0123456789abcdef0123456789abcdeg"),
    ] {
        let (status, _) = call(&router, "/push", token, push(std::slice::from_ref(&op))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "token: {token:?}");
        let (status, _) = call(&router, "/pull", token, pull(0, 10)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "token: {token:?}");
    }

    // And nothing was stored on the way past.
    let (_, body) = call(&router, "/pull", Some(TOKEN), pull(0, 10)).await;
    assert!(body["ops"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_client_from_another_protocol_is_told_which_side_to_upgrade() {
    let (router, _directory) = server().await;
    let body = json!({ "protocol": PROTOCOL + 1, "since": 0, "limit": 10 });
    let (status, body) = call(&router, "/pull", Some(TOKEN), body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = body["error"].as_str().unwrap();
    assert!(
        message.contains(&(PROTOCOL + 1).to_string()),
        "got: {message}"
    );
    assert!(message.contains(&PROTOCOL.to_string()), "got: {message}");
}

#[tokio::test]
async fn a_malformed_operation_never_reaches_the_log() {
    let (router, _directory) = server().await;
    let good = sealed(&memory("uses bun"));

    let cases = [
        json!({ "opid": "short", "envelope": "AAEC" }),
        json!({ "opid": "../../etc/passwd", "envelope": "AAEC" }),
        json!({ "opid": "A".repeat(64), "envelope": "AAEC" }),
        json!({ "opid": good.opid, "envelope": "" }),
    ];
    for case in cases {
        let (status, _) = call(
            &router,
            "/push",
            Some(TOKEN),
            json!({ "protocol": PROTOCOL, "ops": [case.clone()] }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "case: {case}");
    }

    let (_, body) = call(&router, "/pull", Some(TOKEN), pull(0, 10)).await;
    assert!(body["ops"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_log_holds_nothing_the_server_could_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("log.db");
    let store = Store::open(&path).await.unwrap();
    let state = State {
        store: Arc::new(store),
        token: Arc::new(Token::new(TOKEN).unwrap()),
    };
    let router = router(state);

    let secret = "the production database password rotates on fridays";
    call(
        &router,
        "/push",
        Some(TOKEN),
        push(&[sealed(&memory(secret))]),
    )
    .await;

    let mut bytes = Vec::new();
    for suffix in ["", "-wal"] {
        let item = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        if item.is_file() {
            bytes.extend_from_slice(&std::fs::read(&item).unwrap());
        }
    }
    let haystack = String::from_utf8_lossy(&bytes);
    assert!(!haystack.contains(secret), "the body reached the disk");
    assert!(
        !haystack.contains("github.com/wess/synapse"),
        "the project reached the disk"
    );
    assert!(!haystack.contains("\"put\""), "the kind reached the disk");
}

/// `serve` takes a bound listener so a test can use port 0 and read the
/// address back. This is the test that proves it.
#[tokio::test]
async fn the_server_answers_over_a_real_socket() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("log.db")).await.unwrap();
    let state = State {
        store: Arc::new(store),
        token: Arc::new(Token::new(TOKEN).unwrap()),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { synapseserve::serve(listener, state).await });

    let body = pull(0, 10).to_string();
    let request = format!(
        "POST /pull HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {TOKEN}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    assert!(response.starts_with("HTTP/1.1 200 OK"), "got: {response}");
    let (_, payload) = response.split_once("\r\n\r\n").unwrap();
    let value: Value = serde_json::from_str(payload.trim()).unwrap();
    assert_eq!(value["head"], 0);
    assert!(value["ops"].as_array().unwrap().is_empty());
}
