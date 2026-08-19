//! On-disk persistence — **the event log, not a state snapshot**.
//!
//! ## ★★★ Why the log and not the state
//!
//! The engine's central property is `state = fold(events)`. Persisting a state
//! snapshot would make the file the source of truth and the log a story about
//! it — which is exactly backwards, and the first time the two disagreed there
//! would be no way to say which was right. So what survives a quit is the
//! **append-only log**, and the state is rebuilt by folding it on load, by the
//! same `fold_events` the engine ships.
//!
//! ★★ That makes the property *checkable on every launch* rather than asserted:
//! [`Store::load_sustain`] returns the folded state, and nothing else can.
//!
//! ## The format
//!
//! ```text
//! <app_data_dir>/
//!   sustains.json                  the registry — id, label, template, parent
//!   events/<sustain_id>.jsonl      one LoggedEvent per line, append-only
//! ```
//!
//! ★ Plain JSONL rather than SQLite or sled, and that is a choice with reasons:
//! the log **is** the format, a human can read it, `serde_json` is already a
//! dependency, and there is no schema migration to get wrong. What it gives up
//! is named in the honest-limits section of the app README — no compaction, no
//! concurrent writers, and a torn final line on a hard power loss would need
//! the last record dropping.
//!
//! Each append is followed by `sync_all()`, so a clean quit — the case the
//! acceptance check exercises — cannot lose a committed event.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sustena_core::event::CausalStamp;
use sustena_core::sync::{reconcile, LogEntry, Reconciliation, Replayable, Replica};
use sustena_core::VectorClock;
use sustena_core::{
    fold::{fold_events, FoldEvent},
    mutation::Mutation,
    operator::Execution,
};

use crate::dto::EventDto;
use crate::templates::TemplateId;

/// One line of a Sustain's log.
///
/// ★ Carries more than the fold needs — the operator that caused it and the
/// events it published — because an event log a person cannot read is only a
/// database. The fold uses `mutations` alone, so the extra fields can never
/// change what the state is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggedEvent {
    pub seq: u64,
    /// The operator that produced it, or `genesis` for the opening line.
    pub operator: String,
    #[serde(default)]
    pub events: Vec<EventDto>,
    #[serde(default)]
    pub mutations: Vec<Mutation>,
    /// ★★★ **The node that wrote this line.** `seq` alone stopped being an
    /// identity the moment a second node existed: two nodes both write
    /// `seq: 5`, and a merge keyed on the bare sequence silently drops one.
    /// `(origin, seq)` is the identity `sync::LogEntry` needs.
    ///
    /// ★★ `None` on every line written before peering existed — and those
    /// lines genuinely DID originate here, so resolving them to this node is
    /// a true statement rather than a default. Attribution is safe because a
    /// legacy log can only exist for a Sustain this node already had: a
    /// Sustain that arrives from a peer arrives with its origins attached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// The scalar Lamport clock that orders the merged fold (Multiparty §IV).
    ///
    /// ★ `None` on a legacy line, resolved to `seq + 1`: on a single node the
    /// two are the same order, which is exactly why the migration needs no
    /// rewrite of anyone's history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lamport: Option<u64>,
}

impl Replayable for LoggedEvent {
    fn mutations(&self) -> &[Mutation] {
        &self.mutations
    }
}

impl LoggedEvent {
    /// ★★ The genesis line: the opening state as one `ReplaceRoot`. Every
    /// Sustain's history starts here, so there is no "before the log" state to
    /// special-case.
    pub fn genesis(state: &Value) -> Self {
        LoggedEvent {
            seq: 0,
            operator: "genesis".into(),
            events: Vec::new(),
            mutations: vec![Mutation::ReplaceRoot { value: state.clone() }],
            origin: None,
            lamport: None,
        }
    }

    pub fn of(seq: u64, operator: &str, x: &Execution) -> Self {
        LoggedEvent {
            seq,
            operator: operator.to_string(),
            events: x.events.iter().map(EventDto::from).collect(),
            mutations: x.mutations.clone(),
            origin: None,
            lamport: None,
        }
    }

