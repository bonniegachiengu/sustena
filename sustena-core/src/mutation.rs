//! Mutation records — the alphabet the event log is written in.
//!
//! A typed enum rather than the loose dict Python uses. Same wire shape, so
//! the two engines read each other's logs and the conformance vectors compare
//! byte-for-byte; but an impossible record (a `remove` with no `item_id`, an
//! unknown `op`) cannot be constructed here at all.
//!
//! Wire shapes, matching `sustena/core/state.py` and `event_fold.py` exactly:
//!
//!   {"op": "set",          "path": str, "old": Any, "new": Any}
//!   {"op": "append",       "path": str, "item": obj, "item_id": str,
//!    "action": "append"}
//!   {"op": "remove",       "path": str, "item_id": str, "action": "remove"}
//!   {"op": "replace_root", "value": obj}
//!
//! `action` is a legacy duplicate of `op` that the Python records still carry.
//! It is emitted here too: parity means the serialized bytes match, not merely
//! that both engines happen to agree on meaning.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Mutation {
    Set {
        path: String,
        #[serde(default)]
        old: Value,
        new: Value,
    },
    Append {
        path: String,
        item_id: String,
        item: Value,
    },
    Remove {
        path: String,
        item_id: String,
    },
    /// Genesis marker: discard everything and start from `value`.
    /// By construction always the first event in a sustain's history.
    ReplaceRoot {
        value: Value,
    },
}

impl Mutation {
    /// The path this record touches, where it has one.
    pub fn path(&self) -> Option<&str> {
        match self {
            Mutation::Set { path, .. }
            | Mutation::Append { path, .. }
            | Mutation::Remove { path, .. } => Some(path),
            Mutation::ReplaceRoot { .. } => None,
        }
    }

    /// Serialize with the legacy `action` key that Python's records carry, so
    /// output is byte-identical to the reference engine's.
    pub fn to_wire(&self) -> Value {
        let mut v = serde_json::to_value(self).expect("Mutation is always serializable");
        match self {
            Mutation::Append { .. } => {
                v["action"] = Value::String("append".into());
            }
            Mutation::Remove { .. } => {
                v["action"] = Value::String("remove".into());
            }
            _ => {}
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_wire_shape() {
        let m = Mutation::Set {
            path: "a.b".into(),
            old: json!(1),
            new: json!(2),
        };
        assert_eq!(m.to_wire(), json!({"op":"set","path":"a.b","old":1,"new":2}));
    }

    #[test]
    fn append_carries_the_legacy_action_key() {
        let m = Mutation::Append {
            path: "items".into(),
            item_id: "i1".into(),
            item: json!({"id":"i1"}),
        };
        let w = m.to_wire();
        assert_eq!(w["op"], "append");
        assert_eq!(w["action"], "append");
    }

    #[test]
    fn round_trips_through_json() {
        for m in [
            Mutation::Set { path: "a".into(), old: json!(null), new: json!(1) },
            Mutation::Remove { path: "l".into(), item_id: "x".into() },
            Mutation::ReplaceRoot { value: json!({"a":1}) },
        ] {
            let text = serde_json::to_string(&m).unwrap();
            let back: Mutation = serde_json::from_str(&text).unwrap();
            assert_eq!(m, back);
        }
    }
}
