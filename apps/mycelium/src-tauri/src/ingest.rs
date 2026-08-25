//! **Ingest** — the queue between a phone and the gate.
//!
//! A message arrives. `τ` decides what it means; this decides what happens to
//! it, and what is kept.
//!
//! ## ★★★ A rejected message is never written. Anywhere.
//!
//! [`Ingested::capture`] asks [`sustena_core::parse_message`] **before it opens
//! a file**. A `Rejected` result returns immediately: no queue row, no raw
//! payload, no parsed fields, not even a dedup key — because a dedup key is a
//! hash of the text, and a hash of an OTP is still a record that the OTP
//! existed. The only trace is a counter, so a person can see that something was
//! refused without the thing itself being kept.
//!
//! ★★ That check is **not** in a `match` after the row is built. It is a `let
//! else` before the first write, and `Transduction::storable()` is the
//! condition — a method on the engine's own type rather than a convention this
//! file remembers.
//!
//! ## ★★ Idempotent by construction
//!
//! The dedup key is `sha256(sustain + source + raw)`. A capture client that
//! retries a POST it is unsure of gets the **first** capture's outcome back,
//! flagged as a duplicate, and nothing is applied twice. The sustain is in the
//! key deliberately: two households capturing identical text from a
//! same-named source are two different facts.
//!
//! ## ★ Staleness is never guessed
//!
//! A source is `stale` only when it declared a cadence and has missed it. A
//! source with no declared cadence is never flagged, because inventing one
//! would be exactly the fabricated state the queue exists to prevent.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sustena_core::{parse_message, ParseRule};

use crate::store::{StoreError, StoreResult};

/// One captured message, as the queue holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedMessage {
    pub id: String,
    pub sustain_id: String,
    pub source_id: String,
    /// ★★★ Present only because this message was NOT rejected. A rejection
    /// never reaches this struct.
    pub raw_payload: String,
    pub dedup_key: String,
    pub status: String,
    pub parser_name: String,
    pub reason: String,
    pub parsed_fields: BTreeMap<String, serde_json::Value>,
    pub external_ref: Option<String>,
    pub operator: Option<String>,
    pub params: BTreeMap<String, serde_json::Value>,
    /// Whether the mapped operator was applied, and what the gate said.
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub gate_reason: Option<String>,
    #[serde(default)]
    pub resolved: bool,
    /// ★★★ Set aside by a person as not a transaction.
    ///
    /// A real state, never a silent drop. The message stays in the log with
    /// its raw text, so "why did that disappear?" has an answer and a wrongly
    /// skipped one can be found again.
    #[serde(default)]
    pub ignored: bool,
    /// A monotonic capture order — the host's, not a clock.
    pub seq: u64,
}

impl IngestedMessage {
    pub fn needs_attention(&self) -> bool {
        !self.resolved
            && !self.ignored
            && matches!(self.status.as_str(), "parsed_unmapped" | "unparsed")
    }
}

/// A declared capture source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub label: String,
    /// ★ Only a source that declared a cadence can ever be stale.
    #[serde(default)]
    pub expected_interval_minutes: Option<u32>,
    /// The capture seq this source was last seen at. `None` = never.
    #[serde(default)]
    pub last_seen_seq: Option<u64>,
    #[serde(default)]
    pub captures: u64,
}

/// What one capture did.
///
/// ★ `Debug` is hand-written, not derived: a derived one would print a stored
/// message's whole raw payload into any log that formatted it.
#[derive(Clone)]
pub enum Capture {
    /// ★★★ Refused before anything was written.
    Rejected { reason: String },
    /// Already captured; the first outcome, unchanged.
    Duplicate(Box<IngestedMessage>),
    Stored(Box<IngestedMessage>),
}

/// The ingest store: a queue, the declared sources, learned rules, and the
/// classification history that pre-fills a repeat merchant.
pub struct Ingested {
    root: PathBuf,
    /// ★ Rejections counted, never kept. The only trace one leaves.
    rejected: std::sync::Mutex<u64>,
    /// ★★★ Every dedup key already stored, so "have I seen this?" is a lookup
    /// rather than a re-read of the whole log.
    ///
    /// Capture used to answer that question by loading and parsing every
    /// message on disk and scanning the list, ONCE PER MESSAGE. Reading an
    /// inbox of a few thousand made that quadratic on top of a few thousand
    /// full-log reads. Built once, on first use, and appended to from then on.
    seen: std::sync::Mutex<Option<std::collections::HashSet<String>>>,
}

