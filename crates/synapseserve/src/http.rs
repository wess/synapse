//! Two endpoints and a health check.
//!
//! Every handler validates before it touches the log: the client is not
//! trusted to have honoured the protocol's own bounds, and `opid` becomes a
//! primary key.

use crate::{State, auth};
use axum::extract::{DefaultBodyLimit, State as Extract};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use synapsesync::wire::{MAXENVELOPE, MAXOPS, PROTOCOL};
use synapsesync::{Op, PullRequest, PullResponse, PushRequest, PushResponse};

/// Largest request body accepted. A client whose operations do not fit sends
/// them in more than one push.
const REQUESTBYTES: usize = 8 * 1024 * 1024;

pub fn router(state: State) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/push", post(push))
        .route("/pull", post(pull))
        .layer(DefaultBodyLimit::max(REQUESTBYTES))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn push(
    Extract(state): Extract<State>,
    headers: HeaderMap,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>, Failure> {
    let tenant = authorize(&state, &headers).await?;
    protocol(request.protocol)?;
    if request.ops.len() > MAXOPS {
        return Err(Failure::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("a push carries at most {MAXOPS} operations"),
        ));
    }
    for op in &request.ops {
        validate(op)?;
    }

    let (accepted, head) = state
        .store
        .push(tenant, &request.ops)
        .await
        .map_err(Failure::from)?;
    Ok(Json(PushResponse { accepted, head }))
}

async fn pull(
    Extract(state): Extract<State>,
    headers: HeaderMap,
    Json(request): Json<PullRequest>,
) -> Result<Json<PullResponse>, Failure> {
    let tenant = authorize(&state, &headers).await?;
    protocol(request.protocol)?;
    if request.since < 0 {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "a cursor cannot be negative",
        ));
    }

    let (ops, head, more) = state
        .store
        .pull(tenant, request.since, request.limit)
        .await
        .map_err(Failure::from)?;
    Ok(Json(PullResponse { ops, head, more }))
}

/// Which log this request may touch.
///
/// The token no longer grants access to "the log" — it names one. Every read
/// and write below is scoped to what this returns, so a token that is not in
/// the tenant table reaches nothing rather than reaching everything.
async fn authorize(state: &State, headers: &HeaderMap) -> Result<i64, Failure> {
    let presented = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok());
    let token = auth::bearer(presented).ok_or_else(unauthorized)?;
    state
        .store
        .tenantfor(token)
        .await
        .map_err(Failure::from)?
        .ok_or_else(unauthorized)
}

fn unauthorized() -> Failure {
    // Deliberately the same answer for a missing token and an unknown one: the
    // difference would say whether a guess had named a real tenant.
    Failure::new(
        StatusCode::UNAUTHORIZED,
        "this request carried no usable access token",
    )
}

fn protocol(presented: u32) -> Result<(), Failure> {
    // Named in both directions so the reader knows which side to upgrade.
    (presented == PROTOCOL).then_some(()).ok_or_else(|| {
        Failure::new(
            StatusCode::BAD_REQUEST,
            format!("this client speaks protocol {presented} and this server speaks {PROTOCOL}"),
        )
    })
}

fn validate(op: &Op) -> Result<(), Failure> {
    // 64 lowercase hex, because it lands in a primary key and arrives from a
    // client this process has no reason to trust.
    let shaped = op.opid.len() == 64
        && op
            .opid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !shaped {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "an operation identifier is not a sha-256 digest",
        ));
    }
    if op.envelope.is_empty() {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "an operation carried an empty envelope",
        ));
    }
    if op.envelope.len() > MAXENVELOPE {
        return Err(Failure::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("an envelope is larger than {MAXENVELOPE} bytes"),
        ));
    }
    Ok(())
}

pub struct Failure {
    status: StatusCode,
    message: String,
}

impl Failure {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for Failure {
    /// Anything that reached here is this server's problem, not a description
    /// of it that a client should be handed.
    fn from(error: anyhow::Error) -> Self {
        eprintln!("synapseserve: {error:#}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the server could not complete this request",
        )
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}
