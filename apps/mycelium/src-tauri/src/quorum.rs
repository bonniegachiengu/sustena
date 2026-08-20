//! **Agreement before the append** — the answer to *convergence is not
//! preservation*.
//!
//! Slice 6 named the problem precisely and could not fix it there: two nodes
//! recording income concurrently **converge** — the CRDT law is untouched —
//! onto a state where one of the two writes was superseded, because an
//! absolute `Set` lets the later one in fold order win. Merging after the fact
//! cannot recover an intent that was overwritten. The only fix is to stop the
//! two writes from being concurrent in the first place, which is what §V is
//! for.
//!
//! ★★★ **One Paxos instance per SLOT, not per Sustain.** Paxos decides *one*
//! value; a log needs a decision per position. So the instance key is
//! `(sustain_id, slot)` and what is agreed is *which write takes slot n* —
//! the replicated-state-machine shape. Without the slot, the body would
//! re-choose the first value it ever agreed on, forever, and every later write
//! would lose.
//!
//! ★★★ **Only a SHARED Sustain pays for this.** A Sustain with no declared
//! co-owners writes exactly as it did before — no round, no round-trip, no
//! reachability requirement. A household on one device never touches this
//! file, which is the difference between a local-first app and a distributed
//! one wearing local-first as a label.
//!
//! ★★★ **A promise that does not survive a restart is not a promise.** An
//! acceptor that forgot what it promised could accept two different values for
//! one slot, which is precisely what quorum overlap exists to prevent. So the
//! acceptor state is written to disk (temp + rename) **before** the reply goes
//! out, not after.
//!
//! ★★ **A minority cannot write, and that is correctness.** An unreachable
//! co-owner is not a grant — see `consensus::granted` — so a node cut off from
//! the rest is refused rather than allowed to build a history the others will
//! never accept. That is the CAP tax, taken deliberately for the Sustains that
//! declare they cannot tolerate a supersession, and *not* taken for everything
//! else.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sustena_core::consensus::{Acceptor, Promise, ProposalNumber};

/// The instance key: one Paxos run per position in one Sustain's log.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Slot {
    pub sustain_id: String,
    pub slot: u64,
}

impl Slot {
    pub fn new(sustain_id: &str, slot: u64) -> Slot {
        Slot { sustain_id: sustain_id.to_string(), slot }
    }

    fn key(&self) -> String {
        format!("{}#{}", self.sustain_id, self.slot)
    }
}

/// What is being agreed: **which write takes this slot**.
///
/// ★★ The `operator` and `params` a node wants to run, plus who is asking.
/// Deliberately not the resulting state — agreeing on an outcome would make
/// every node's gate advisory, and the whole point is that each node still
/// runs its own admission check against its own state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Write {
    pub operator: String,
    pub params: Map<String, Value>,
    /// The proposing node's public key.
    pub by: String,
}

impl Write {
    pub fn value(&self) -> Value {
        json!({ "operator": self.operator, "params": self.params, "by": self.by })
    }

    pub fn from_value(v: &Value) -> Option<Write> {
        Some(Write {
            operator: v.get("operator")?.as_str()?.to_string(),
            params: v.get("params")?.as_object()?.clone(),
            by: v.get("by")?.as_str()?.to_string(),
        })
    }
}

/// Why a proposed write did not land.
///
/// ★★★ **Three outcomes, and collapsing them would be the defect.** *The body
/// could not be reached* and *the body agreed on someone else's write* are
/// completely different things to a person: the first means try again when the
/// family is online, the second means your move was ordered behind theirs and
/// should be retried against the state that now exists. A single "failed"
/// would leave them indistinguishable.
#[derive(Debug, Clone, PartialEq)]
pub enum Agreement {
    /// The body chose this node's write. It may now be applied and appended.
    Won { number: ProposalNumber, slot: u64 },
    /// The body chose a **different** write for this slot. Nothing was
    /// applied. ★ Retryable, against the state that write produces.
    Lost { slot: u64, to: String, chose: Box<Write> },
    /// Not enough co-owners answered. ★★ Nothing was applied, and nothing was
    /// half-applied: the round is abandoned before any local write.
    NoQuorum { slot: u64, reached: usize, needed: usize, unreachable: Vec<String> },
}

impl Agreement {
    pub fn won(&self) -> bool {
        matches!(self, Agreement::Won { .. })
    }

    /// The refusal class, by the name the gate family uses.
    pub fn rule(&self) -> Option<&'static str> {
        match self {
            Agreement::Won { .. } => None,
            Agreement::Lost { .. } => Some("lost_the_round"),
            Agreement::NoQuorum { .. } => Some("no_quorum"),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Agreement::Won { slot, .. } => format!("the body agreed this write takes slot {slot}"),
            Agreement::Lost { slot, to, chose } => format!(
                "slot {slot} went to {}'s {} — nothing here was applied; retry against the state that leaves",
                short(to),
                chose.operator
            ),
            Agreement::NoQuorum { reached, needed, unreachable, .. } => format!(
                "{reached} of {needed} co-owners answered — this write was NOT applied. \
                 Unreachable: {}. A shared Sustain cannot be written from a minority, \
                 because the others would never accept the history.",
                if unreachable.is_empty() {
                    "none named".to_string()
                } else {
                    unreachable.iter().map(|k| short(k)).collect::<Vec<_>>().join(", ")
                }
            ),
        }
    }
}

fn short(key: &str) -> String {
    key.chars().take(8).collect()
}

// ---------------------------------------------------------------------------
// The acceptor store
// ---------------------------------------------------------------------------