impl Ingested {
    pub fn at(root: impl AsRef<Path>) -> StoreResult<Self> {
        let root = root.as_ref().join("ingest");
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            rejected: std::sync::Mutex::new(0),
            seen: std::sync::Mutex::new(None),
        })
    }

    fn messages_path(&self) -> PathBuf {
        self.root.join("messages.jsonl")
    }
    fn sources_path(&self) -> PathBuf {
        self.root.join("sources.json")
    }
    fn rules_path(&self) -> PathBuf {
        self.root.join("rules.jsonl")
    }
    fn history_path(&self) -> PathBuf {
        self.root.join("history.json")
    }
    fn marks_path(&self) -> PathBuf {
        self.root.join("read_marks.json")
    }

    /// The newest message already read from a source, as the device timestamps
    /// it.
    ///
    /// ★★★ Why this exists. Re-reading the inbox was correct and wasteful: the
    /// dedup index caught every repeat, but only after the whole inbox had been
    /// walked and every message offered again. Two thousand captures re-read
    /// two thousand times is work nobody asked for. The mark says where the
    /// last read got to, so a repeat only looks at what arrived since.
    ///
    /// ★★ Kept per (sustain, source) because sources are read independently.
    pub fn read_mark(&self, sustain_id: &str, source_id: &str) -> Option<i64> {
        self.marks().ok()?.get(&mark_key(sustain_id, source_id)).copied()
    }

    fn marks(&self) -> StoreResult<BTreeMap<String, i64>> {
        let path = self.marks_path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|e| StoreError::Io(e.to_string()))
    }

    /// Move the mark forward. ★ Never backwards: a partial read must not make
    /// the next one skip what it missed.
    pub fn set_read_mark(&self, sustain_id: &str, source_id: &str, newest_ms: i64) -> StoreResult<()> {
        let mut marks = self.marks()?;
        let key = mark_key(sustain_id, source_id);
        if newest_ms > marks.get(&key).copied().unwrap_or(i64::MIN) {
            marks.insert(key, newest_ms);
            let text =
                serde_json::to_string_pretty(&marks).map_err(|e| StoreError::Io(e.to_string()))?;
            let tmp = self.marks_path().with_extension("json.tmp");
            fs::write(&tmp, text)?;
            fs::rename(&tmp, self.marks_path())?;
        }
        Ok(())
    }

    pub fn rejected_count(&self) -> u64 {
        *self.rejected.lock().expect("rejected lock")
    }

    // ── the queue ────────────────────────────────────────────────────────────

    pub fn messages(&self) -> StoreResult<Vec<IngestedMessage>> {
        read_lines(&self.messages_path())
    }

    /// The latest version of each message, by id — the log is append-only, so a
    /// later line supersedes an earlier one.
    pub fn current(&self) -> StoreResult<Vec<IngestedMessage>> {
        let mut by_id: BTreeMap<String, IngestedMessage> = BTreeMap::new();
        for m in self.messages()? {
            by_id.insert(m.id.clone(), m);
        }
        let mut out: Vec<IngestedMessage> = by_id.into_values().collect();
        out.sort_by_key(|m| m.seq);
        Ok(out)
    }

    fn append_message(&self, m: &IngestedMessage) -> StoreResult<()> {
        append_line(&self.messages_path(), m)
    }

    /// **Capture one message.**
    ///
    /// ★★★ The secret check happens before the first write, and a rejection
    /// returns without touching the disk.
    pub fn capture(
        &self,
        sustain_id: &str,
        source_id: &str,
        raw: &str,
        extra_rules: &[ParseRule],
    ) -> StoreResult<Capture> {
        let t = parse_message(raw, Some(source_id), extra_rules);

        // ★★★ THE LINE. `storable()` is the engine's own answer, and this runs
        //   before the dedup key is even computed — a hash of an OTP is still a
        //   record that the OTP existed.
        if !t.storable() {
            *self.rejected.lock().expect("rejected lock") += 1;
            return Ok(Capture::Rejected { reason: t.reason().to_string() });
        }

        let dedup_key = dedup(sustain_id, source_id, raw);

        // ★★ The cheap question first. Only a key we have never seen costs a
        //    read of the log, so re-reading an inbox that is already captured
        //    is a few thousand hash lookups rather than a few thousand full
        //    parses of everything stored.
        let is_new = {
            let mut guard = self.seen.lock().expect("seen lock");
            let set = match guard.as_mut() {
                Some(set) => set,
                None => {
                    let built: std::collections::HashSet<String> =
                        self.messages()?.into_iter().map(|m| m.dedup_key).collect();
                    guard.insert(built)
                }
            };
            set.insert(dedup_key.clone())
        };
        if !is_new {
            // Seen before. Read the log now, once, to hand back what was stored.
            let existing = self.current()?;
            if let Some(prior) = existing.iter().find(|m| m.dedup_key == dedup_key) {
                return Ok(Capture::Duplicate(Box::new(prior.clone())));
            }
        }

        let existing = self.current()?;
        let seq = existing.last().map(|m| m.seq + 1).unwrap_or(0);
        let message = IngestedMessage {
            id: format!("msg-{seq}-{}", &dedup_key[..8]),
            sustain_id: sustain_id.to_string(),
            source_id: source_id.to_string(),
            raw_payload: raw.to_string(),
            dedup_key,
            status: t.status().to_string(),
            parser_name: t.parser_name().to_string(),
            reason: t.reason().to_string(),
            parsed_fields: t.parsed_fields(),
            external_ref: t.external_ref().map(str::to_string),
            operator: t.operator().map(str::to_string),
            params: t.operator_params(),
            applied: false,
            gate_reason: None,
            resolved: false,
            ignored: false,
            seq,
        };
        self.append_message(&message)?;
        self.mark_seen(source_id, seq)?;
        Ok(Capture::Stored(Box::new(message)))
    }

    /// Record what the gate said about a mapped message's operator.
    pub fn record_outcome(
        &self,
        id: &str,
        applied: bool,
        gate_reason: Option<String>,
    ) -> StoreResult<()> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(());
        };
        m.applied = applied;
        m.gate_reason = gate_reason;
        // ★ Applying resolves it; a refusal leaves it in the queue, because a
        //   person still has something to do about it.
        m.resolved = applied;
        self.append_message(&m)
    }

    /// Set a message aside as not a transaction.
    ///
    /// ★ Distinct from `resolve`, which means "a person dealt with this". This
    /// one means "there was nothing here to deal with", and the log keeps both
    /// the message and which of the two happened.
    pub fn ignore(&self, id: &str) -> StoreResult<bool> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(false);
        };
        m.ignored = true;
        self.append_message(&m)?;
        Ok(true)
    }

    /// Mark a message as handled by a person.
    pub fn resolve(&self, id: &str) -> StoreResult<bool> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(false);
        };
        m.resolved = true;
        self.append_message(&m)?;
        Ok(true)
    }

    // ── sources ──────────────────────────────────────────────────────────────

    pub fn sources(&self) -> StoreResult<Vec<Source>> {
        let path = self.sources_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|e| StoreError::Io(e.to_string()))
    }

    pub fn declare_source(
        &self,
        id: &str,
        label: &str,
        interval: Option<u32>,
    ) -> StoreResult<()> {
        let mut all = self.sources()?;
        match all.iter_mut().find(|s| s.id == id) {
            Some(s) => {
                s.label = label.to_string();
                s.expected_interval_minutes = interval;
            }
            None => all.push(Source {
                id: id.to_string(),
                label: label.to_string(),
                expected_interval_minutes: interval,
                last_seen_seq: None,
                captures: 0,
            }),
        }
        self.write_sources(&all)
    }

    fn mark_seen(&self, id: &str, seq: u64) -> StoreResult<()> {
        let mut all = self.sources()?;
        match all.iter_mut().find(|s| s.id == id) {
            Some(s) => {
                // ★ Stamped on EVERY capture — mapped, unmapped or unparsed
                //   alike. Staleness is about whether a source is alive, not
                //   whether any one message happened to parse.
                s.last_seen_seq = Some(seq);
                s.captures += 1;
            }
            None => all.push(Source {
                id: id.to_string(),
                label: id.to_string(),
                expected_interval_minutes: None,
                last_seen_seq: Some(seq),
                captures: 1,
            }),
        }
        self.write_sources(&all)
    }

    fn write_sources(&self, all: &[Source]) -> StoreResult<()> {
        let text = serde_json::to_string_pretty(all).map_err(|e| StoreError::Io(e.to_string()))?;
        let tmp = self.sources_path().with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, self.sources_path())?;
        Ok(())
    }

    // ── learned rules ────────────────────────────────────────────────────────

    pub fn learned_rules(&self) -> StoreResult<Vec<ParseRule>> {
        read_lines(&self.rules_path())
    }

    /// ★ Append-only and versioned: a correction is never a silent overwrite,
    /// it is a new line, and the latest wins.
    pub fn add_rule(&self, rule: &ParseRule) -> StoreResult<()> {
        append_line(&self.rules_path(), rule)
    }

    /// The rules in force: the latest version of each id.
    pub fn effective_rules(&self) -> StoreResult<Vec<ParseRule>> {
        let mut by_id: BTreeMap<String, ParseRule> = BTreeMap::new();
        for r in self.learned_rules()? {
            by_id.insert(r.id.clone(), r);
        }
        Ok(by_id.into_values().collect())
    }

    // ── classification history ───────────────────────────────────────────────

    pub fn history(&self) -> StoreResult<BTreeMap<String, (String, u32)>> {
        let path = self.history_path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|e| StoreError::Io(e.to_string()))
    }

    /// ★ Recency-only: the most recent confirmation for a counterparty wins.
    /// A weighted vote is a real, disclosed improvement nobody has asked for.
    pub fn remember(&self, sustain_id: &str, description: &str, pocket: &str) -> StoreResult<()> {
        let key = history_key(sustain_id, description);
        let mut all = self.history()?;
        let count = all.get(&key).map(|(_, n)| n + 1).unwrap_or(1);
        all.insert(key, (pocket.to_string(), count));
        let text = serde_json::to_string_pretty(&all).map_err(|e| StoreError::Io(e.to_string()))?;
        let tmp = self.history_path().with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, self.history_path())?;
        Ok(())
    }

    /// How many times each pocket has been chosen, for this Sustain.
    ///
    /// ★ Real counts off the classification history, so the picker can lead
    /// with what he actually uses rather than whatever order the state happens
    /// to hold.
    pub fn pocket_use_counts(&self, sustain_id: &str) -> StoreResult<BTreeMap<String, u32>> {
        let prefix = format!("{sustain_id}::");
        let mut out: BTreeMap<String, u32> = BTreeMap::new();
        for (key, (pocket, count)) in self.history()? {
            if key.starts_with(&prefix) {
                *out.entry(pocket).or_insert(0) += count;
            }
        }
        Ok(out)
    }

    pub fn recall(&self, sustain_id: &str, description: &str) -> Option<(String, u32)> {
        self.history().ok()?.get(&history_key(sustain_id, description)).cloned()
    }
}

