//! The request and response shapes, and the number that governs whether a
//! client and a server can talk at all.

use crate::op::{Numbered, Op};
use serde::{Deserialize, Serialize};

/// What this build speaks.
///
/// Bump deliberately, and only for a change a peer cannot ignore. This is not
/// derived from any crate version: the desktop app and the server release on
/// separate schedules, and an old client has to keep working against a server
/// that has moved on. [`crate::envelope::VERSION`] covers the sealed bytes
/// separately, because those can change without the wire changing.
pub const PROTOCOL: u32 = 1;

/// Most operations a single request may carry. A push larger than this is
/// split into several, so one request can never be unboundedly large.
pub const MAXOPS: usize = 500;

/// Largest envelope the server will accept. Memories are prose; anything near
/// this is a mistake somewhere upstream.
pub const MAXENVELOPE: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PushRequest {
    pub protocol: u32,
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PushResponse {
    /// Operations that were new. A push the client repeats after a dropped
    /// response accepts nothing the second time and is not an error.
    pub accepted: usize,
    /// The highest sequence number the server holds.
    pub head: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PullRequest {
    pub protocol: u32,
    /// The last sequence number this client applied. Zero pulls everything.
    pub since: i64,
    pub limit: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PullResponse {
    pub ops: Vec<Numbered>,
    pub head: i64,
    /// Whether more operations remain past the last one returned. The client
    /// advances its cursor and asks again rather than assuming a short page
    /// means the end.
    pub more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_survives_a_round_trip() {
        let request = PushRequest {
            protocol: PROTOCOL,
            ops: vec![Op {
                opid: "abc".into(),
                envelope: vec![1, 2, 3],
            }],
        };
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<PushRequest>(&text).unwrap(), request);
    }

    #[test]
    fn a_pull_survives_a_round_trip() {
        let response = PullResponse {
            ops: vec![Numbered {
                seq: 9,
                opid: "abc".into(),
                envelope: vec![4, 5],
            }],
            head: 9,
            more: false,
        };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<PullResponse>(&text).unwrap(),
            response
        );
    }
}
