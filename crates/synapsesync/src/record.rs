//! The plaintext inside an envelope. The server never sees this type.
//!
//! Whether an operation stored a memory or removed one is part of the sealed
//! payload rather than the wire, so a server that logged every request it
//! received still could not say how large anyone's memory store is or when
//! they deleted something.

use serde::{Deserialize, Serialize};

/// Which memories a record belongs to, mirroring `brain`'s own scopes.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Project,
}

impl Scope {
    /// The wire spelling, matching `brain`'s own. The identity derivation
    /// hashes this, so it is part of the format rather than a display detail.
    pub fn value(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

/// One operation, as the client understands it.
///
/// A memory's identity is derived from its content (see [`crate::op::uid`]),
/// so there is no edit: changing a memory is a [`Record::Del`] of the old
/// identity and a [`Record::Put`] of the new one. That leaves exactly one
/// conflict for two devices to disagree about — whether a given identity is
/// present or removed — which the later `at`/`created` wins.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Record {
    Put {
        body: String,
        source: String,
        scope: Scope,
        /// Stable project identity, never a local absolute path. Two machines
        /// check the same repository out at different paths, and a path-keyed
        /// project silently recalls nothing on the second one.
        project: String,
        created: i64,
    },
    Del {
        at: i64,
    },
}

impl Record {
    /// When this record was made, whichever variant it is. Two devices that
    /// disagree about one identity settle it with this.
    pub fn at(&self) -> i64 {
        match self {
            Self::Put { created, .. } => *created,
            Self::Del { at } => *at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put() -> Record {
        Record::Put {
            body: "prefers bun over npm".into(),
            source: "session".into(),
            scope: Scope::Project,
            project: "github.com/wess/synapse".into(),
            created: 1_700_000_000,
        }
    }

    #[test]
    fn a_record_survives_a_round_trip_through_json() {
        let encoded = serde_json::to_vec(&put()).unwrap();
        assert_eq!(serde_json::from_slice::<Record>(&encoded).unwrap(), put());
    }

    #[test]
    fn both_variants_report_when_they_happened() {
        assert_eq!(put().at(), 1_700_000_000);
        assert_eq!(Record::Del { at: 42 }.at(), 42);
    }
}