    /// Stamp a line with the node that wrote it and its logical clock.
    ///
    /// ★ A builder rather than two more constructor arguments: `genesis` and
    /// `of` are called from places that do not all know the clock yet, and a
    /// half-filled stamp would be worse than a clearly unstamped one.
    pub fn written_by(mut self, node: &str, seq: u64, lamport: u64) -> Self {
        self.origin = Some(node.to_string());
        self.seq = seq;
        self.lamport = Some(lamport);
        self
    }
}

/// One side of a transfer, as the write-ahead journal holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalLeg {
    pub sustain_id: String,
    pub event: LoggedEvent,
}

/// A transfer that has been decided and is being written.
///
/// ★★ It holds BOTH legs fully materialised, so recovery needs nothing but this
/// file — not the core, not the two states, not the decision that produced it.
/// A journal that only recorded intent would need the transfer re-decided
/// against states that have since moved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferJournal {
    pub transfer_id: String,
    pub legs: Vec<JournalLeg>,
}

/// What the registry remembers about one Sustain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SustainRecord {
    pub id: String,
    pub label: String,
    pub template: TemplateId,
    /// ★ Set when this Sustain was instantiated from an AUTHORED definition
    /// rather than a built-in template. `template` is then only a shape hint;
    /// `custom` is the truth.
    #[serde(default)]
    pub custom: Option<String>,
    /// `⊕` — the parent in the household's composition, if any.
    pub parent: Option<String>,
    /// ★★★ **Whose Sustain this is** — the handle that holds OWNER authority
    /// over it.
    ///
    /// The membership graph is derived from this rather than guessed from the
    /// id: an earlier pass matched `habitat-<x>` against the principal's handle
    /// and got it wrong the moment the handle (`bg.myc`) and the member name
    /// (`bonnie`) were not the same word. A Sustain says who it belongs to; the
    /// host does not infer it from spelling.
    ///
    /// `None` means *nobody has claimed it*, which is a real state — the five
    /// habitats whose members have no identity on this machine yet.
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub sustains: Vec<SustainRecord>,
    /// Which Sustain the cockpit is looking at. Persisted so a relaunch lands
    /// where you left off.
    #[serde(default)]
    pub selected: Option<String>,
}

#[derive(Debug)]
pub enum StoreError {
    Io(String),
    Corrupt { file: String, line: usize, reason: String },
    Fold(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "disk: {e}"),
            // ★ Names the file AND the line. A corrupt-log error that cannot
            //   say where is an alarm, not a diagnosis.
            StoreError::Corrupt { file, line, reason } => {
                write!(f, "{file} line {line} is unreadable: {reason}")
            }
            StoreError::Fold(e) => write!(f, "the log does not replay: {e}"),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e.to_string())
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

/// The household's durable home.
///
/// ★★ `Clone` shares the SAME append lock, deliberately. Once a peer server
/// runs on its own thread there are two writers to one `.jsonl`, and an
/// interleaved append would corrupt a line rather than merely reorder it. A
/// clone is a second handle to one store, not a second store.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
    appending: Arc<Mutex<()>>,
}

impl Store {
    pub fn at(root: impl Into<PathBuf>) -> StoreResult<Store> {
        let root = root.into();
        fs::create_dir_all(root.join("events"))?;
        fs::create_dir_all(root.join("transfers"))?;
        Ok(Store { root, appending: Arc::new(Mutex::new(())) })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn registry_path(&self) -> PathBuf {
        self.root.join("sustains.json")
    }

    fn definitions_path(&self) -> PathBuf {
        self.root.join("definitions.jsonl")
    }

    /// Every authored definition, in the order they were written.
    ///
    /// ★ Append-only, like the event log: a definition is never edited in
    /// place. The LAST entry for an id is the current one, so the file is the
    /// history of how a `Σ` was arrived at rather than only where it ended up.
    pub fn read_definitions(&self) -> StoreResult<Vec<crate::definitions::AuthoredDefinition>> {
        let path = self.definitions_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path)?;
        let mut latest: std::collections::BTreeMap<String, crate::definitions::AuthoredDefinition> =
            Default::default();
        for (i, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let d: crate::definitions::AuthoredDefinition =
                serde_json::from_str(&line).map_err(|e| StoreError::Corrupt {
                    file: "definitions.jsonl".into(),
                    line: i + 1,
                    reason: e.to_string(),
                })?;
            latest.insert(d.id.clone(), d);
        }
        Ok(latest.into_values().collect())
    }

