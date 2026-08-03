//! What the server stores, and how a memory gets an identity that means the
//! same thing on every machine.

use crate::record::Record;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One operation as it goes over the wire. Both fields are opaque to the
/// server: it dedupes on `opid` and hands `envelope` back untouched.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Op {
    pub opid: String,
    #[serde(with = "b64")]
    pub envelope: Vec<u8>,
}

/// An operation the server has accepted and numbered.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Numbered {
    pub seq: i64,
    pub opid: String,
    #[serde(with = "b64")]
    pub envelope: Vec<u8>,
}

/// The identity a memory has on every machine that stores it.
///
/// Derived from the content rather than assigned, so the same memory written
/// on two devices converges to one row instead of two, and an import that runs
/// twice stores nothing the second time. The local `rowid` cannot do this job:
/// it is a per-database counter, and two machines both mint `42` for different
/// memories on their first write.
///
/// Fields are length-prefixed before hashing so that no two different memories
/// can hash the same bytes by moving a delimiter into a field.
pub fn uid(record: &Record) -> String {
    let (body, source, scope, project, created) = match record {
        Record::Put {
            body,
            source,
            scope,
            project,
            created,
        } => (body, source, scope, project, created),
        // A deletion names an identity that already exists rather than minting one.
        Record::Del { uid, .. } => return uid.clone(),
    };

    let mut hasher = Sha256::new();
    hasher.update(b"synapse.memory.v1");
    for field in [
        scope.value().as_bytes(),
        project.as_bytes(),
        source.as_bytes(),
        body.as_bytes(),
    ] {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field);
    }
    hasher.update(created.to_le_bytes());
    digest(hasher)
}

/// What the server files this operation under.
///
/// Distinct for the put and the delete of one memory, so both survive in the
/// log and the client settles them by their sealed timestamps. Keying the log
/// on the memory identity instead would force the server to choose between two
/// broken options: replace, and a retried put silently resurrects a memory
/// somebody deleted; or append, and every retry grows the log forever.
///
/// It is still a digest, so the server learns no more from it than from the
/// memory identity — including whether this operation stored something or
/// removed it.
pub fn opid(record: &Record) -> String {
    let kind = match record {
        Record::Put { .. } => "put",
        Record::Del { .. } => "del",
    };
    let uid = uid(record);

    let mut hasher = Sha256::new();
    hasher.update(b"synapse.op.v1");
    for field in [kind.as_bytes(), uid.as_bytes()] {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field);
    }
    digest(hasher)
}

fn digest(hasher: Sha256) -> String {
    format!("{:x}", hasher.finalize())
}

/// Envelopes are bytes, and a byte array in JSON is a list of numbers roughly
/// six times the size it needs to be.
mod b64 {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Scope;

    fn put(body: &str, project: &str, created: i64) -> Record {
        Record::Put {
            body: body.into(),
            source: "session".into(),
            scope: Scope::Project,
            project: project.into(),
            created,
        }
    }

    #[test]
    fn the_same_memory_gets_the_same_identity_twice() {
        let first = uid(&put("uses bun", "github.com/wess/synapse", 10));
        let second = uid(&put("uses bun", "github.com/wess/synapse", 10));
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn changing_any_field_changes_the_identity() {
        let base = uid(&put("uses bun", "github.com/wess/synapse", 10));
        assert_ne!(base, uid(&put("uses npm", "github.com/wess/synapse", 10)));
        assert_ne!(base, uid(&put("uses bun", "github.com/wess/guise", 10)));
        assert_ne!(base, uid(&put("uses bun", "github.com/wess/synapse", 11)));

        let global = Record::Put {
            body: "uses bun".into(),
            source: "session".into(),
            scope: Scope::Global,
            project: "github.com/wess/synapse".into(),
            created: 10,
        };
        assert_ne!(base, uid(&global));
    }

    #[test]
    fn fields_cannot_be_slid_across_the_boundary_between_them() {
        // Without length prefixes these two hash identical bytes.
        let left = Record::Put {
            body: "b".into(),
            source: "a".into(),
            scope: Scope::Global,
            project: String::new(),
            created: 0,
        };
        let right = Record::Put {
            body: String::new(),
            source: "ab".into(),
            scope: Scope::Global,
            project: String::new(),
            created: 0,
        };
        assert_ne!(uid(&left), uid(&right));
    }

    #[test]
    fn a_deletion_reports_the_identity_it_removes() {
        let memory = uid(&put("uses bun", "github.com/wess/synapse", 10));
        let removal = Record::Del {
            uid: memory.clone(),
            at: 20,
        };
        assert_eq!(uid(&removal), memory);
    }

    /// The reason the log is keyed on the operation rather than the memory: a
    /// retried put must not be able to overwrite somebody's deletion.
    #[test]
    fn storing_and_removing_one_memory_are_filed_separately() {
        let stored = put("uses bun", "github.com/wess/synapse", 10);
        let removed = Record::Del {
            uid: uid(&stored),
            at: 20,
        };
        assert_eq!(uid(&stored), uid(&removed));
        assert_ne!(opid(&stored), opid(&removed));
    }

    #[test]
    fn the_same_operation_is_filed_under_the_same_name_twice() {
        let first = opid(&put("uses bun", "github.com/wess/synapse", 10));
        let second = opid(&put("uses bun", "github.com/wess/synapse", 10));
        assert_eq!(first, second, "a retry must not add a second row");
    }

    /// Pins both derivations. Every device that has ever stored a memory agrees
    /// on these digests, so a refactor that changes them silently re-imports
    /// every memory on every machine as a new row.
    #[test]
    fn the_derivations_are_pinned() {
        let record = put("uses bun", "github.com/wess/synapse", 10);
        assert_eq!(
            uid(&record),
            "5c79e8185f72018aa0f5d46e34d37a9f067103c58fc4a7c8c61fa776b078ee5f"
        );
        assert_eq!(
            opid(&record),
            "179e53addd463bcd0661e70b76e42ba01807c33fc9c4a36a399ba070c394882a"
        );
    }

    #[test]
    fn an_op_carries_its_envelope_through_json_as_base64() {
        let op = Op {
            opid: "abc".into(),
            envelope: vec![0, 1, 250, 255],
        };
        let text = serde_json::to_string(&op).unwrap();
        assert!(text.contains("\"AAH6/w==\""), "got: {text}");
        assert_eq!(serde_json::from_str::<Op>(&text).unwrap(), op);
    }
}