fn mark_key(sustain_id: &str, source_id: &str) -> String {
    format!("{sustain_id}::{source_id}")
}

fn history_key(sustain_id: &str, description: &str) -> String {
    format!("{sustain_id}::{}", description.trim().to_lowercase())
}

fn dedup(sustain_id: &str, source_id: &str, raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(sustain_id.as_bytes());
    h.update(b"\0");
    h.update(source_id.as_bytes());
    h.update(b"\0");
    h.update(raw.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn read_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> StoreResult<Vec<T>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(serde_json::from_str(line).map_err(|e| {
            StoreError::Io(format!("{}:{} — {e}", path.display(), i + 1))
        })?);
    }
    Ok(out)
}

fn append_line<T: Serialize>(path: &Path, value: &T) -> StoreResult<()> {
    let line = serde_json::to_string(value).map_err(|e| StoreError::Io(e.to_string()))?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // ★ A rejection's reason is safe; its text was never held.
            Self::Rejected { reason } => f.debug_struct("Rejected").field("reason", reason).finish(),
            Self::Duplicate(m) => f.debug_struct("Duplicate").field("id", &m.id).finish(),
            Self::Stored(m) => {
                f.debug_struct("Stored").field("id", &m.id).field("status", &m.status).finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-ingest-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    const OTP: &str = "Your OTP is 483920. Do not share it with anyone.";

    fn income() -> String {
        sustena_core::seed_rules("mpesa")
            .iter()
            .find(|r| r.id == "mpesa_received")
            .and_then(|r| r.examples.first().cloned())
            .expect("a shipped example")
    }

    #[test]
    fn a_real_income_message_maps_and_is_stored() {
        let i = Ingested::at(scratch("income")).unwrap();
        match i.capture("h", "mpesa", &income(), &[]).unwrap() {
            Capture::Stored(m) => {
                assert_eq!(m.status, "mapped");
                assert_eq!(m.operator.as_deref(), Some("budget.record_income"));
                assert!(m.params["amount"].as_f64().unwrap() > 0.0);
            }
            other => panic!("expected stored, got {other:?}"),
        }
        assert_eq!(i.current().unwrap().len(), 1);
    }

    /// ★★★ THE ONE THAT MUST NOT REGRESS.
    #[test]
    fn an_otp_is_rejected_and_nothing_at_all_is_written() {
        let dir = scratch("otp");
        let i = Ingested::at(&dir).unwrap();
        match i.capture("h", "mpesa", OTP, &[]).unwrap() {
            Capture::Rejected { .. } => {}
            other => panic!("expected rejected, got {other:?}"),
        }
        // The store row count is ZERO.
        assert_eq!(i.current().unwrap().len(), 0, "a rejection must leave no row");
        // And the file does not exist at all — not an empty row, not a hash.
        assert!(!dir.join("ingest").join("messages.jsonl").exists());
        // The OTP text appears nowhere under the store root.
        for entry in fs::read_dir(dir.join("ingest")).unwrap() {
            let p = entry.unwrap().path();
            let content = fs::read_to_string(&p).unwrap_or_default();
            assert!(!content.contains("483920"), "the code leaked into {}", p.display());
            assert!(!content.contains("OTP"), "the text leaked into {}", p.display());
        }
        // The only trace is a count.
        assert_eq!(i.rejected_count(), 1);
    }

    #[test]
    fn a_rejection_does_not_even_mark_the_source_as_seen() {
        // ★ Otherwise a source's activity would silently record that a secret
        //   arrived at a particular moment.
        let i = Ingested::at(scratch("otp-source")).unwrap();
        i.capture("h", "mpesa", OTP, &[]).unwrap();
        assert!(i.sources().unwrap().is_empty());
    }

    #[test]
    fn a_replay_returns_the_first_outcome_and_applies_nothing_twice() {
        let i = Ingested::at(scratch("dedup")).unwrap();
        let first = i.capture("h", "mpesa", &income(), &[]).unwrap();
        let Capture::Stored(a) = first else { panic!("expected stored") };
        match i.capture("h", "mpesa", &income(), &[]).unwrap() {
            Capture::Duplicate(b) => assert_eq!(a.id, b.id),
            other => panic!("expected duplicate, got {other:?}"),
        }
        assert_eq!(i.current().unwrap().len(), 1, "one row, not two");
    }

    #[test]
    fn two_sustains_capturing_the_same_text_are_two_different_facts() {
        let i = Ingested::at(scratch("two")).unwrap();
        assert!(matches!(i.capture("a", "mpesa", &income(), &[]).unwrap(), Capture::Stored(_)));
        assert!(matches!(i.capture("b", "mpesa", &income(), &[]).unwrap(), Capture::Stored(_)));
        assert_eq!(i.current().unwrap().len(), 2);
    }

    #[test]
    fn an_unrecognised_message_queues_for_a_person_rather_than_being_dropped() {
        let i = Ingested::at(scratch("unparsed")).unwrap();
        let Capture::Stored(m) = i.capture("h", "mpesa", "some shape nobody knows", &[]).unwrap()
        else {
            panic!("expected stored")
        };
        assert_eq!(m.status, "unparsed");
        assert!(m.needs_attention());
    }

    #[test]
    fn a_source_is_stamped_on_every_capture_whatever_the_tier() {
        let i = Ingested::at(scratch("seen")).unwrap();
        i.capture("h", "mpesa", "nonsense", &[]).unwrap();
        i.capture("h", "mpesa", &income(), &[]).unwrap();
        let s = &i.sources().unwrap()[0];
        assert_eq!(s.captures, 2, "staleness is about whether it is alive");
        assert!(s.last_seen_seq.is_some());
        // ★ And a source that declared no cadence carries none to guess from.
        assert!(s.expected_interval_minutes.is_none());
    }

    #[test]
    fn a_refused_gate_leaves_the_message_in_the_queue() {
        let i = Ingested::at(scratch("refused")).unwrap();
        let Capture::Stored(m) = i.capture("h", "mpesa", &income(), &[]).unwrap() else {
            panic!()
        };
        i.record_outcome(&m.id, false, Some("would violate an invariant".into())).unwrap();
        let after = i.current().unwrap().into_iter().find(|x| x.id == m.id).unwrap();
        assert!(!after.applied);
        assert!(!after.resolved, "a person still has something to do about it");
        assert_eq!(after.gate_reason.as_deref(), Some("would violate an invariant"));
    }

    #[test]
    fn history_pre_fills_by_counterparty_and_counts_uses() {
        let i = Ingested::at(scratch("history")).unwrap();
        assert!(i.recall("h", "NAIVAS").is_none());
        i.remember("h", "NAIVAS SUPERMARKET", "food").unwrap();
        i.remember("h", "naivas supermarket", "food").unwrap();
        assert_eq!(i.recall("h", "NAIVAS SUPERMARKET"), Some(("food".into(), 2)));
        // ★ Scoped per sustain — one household's habit is not another's.
        assert!(i.recall("other", "NAIVAS SUPERMARKET").is_none());
    }

    #[test]
    fn a_learned_rule_is_appended_and_the_latest_version_wins() {
        let i = Ingested::at(scratch("rules")).unwrap();
        let mut r = sustena_core::seed_rules("mpesa")[0].clone();
        r.id = "learned_x".into();
        i.add_rule(&r).unwrap();
        r.version = 2;
        i.add_rule(&r).unwrap();
        let effective = i.effective_rules().unwrap();
        assert_eq!(effective.len(), 1, "one id, latest version");
        assert_eq!(effective[0].version, 2);
        assert_eq!(i.learned_rules().unwrap().len(), 2, "the history is kept");
    }

    #[test]
    fn everything_survives_a_reopen() {
        let dir = scratch("reopen");
        {
            let i = Ingested::at(&dir).unwrap();
            i.capture("h", "mpesa", &income(), &[]).unwrap();
            i.remember("h", "NAIVAS", "food").unwrap();
            i.declare_source("mpesa", "M-Pesa", Some(1440)).unwrap();
        }
        let i = Ingested::at(&dir).unwrap();
        assert_eq!(i.current().unwrap().len(), 1);
        assert_eq!(i.recall("h", "NAIVAS").unwrap().0, "food");
        assert_eq!(i.sources().unwrap()[0].expected_interval_minutes, Some(1440));
    }
}

#[cfg(test)]
mod reread_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-reread-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    /// ★★★ P4: re-reading an inbox does not double-count.
    ///
    /// The dedup key fingerprints the sustain, the source and the exact text,
    /// so the same message captured twice is stored once and reported as a
    /// duplicate the second time. Re-tapping "read my texts" is safe: it
    /// re-offers everything and stores nothing new.
    #[test]
    fn capturing_the_same_text_twice_stores_it_once() {
        let ing = Ingested::at(scratch("twice")).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        let raw = "Ksh100.00 paid to NAIVAS on 1/8/26";

        assert!(matches!(ing.capture("h", "mpesa", raw, &rules), Ok(Capture::Stored(_))));
        assert!(matches!(ing.capture("h", "mpesa", raw, &rules), Ok(Capture::Duplicate(_))));
        assert_eq!(ing.current().expect("current").len(), 1, "stored once");
    }

    /// The index is built from disk when first needed, so a store opened fresh
    /// still recognises what an earlier run captured.
    #[test]
    fn a_reopened_store_still_recognises_what_it_has() {
        let dir = scratch("reopen");
        let raw = "Ksh250.00 paid to KPLC PREPAID on 1/8/26";
        {
            let ing = Ingested::at(&dir).expect("ingest");
            let rules = ing.effective_rules().expect("rules");
            assert!(matches!(ing.capture("h", "mpesa", raw, &rules), Ok(Capture::Stored(_))));
        }
        let ing = Ingested::at(&dir).expect("reopen");
        let rules = ing.effective_rules().expect("rules");
        assert!(
            matches!(ing.capture("h", "mpesa", raw, &rules), Ok(Capture::Duplicate(_))),
            "a reopened store recognises the earlier capture"
        );
        assert_eq!(ing.current().expect("current").len(), 1);
    }

    /// ★★★ P4: the mark makes a repeat read cheap.
    ///
    /// It starts absent, moves forward when a read completes, and never moves
    /// backwards, so a read abandoned halfway cannot make the next one skip
    /// what it never looked at.
    #[test]
    fn the_read_mark_only_moves_forward() {
        let ing = Ingested::at(scratch("mark")).expect("ingest");
        assert_eq!(ing.read_mark("h", "inbox"), None, "no mark before a read");

        ing.set_read_mark("h", "inbox", 1_000).expect("set");
        assert_eq!(ing.read_mark("h", "inbox"), Some(1_000));

        ing.set_read_mark("h", "inbox", 2_500).expect("forward");
        assert_eq!(ing.read_mark("h", "inbox"), Some(2_500));

        ing.set_read_mark("h", "inbox", 900).expect("backwards is ignored");
        assert_eq!(ing.read_mark("h", "inbox"), Some(2_500), "never goes back");
    }

    /// Marks are per source, so reading one never advances another.
    #[test]
    fn marks_do_not_leak_between_sources() {
        let ing = Ingested::at(scratch("mark-src")).expect("ingest");
        ing.set_read_mark("h", "mpesa", 5_000).expect("set");
        assert_eq!(ing.read_mark("h", "kcb"), None);
        assert_eq!(ing.read_mark("other", "mpesa"), None);
    }

    /// Different text from the same sender is a different message.
    #[test]
    fn different_text_is_not_a_duplicate() {
        let ing = Ingested::at(scratch("diff")).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        assert!(matches!(ing.capture("h", "mpesa", "Ksh1.00 paid to A", &rules), Ok(Capture::Stored(_))));
        assert!(matches!(ing.capture("h", "mpesa", "Ksh2.00 paid to B", &rules), Ok(Capture::Stored(_))));
        assert_eq!(ing.current().expect("current").len(), 2);
    }
}

