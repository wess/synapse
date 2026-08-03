//! What the server stores, and how a memory gets an identity that means the
//! same thing on every machine.

use crate::record::Record;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One operation as it goes over the wire. Both fields are opaque to the
/// server: it dedupes on `uid` and hands `envelope` back untouched.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Op {
    pub uid: String,
    #[serde(with = "b64")]
    pub envelope: Vec<u8>,
}

/// An operation the server has accepted and numbered.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Numbered {
    pub seq: i64,
    pub uid: String,
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
pub fn uid(record: &Record) -> Result<String> {
    let Record::Put {
        body,
        source,
        scope,
        project,
        created,
    } = record
    else {
        // A deletion names an identity that already exists; it does not mint one.
        return Err(anyhow!("a deletion carries the identity it removes"));
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
    Ok(format!("{:x}", hasher.finalize()))
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
        let first = uid(&put("uses bun", "github.com/wess/synapse", 10)).unwrap();
        let second = uid(&put("uses bun", "github.com/wess/synapse", 10)).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn changing_any_field_changes_the_identity() {
        let base = uid(&put("uses bun", "github.com/wess/synapse", 10)).unwrap();
        assert_ne!(
            base,
            uid(&put("uses npm", "github.com/wess/synapse", 10)).unwrap()
        );
        assert_ne!(
            base,
            uid(&put("uses bun", "github.com/wess/guise", 10)).unwrap()
        );
        assert_ne!(
            base,
            uid(&put("uses bun", "github.com/wess/synapse", 11)).unwrap()
        );

        let global = Record::Put {
            body: "uses bun".into(),
            source: "session".into(),
            scope: Scope::Global,
            project: "github.com/wess/synapse".into(),
            created: 10,
        };
        assert_ne!(base, uid(&global).unwrap());
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
        assert_ne!(uid(&left).unwrap(), uid(&right).unwrap());
    }

    #[test]
    fn a_deletion_has_no_identity_of_its_own() {
        assert!(uid(&Record::Del { at: 1 }).is_err());
    }

    /// Pins the derivation. Every device that has ever stored a memory agrees
    /// on these digests, so a refactor that changes them silently re-imports
    /// every memory on every machine as a new row.
    #[test]
    fn the_derivation_is_pinned() {
        assert_eq!(
            uid(&put("uses bun", "github.com/wess/synapse", 10)).unwrap(),
            "5c79e8185f72018aa0f5d46e34d37a9f067103c58fc4a7c8c61fa776b078ee5f"
        );
    }

    #[test]
    fn an_op_carries_its_envelope_through_json_as_base64() {
        let op = Op {
            uid: "abc".into(),
            envelope: vec![0, 1, 250, 255],
        };
        let text = serde_json::to_string(&op).unwrap();
        assert!(text.contains("\"AAH6/w==\""), "got: {text}");
        assert_eq!(serde_json::from_str::<Op>(&text).unwrap(), op);
    }
}