/// This node's acceptors, one per slot it has been asked about.
///
/// ★★ Held in memory and mirrored to disk on every change. The mirror is the
/// authority after a restart; the memory is only speed.
pub struct Acceptors {
    path: PathBuf,
    state: Mutex<BTreeMap<String, Acceptor>>,
}

impl Acceptors {
    pub fn at(root: &Path) -> Acceptors {
        let path = root.join("acceptors.json");
        let state = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Acceptors { path, state: Mutex::new(state) }
    }

    fn save(&self, state: &BTreeMap<String, Acceptor>) -> Result<(), String> {
        let text = serde_json::to_string(state).map_err(|e| e.to_string())?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &self.path).map_err(|e| e.to_string())
    }

    /// `PREPARE(n)` for one slot. ★★★ **Persisted before the answer.**
    pub fn on_prepare(&self, slot: &Slot, n: &ProposalNumber) -> Result<Promise, String> {
        let mut state = self.state.lock().expect("acceptors lock");
        let acceptor = state.entry(slot.key()).or_default();
        let reply = acceptor.on_prepare(n);
        self.save(&state)?;
        Ok(reply)
    }

    /// `ACCEPT(n, v)` for one slot. ★★★ **Persisted before the answer.**
    pub fn on_accept(
        &self,
        slot: &Slot,
        n: &ProposalNumber,
        v: &Value,
    ) -> Result<sustena_core::consensus::Accepted, String> {
        let mut state = self.state.lock().expect("acceptors lock");
        let acceptor = state.entry(slot.key()).or_default();
        let reply = acceptor.on_accept(n, v);
        self.save(&state)?;
        Ok(reply)
    }

    /// What this node accepted for a slot, if anything.
    pub fn accepted(&self, slot: &Slot) -> Option<(ProposalNumber, Value)> {
        self.state
            .lock()
            .expect("acceptors lock")
            .get(&slot.key())
            .and_then(|a| a.accepted().cloned())
    }

    pub fn len(&self) -> usize {
        self.state.lock().expect("acceptors lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mycelium-quorum-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn a_promise_survives_a_restart() {
        // ★★★ The property the whole safety argument rests on. An acceptor
        //     that forgot its promise could accept two different values for
        //     one slot, and two nodes would then have ratified different
        //     histories for the same position.
        let dir = scratch("persist");
        let slot = Slot::new("shared", 1);
        let high = ProposalNumber::new(9, "alice");

        {
            let acceptors = Acceptors::at(&dir);
            assert!(matches!(
                acceptors.on_prepare(&slot, &high).expect("prepare"),
                Promise::Granted { .. }
            ));
        }

        // A different process, reading the same directory.
        let reopened = Acceptors::at(&dir);
        let low = ProposalNumber::new(2, "bob");
        assert_eq!(
            reopened.on_prepare(&slot, &low).expect("prepare"),
            Promise::Refused { promised: high },
            "the restart did not lose the promise",
        );
    }

    #[test]
    fn slots_do_not_interfere() {
        // ★★ One Paxos instance per position. A promise about slot 1 says
        //    nothing about slot 2, or the body would decide once and refuse
        //    every subsequent write forever.
        let dir = scratch("slots");
        let acceptors = Acceptors::at(&dir);
        let n = ProposalNumber::new(1, "alice");
        acceptors.on_prepare(&Slot::new("shared", 1), &n).expect("prepare");
        acceptors
            .on_accept(&Slot::new("shared", 1), &n, &json!("first"))
            .expect("accept");

        assert!(matches!(
            acceptors.on_prepare(&Slot::new("shared", 2), &n).expect("prepare"),
            Promise::Granted { accepted: None },
            ));
        assert!(acceptors.accepted(&Slot::new("shared", 2)).is_none());
    }

    #[test]
    fn two_sustains_do_not_share_a_slot() {
        let dir = scratch("sustains");
        let acceptors = Acceptors::at(&dir);
        let n = ProposalNumber::new(1, "alice");
        acceptors.on_prepare(&Slot::new("a", 1), &n).expect("prepare");
        acceptors.on_accept(&Slot::new("a", 1), &n, &json!("x")).expect("accept");
        assert!(acceptors.accepted(&Slot::new("b", 1)).is_none());
    }

    #[test]
    fn the_three_outcomes_read_as_three_different_things() {
        // ★★★ A person needs to know WHICH of these happened. "Failed" would
        //     leave "your family is offline" and "your move was ordered behind
        //     theirs" looking identical.
        let lost = Agreement::Lost {
            slot: 3,
            to: "bb".repeat(32),
            chose: Box::new(Write {
                operator: "budget.record_income".into(),
                params: Map::new(),
                by: "bb".repeat(32),
            }),
        };
        let none = Agreement::NoQuorum {
            slot: 3,
            reached: 1,
            needed: 2,
            unreachable: vec!["cc".repeat(32)],
        };
        assert_eq!(lost.rule(), Some("lost_the_round"));
        assert_eq!(none.rule(), Some("no_quorum"));
        assert!(lost.describe().contains("retry"));
        assert!(none.describe().contains("NOT applied"));
        assert!(!lost.won() && !none.won());
        assert_eq!(Agreement::Won { number: ProposalNumber::new(1, "a"), slot: 3 }.rule(), None);
    }

    #[test]
    fn a_write_round_trips_through_its_agreed_value() {
        let w = Write {
            operator: "budget.record_income".into(),
            params: [("amount".to_string(), json!(300.0))].into_iter().collect(),
            by: "aa".repeat(32),
        };
        assert_eq!(Write::from_value(&w.value()), Some(w));
    }
}