#[cfg(test)]
mod ignore_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-ignore-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    /// ★★★ From his phone: a card reversal was asking which pocket it came
    /// from. Setting it aside takes it out of the queue and leaves the record.
    #[test]
    fn ignoring_takes_it_out_of_the_queue_without_deleting_it() {
        let ing = Ingested::at(scratch("one")).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        let raw = "Ksh100.00 paid to SOMEWHERE on 1/8/26";
        let Capture::Stored(m) = ing.capture("h", "mpesa", raw, &rules).expect("capture") else {
            panic!("expected it to be stored");
        };
        assert!(m.needs_attention(), "it starts in the queue");

        assert!(ing.ignore(&m.id).expect("ignore"));

        let after = ing.current().expect("current");
        assert_eq!(after.len(), 1, "still on record, not deleted");
        let m2 = &after[0];
        assert!(m2.ignored, "and marked as set aside");
        assert!(!m2.needs_attention(), "so it no longer asks");
        assert_eq!(m2.raw_payload, raw, "the text is kept");
    }

    /// Ignoring is not the same fact as handling, and the log keeps both apart.
    #[test]
    fn ignored_and_resolved_are_different_facts() {
        let ing = Ingested::at(scratch("two")).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        let Capture::Stored(m) = ing
            .capture("h", "mpesa", "Ksh5.00 paid to A on 1/8/26", &rules)
            .expect("capture")
        else {
            panic!("stored");
        };
        ing.ignore(&m.id).expect("ignore");
        let got = ing.current().expect("current").remove(0);
        assert!(got.ignored && !got.resolved, "set aside, not handled");
    }

    /// Ignoring something that is not there says so rather than pretending.
    #[test]
    fn ignoring_an_unknown_message_reports_it() {
        let ing = Ingested::at(scratch("three")).expect("ingest");
        assert!(!ing.ignore("nope").expect("ignore"));
    }
}