    /// Append one authored definition. ★ Durably, like every other append.
    pub fn append_definition(
        &self,
        d: &crate::definitions::AuthoredDefinition,
    ) -> StoreResult<()> {
        let line = serde_json::to_string(d).map_err(|e| StoreError::Io(e.to_string()))?;
        let mut f =
            OpenOptions::new().create(true).append(true).open(self.definitions_path())?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        Ok(())
    }

    fn log_path(&self, sustain_id: &str) -> PathBuf {
        // ★ Ids are host-minted (`habitat-bonnie`, `homestead`) and never come
        //   from a user, so there is no path-traversal question to answer —
        //   but the sanitiser is cheap and the day an id becomes user-typed it
        //   is already correct.
        let safe: String = sustain_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.root.join("events").join(format!("{safe}.jsonl"))
    }

    // ── the registry ─────────────────────────────────────────────────────────

    pub fn load_registry(&self) -> StoreResult<Registry> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(Registry::default());
        }
        let text = fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|e| StoreError::Corrupt {
            file: "sustains.json".into(),
            line: e.line(),
            reason: e.to_string(),
        })
    }

    pub fn save_registry(&self, registry: &Registry) -> StoreResult<()> {
        let text = serde_json::to_string_pretty(registry).map_err(|e| StoreError::Io(e.to_string()))?;
        let tmp = self.registry_path().with_extension("json.tmp");
        {
            // ★ Written to a temp file and renamed, so a crash mid-write leaves
            //   the previous registry intact rather than a half one.
            let mut f = File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, self.registry_path())?;
        Ok(())
    }

    // ── the log ──────────────────────────────────────────────────────────────

    /// Every line of one Sustain's log, in order.
    pub fn read_log(&self, sustain_id: &str) -> StoreResult<Vec<LoggedEvent>> {
        let path = self.log_path(sustain_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path)?;
        let mut out = Vec::new();
        for (i, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let ev: LoggedEvent = serde_json::from_str(&line).map_err(|e| StoreError::Corrupt {
                file: format!("events/{sustain_id}.jsonl"),
                line: i + 1,
                reason: e.to_string(),
            })?;
            out.push(ev);
        }
        Ok(out)
    }

    /// Append one event, durably.
    ///
    /// ★★ `sync_all` before returning: a committed event that the app reported
    /// as committed must survive the quit that follows it.
    pub fn append(&self, sustain_id: &str, event: &LoggedEvent) -> StoreResult<()> {
        let line = serde_json::to_string(event).map_err(|e| StoreError::Io(e.to_string()))?;
        // ★ One writer at a time across every handle to this store.
        let _guard = self.appending.lock().expect("append lock");
        let mut f = OpenOptions::new().create(true).append(true).open(self.log_path(sustain_id))?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        Ok(())
    }

    // ── the two-log transfer, crash-safe ─────────────────────────────────────

    fn transfers_dir(&self) -> PathBuf {
        self.root.join("transfers")
    }

    fn journal_path(&self, transfer_id: &str) -> PathBuf {
        let safe: String = transfer_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.transfers_dir().join(format!("{safe}.json"))
    }

    /// ★★★ **Both logs get their leg, or neither does.**
    ///
    /// The core makes half a transfer *unrepresentable*; this makes half a
    /// transfer *unwritable*. Two `.jsonl` files cannot be appended atomically
    /// by any filesystem call, so the only honest mechanism is a **write-ahead
    /// intent journal**:
    ///
    /// ```text
    ///   1. write transfers/<id>.json  (both legs, materialised)  — temp+rename
    ///   2. append leg A                                          — fsync
    ///   3. append leg B                                          — fsync
    ///   4. delete the journal
    /// ```
    ///
    /// A crash anywhere in 2–3 leaves the journal on disk, and
    /// [`recover_transfers`](Store::recover_transfers) finishes the job on the
    /// next open. A crash in 1 leaves a temp file and **nothing appended**,
    /// which is a transfer that never started.
    ///
    /// ★★ Recovery is idempotent because it re-reads each log first: a leg
    /// already present is skipped by its `seq`, so replaying a journal twice
    /// cannot double-apply money. That is what makes "finish it later" safe
    /// rather than a second way to lose track.
    pub fn commit_transfer(
        &self,
        transfer_id: &str,
        legs: [(String, LoggedEvent); 2],
    ) -> StoreResult<()> {
        let journal = TransferJournal {
            transfer_id: transfer_id.to_string(),
            legs: legs
                .iter()
                .map(|(id, e)| JournalLeg { sustain_id: id.clone(), event: e.clone() })
                .collect(),
        };
        self.write_journal(&journal)?;
        self.apply_journal(&journal)?;
        // ★ Only now. A journal removed before both appends landed would be a
        //   promise withdrawn before it was kept.
        let _ = fs::remove_file(self.journal_path(transfer_id));
        Ok(())
    }

    fn write_journal(&self, j: &TransferJournal) -> StoreResult<()> {
        let text = serde_json::to_string_pretty(j).map_err(|e| StoreError::Io(e.to_string()))?;
        let path = self.journal_path(&j.transfer_id);
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Append whichever legs are not already on disk. Idempotent by `seq`.
    fn apply_journal(&self, j: &TransferJournal) -> StoreResult<()> {
        for leg in &j.legs {
            let already = self
                .read_log(&leg.sustain_id)?
                .iter()
                // ★★ `(origin, seq)`, not `seq`. Once a log can hold a peer's
                //    entries, a bare sequence is no longer an identity, and
                //    "already present" would start matching the wrong line.
                .any(|e| e.seq == leg.event.seq && e.origin == leg.event.origin);
            if !already {
                self.append(&leg.sustain_id, &leg.event)?;
            }
        }
        Ok(())
    }

    /// ★★★ Finish any transfer a crash interrupted. Called on every open.
    ///
    /// Returns the ids it completed, so a caller can say so rather than
    /// recovering silently — a half-transfer that healed itself without telling
    /// anyone is still a household that was briefly wrong.
    pub fn recover_transfers(&self) -> StoreResult<Vec<String>> {
        let dir = self.transfers_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut done = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                // A leftover `.json.tmp` is a journal that never landed, which
                // means nothing was appended. Removing it is safe and correct.
                let _ = fs::remove_file(&path);
                continue;
            }
            let text = fs::read_to_string(&path)?;
            let j: TransferJournal = serde_json::from_str(&text).map_err(|e| {
                StoreError::Io(format!("transfer journal {}: {e}", path.display()))
            })?;
            self.apply_journal(&j)?;
            let _ = fs::remove_file(&path);
            done.push(j.transfer_id);
        }
        done.sort();
        Ok(done)
    }

    /// ★ Test-only seam: write the journal and apply ONLY the first leg, then
    /// stop — a crash between the two appends, produced rather than imagined.
    #[doc(hidden)]
    pub fn crash_after_first_leg(
        &self,
        transfer_id: &str,
        legs: [(String, LoggedEvent); 2],
    ) -> StoreResult<()> {
        let journal = TransferJournal {
            transfer_id: transfer_id.to_string(),
            legs: legs
                .iter()
                .map(|(id, e)| JournalLeg { sustain_id: id.clone(), event: e.clone() })
                .collect(),
        };
        self.write_journal(&journal)?;
        let first = &journal.legs[0];
        self.append(&first.sustain_id, &first.event)?;
        Ok(())
    }

    /// ★★★ **`state = fold(events)`, on every load.**
    ///
    /// The state is not stored anywhere. This is the only way to obtain it, so
    /// the property cannot quietly stop being true.
    pub fn load_state(&self, sustain_id: &str) -> StoreResult<(Value, u64)> {
        let log = self.read_log(sustain_id)?;
        let next_seq = log.last().map(|e| e.seq + 1).unwrap_or(0);
        let folded: Vec<FoldEvent> =
            log.into_iter().map(|e| FoldEvent { mutations: e.mutations }).collect();
        let state = fold_events(&folded, None).map_err(|e| StoreError::Fold(format!("{e:?}")))?;
        Ok((state, next_seq))
    }

    // ── the replicated view of the same log ───────────────────────────

    /// The same `.jsonl`, read as a CRDT replica.
    ///
    /// ★★★ **One file, two readings, and they agree on a single node.**
    /// [`Store::load_state`] folds in FILE order; this folds in CAUSAL order.
    /// While every line was written here those are the same sequence, which is
    /// what makes the migration free. They diverge the moment a peer's entry
    /// lands mid-history — and from then on the causal one is the correct one,
    /// which is why [`Store::load_replicated`] is what the world uses once a
    /// Sustain is shared.
    ///
    /// ★ `node` is this machine's public key. Legacy lines are attributed to
    /// it (see [`LoggedEvent::origin`]).
    pub fn read_replica(&self, sustain_id: &str, node: &str) -> StoreResult<Replica<LoggedEvent>> {
        let mut replica = Replica::new();
        for line in self.read_log(sustain_id)? {
            replica.insert(entry_of(line, node));
        }
        Ok(replica)
    }

    /// Fold a Sustain in causal order, and report what the merge could not
    /// decide. `invariants` are the Sustain's own — the same rules the gate
    /// checks — and `None` means *unmeasured*, not *fine*.
    pub fn load_replicated(
        &self,
        sustain_id: &str,
        node: &str,
        invariants: Option<&[(String, String)]>,
    ) -> StoreResult<(Reconciliation, u64)> {
        let replica = self.read_replica(sustain_id, node)?;
        // ★★ This node's next sequence is read off its OWN frontier, never off
        //    the last line in the file: a peer's entry appended after ours
        //    would otherwise push this node's sequence forward into a number
        //    the peer may already have used.
        //
        // ★ No `+ 1`: the frontier holds counters, and `counter = seq + 1`
        //   (see `entry_of`), so the highest counter already IS the next seq.
        //   A node with no entries reads `0`, which is genesis.
        let next_seq = replica.frontier().get(node);
        let out = reconcile(&replica, None, invariants)
            .map_err(|e| StoreError::Fold(format!("{e:?}")))?;
        Ok((out, next_seq))
    }

    /// Take entries from a peer, keeping only what is genuinely new.
    ///
    /// ★★ Idempotent by identity rather than by bookkeeping: an entry whose
    /// `(origin, seq)` is already on disk is skipped, so re-syncing the same
    /// peer any number of times appends nothing. That is the property that
    /// makes a retry after a dropped connection always safe.
    ///
    /// Returns the entries actually written.
    pub fn merge_entries(
        &self,
        sustain_id: &str,
        node: &str,
        incoming: Vec<LogEntry<LoggedEvent>>,
    ) -> StoreResult<Vec<LogEntry<LoggedEvent>>> {
        let mut have = self.read_replica(sustain_id, node)?;
        let mut written = Vec::new();
        for entry in incoming {
            if have.contains(&entry.stamp.node, entry.stamp.counter) {
                continue;
            }
            let mut line = entry.payload.clone();
            line.origin = Some(entry.stamp.node.clone());
            // The inverse of `entry_of`'s `seq + 1`, in the one other place
            // the two representations meet.
            line.seq = entry.stamp.counter.saturating_sub(1);
            line.lamport = Some(entry.lamport);
            self.append(sustain_id, &line)?;
            have.insert(entry.clone());
            written.push(entry);
        }
        Ok(written)
    }
}

/// Resolve a stored line into a replicated entry.
///
/// ★ The one place the legacy defaults are interpreted, so there is exactly
/// one answer to *what does an unstamped line mean* rather than one per
/// call site.
fn entry_of(line: LoggedEvent, this_node: &str) -> LogEntry<LoggedEvent> {
    let origin = line.origin.clone().unwrap_or_else(|| this_node.to_string());
    // ★★★ **`counter = seq + 1`, and the off-by-one is load-bearing.** A
    //     vector clock's absent component is `0` and MEANS *this node has done
    //     nothing* — that is what lets two clocks over different node sets be
    //     compared at all. A log whose first line sits at `seq 0` would be
    //     indistinguishable from a node that had never written, so
    //     `missing_from` would never send anyone their genesis. Found by two
    //     real nodes syncing and one of them receiving nothing.
    let counter = line.seq + 1;
    let lamport = line.lamport.unwrap_or(line.seq + 1);
    let clock = VectorClock::new().at(&origin, counter);
    LogEntry {
        stamp: CausalStamp { counter, node: origin },
        lamport,
        clock,
        t_event: 0,
        payload: line,
    }
}
