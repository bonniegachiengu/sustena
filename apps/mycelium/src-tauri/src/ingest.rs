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

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
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
    /// ★★★ Cancelled against its opposite number, and which one.
    ///
    /// A charge and its refund net to zero. Both leave the queue, and each
    /// records the id of the other, so "where did those two go?" has an answer
    /// and the pairing can be checked rather than taken on trust.
    #[serde(default)]
    pub netted_with: Option<String>,
    /// ★★★ What was actually DONE about this message, in the order it happened.
    ///
    /// Recording the outcome as a bare `resolved: true` answered "was this
    /// dealt with" and lost "how". That gap is what made a refund unanswerable:
    /// undoing a charge needs to know which pocket it went to, and whether the
    /// filing also moved money out of liquid to get it there.
    ///
    /// A list rather than one entry, because filing a past charge is two moves
    /// -- an allocation to make room, then the spend itself -- and undoing it
    /// is both of them in reverse.
    #[serde(default)]
    pub filed: Vec<Filing>,
    /// ★★★ Another message, from another sender, about this same transaction.
    ///
    /// Two banks can both write about one movement of money. The texts differ,
    /// so both are captured — correctly, they are two real messages — but only
    /// one of them may be applied, or the money is counted twice. This records
    /// which one got there first.
    #[serde(default)]
    pub same_event_as: Option<String>,
    /// ★★★ Put off honestly, because he does not remember yet.
    ///
    /// A different act from setting something aside as not a transaction. That
    /// one says "there is nothing here"; this one says "there is something
    /// here and I cannot answer it right now". Collapsing the two would either
    /// lose real transactions or fill the queue with junk, and the difference
    /// is the whole reason not to guess.
    ///
    /// The value is the capture seq it was deferred at, so the ones he has been
    /// putting off longest come back first.
    #[serde(default)]
    pub deferred_at: Option<u64>,
    /// ★★★ Set aside once, then overturned by evidence.
    ///
    /// A skip is a person's judgement, and overturning one quietly would be
    /// worse than leaving it wrong: the queue would change under him with no
    /// account of why. Only a shared transaction reference does it — proof
    /// that the thing he waved past was one half of a real movement of money.
    #[serde(default)]
    pub reclaimed: bool,
    /// ★★★ Set aside by a person as not a transaction.
    ///
    /// A real state, never a silent drop. The message stays in the log with
    /// its raw text, so "why did that disappear?" has an answer and a wrongly
    /// skipped one can be found again.
    #[serde(default)]
    pub ignored: bool,
    /// When the phone says the message arrived, in epoch milliseconds.
    ///
    /// ★★★ NOT the same fact as `seq`, and the difference decides real money.
    /// `seq` is the order they were READ, and a backlog is read newest-first,
    /// so it runs backwards through time. Asking which balance a bank most
    /// recently reported is a question about when the message was SENT, and
    /// only this answers it.
    ///
    /// `None` for anything captured before this was carried, and for a pasted
    /// message, which has no arrival time of its own. Absent rather than
    /// guessed: a fabricated timestamp here would silently pick the wrong
    /// message as the freshest word on an account balance.
    #[serde(default)]
    pub sent_at_ms: Option<i64>,
    /// **`t_event`** — the moment the MESSAGE says it happened.
    ///
    /// ★★★ The second clock, and without it skew is not merely unknown but
    /// unmeasurable. Every capture carried only the moment the phone read it,
    /// so a backfill of two thousand texts stamped a month of spending with one
    /// afternoon — and nothing in the system could tell, because there was no
    /// second reading to disagree with the first.
    ///
    /// ★★ `None` when the message carried no readable time. Never the current
    /// moment: that would make skew exactly zero, the one value that looks
    /// healthy, for precisely the messages nobody could read a time from.
    #[serde(default)]
    pub event_at_ms: Option<i64>,
    /// A monotonic capture order — the host's, not a clock.
    pub seq: u64,
}

impl IngestedMessage {
    pub fn needs_attention(&self) -> bool {
        !self.resolved
            && !self.ignored
            && self.netted_with.is_none()
            // ★ The other bank's account of something already recorded. Real,
            //   kept, and not a decision anyone has to make.
            && self.same_event_as.is_none()
            && matches!(self.status.as_str(), "parsed_unmapped" | "unparsed")
    }

    /// The amount the transducer read off this message, if it read one.
    pub fn amount(&self) -> Option<f64> {
        self.parsed_fields.get("amount").and_then(|v| {
            v.as_f64().or_else(|| v.as_str().and_then(|s| s.replace(',', "").parse().ok()))
        })
    }

    /// Who it was with, as the transducer read it.
    pub fn counterparty(&self) -> Option<&str> {
        self.parsed_fields.get("counterparty").and_then(|v| v.as_str())
    }

    /// ★★★ The transaction's own reference code — the reliable join key.
    ///
    /// Both halves of one real movement of money carry it. A KCB
    /// "SEND TO M-PESA" quotes `M-PESA REF: UHPB9480T9`, and the M-Pesa text
    /// confirming the same transfer opens with that identical code. So do a
    /// charge and the refund that reverses it, and so do the two texts two
    /// banks send about one payment.
    ///
    /// ★★★ This is what amount-and-merchant was standing in for, and it is
    /// strictly better. Amount plus merchant asks "could these be the same
    /// thing?" and has to refuse whenever more than one candidate fits; a ref
    /// asks "are these the same thing?" and the answer is yes or no. Two
    /// identical KES 2,000 transfers to the same person on the same day are
    /// hopeless for the first and trivial for the second.
    pub fn reference(&self) -> Option<&str> {
        self.parsed_fields
            .get("ref")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|r| r.len() >= MIN_REF_LEN)
    }
}

/// One real operator call made about a captured message.
///
/// ★ The params as they were sent, not as they were inferred. An inference can
/// be edited before it is confirmed, and it is the confirmed call that moved
/// money.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filing {
    pub operator: String,
    pub params: BTreeMap<String, serde_json::Value>,
}

/// One text that describes a whole move between his own accounts.
///
/// ★★★ The shape that unsticks the real case. A KCB "SEND TO M-PESA request of
/// KES X from <his account> to <his number>" names BOTH ends in one sentence,
/// so it does not have to wait for a partner text to arrive before it can be
/// recognised. Two-leg matching stays for the shapes that only ever describe
/// their own side.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelfMove {
    pub message: String,
    pub from_account: String,
    pub to_account: String,
    pub amount: f64,
    /// What the bank charged. ★ This is the shape that quotes a cost most
    /// often -- one text naming both ends usually names the fee as well.
    #[serde(default)]
    pub fee: f64,
    /// An income already filed for the other side of this same move, if the
    /// partner text arrived first and applied itself.
    pub undo_income: Option<String>,
    /// ★★ True when he had set this aside and a reference brought it back.
    /// Reported, so the queue never changes under him unexplained.
    pub reclaimed_from_skip: bool,
}

/// Two texts that are one move between his own accounts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MatchedTransfer {
    /// The message saying money left.
    pub out_leg: String,
    /// The message saying money arrived.
    pub in_leg: String,
    pub from_account: String,
    pub to_account: String,
    pub amount: f64,
    /// What the bank charged for the move, read off the leaving leg.
    ///
    /// ★★ Money that does NOT come back. The transfer nets to zero across his
    /// own accounts; the charge is a real cost, and leaving it out would show a
    /// move that cost nothing.
    #[serde(default)]
    pub fee: f64,
    /// ★★★ An income already filed for the arriving leg, to be taken back off
    /// the books before this transfer credits the same account.
    ///
    /// Every text saying money arrived reads the same whether it came from an
    /// employer or from his own other account, so the arriving leg files itself
    /// as income long before the leaving leg is read. Booking the transfer on
    /// top of that would count the money twice -- once as earnings that never
    /// happened, once as the move it really was.
    #[serde(default)]
    pub undo_income: Option<String>,
}

/// What a transfer pass found, and what it would not guess at.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TransferReport {
    pub matched: Vec<MatchedTransfer>,
    /// ★★★ Legs addressed to one of his own numbers with no partner text.
    ///
    /// The other half may not have been read yet, or the second bank may not
    /// send one. Left in the queue as an ordinary question rather than made
    /// into a half transfer, which would move money out of one account and
    /// into nowhere.
    pub unpaired: u32,
    /// More than one candidate partner. Left alone, like everything else here.
    pub ambiguous: u32,
    /// ★★★ The partner leg exists but has ALREADY been filed as income.
    ///
    /// Every KCB text saying money arrived is mapped and applied the moment it
    /// is captured, long before the matching outgoing text may have been read.
    /// So by the time both halves are here, the income has already been
    /// recorded -- which is exactly the overstated income this whole mechanism
    /// exists to prevent, and it cannot be undone yet because there is no
    /// inverse of `budget.record_income`.
    ///
    /// Counted and reported rather than silently returning nothing, because a
    /// zero here would read as "no transfers found" when the truth is "found
    /// and cannot act on it yet". Undoing it needs `budget.unrecord_income`,
    /// and that needs each applied income to carry the id of the message that
    /// caused it so the right entry can be taken back out.
    pub blocked_by_applied_income: u32,
    /// Moves recognised from a single text that names both ends.
    pub self_moves: Vec<SelfMove>,
    /// ★★ Skipped messages a reference brought back into play. Counted so a
    /// queue that grew can say why, the same way one that shrank does.
    pub reclaimed: u32,
}

/// The numbers a household recognises as its own.
///
/// ★ Never leaves the device. There is no code path that sends this anywhere,
/// and there should not be one.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OwnIdentifiers {
    /// His own M-Pesa number, or numbers.
    #[serde(default)]
    pub mpesa: Vec<String>,
    /// His own KCB account number, or numbers.
    #[serde(default)]
    pub kcb: Vec<String>,
}

impl OwnIdentifiers {
    pub fn is_empty(&self) -> bool {
        self.mpesa.is_empty() && self.kcb.is_empty()
    }

    /// Does this identifier belong to him?
    ///
    /// ★★ Compared on the trailing digits, because the same phone is printed
    /// as `254712345678`, `0712345678` and `+254 712 345678` depending on who
    /// is printing it. Nine digits is the length of a Kenyan subscriber number
    /// without its country or trunk prefix, so it is the longest tail the three
    /// forms reliably share.
    pub fn claims(&self, candidate: &str) -> bool {
        let c = digits_of(candidate);
        if c.is_empty() {
            return false;
        }
        self.mpesa.iter().chain(self.kcb.iter()).any(|own| same_number(own, &c))
            || self.mpesa.iter().chain(self.kcb.iter()).any(|own| mask_matches(candidate, own))
    }

    /// Is this one of his M-Pesa numbers specifically?
    pub fn is_mpesa(&self, candidate: &str) -> bool {
        let c = digits_of(candidate);
        self.mpesa.iter().any(|own| same_number(own, &c) || mask_matches(candidate, own))
    }

    /// Is this one of his KCB accounts specifically?
    pub fn is_kcb(&self, candidate: &str) -> bool {
        let c = digits_of(candidate);
        self.kcb.iter().any(|own| same_number(own, &c) || mask_matches(candidate, own))
    }
}

/// Does this message name one of his own numbers as the other party?
///
/// ★ Checks every identifying field a rule extracts -- a phone, a bank account,
/// a paybill -- because which one a text carries depends on how the money was
/// sent, and a transfer is a transfer either way.
fn addresses_own(m: &IngestedMessage, own: &OwnIdentifiers) -> bool {
    // ★ Every field a rule can put an identifier in. `from_account` and
    //   `to_account` come from the shapes that name both ends of a move.
    ["phone", "account", "paybill", "from_account", "to_account"].iter().any(|f| {
        m.parsed_fields
            .get(*f)
            .and_then(|v| v.as_str())
            .is_some_and(|x| own.claims(x))
    })
}

fn digits_of(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// The smallest number of visible digits a masked identifier must show.
///
/// ★★★ A mask hides its middle, so all it proves is a prefix and a suffix.
/// Six visible digits is roughly a one-in-a-million coincidence for a number
/// of ordinary length, and it is what his own bank actually shows
/// (`135***140`, `254***143`). Below that the match is a guess, and a guess
/// here would call a real payment to another person a move between his own
/// accounts -- which would erase the expense entirely.
const MASK_MIN_VISIBLE: usize = 6;

/// Does a masked identifier describe this full number?
///
/// ★★ Only ever masked-against-full. Two masks are never compared to each
/// other: both hide their middles, so agreeing on the ends says nothing about
/// whether they are the same number.
fn mask_matches(masked: &str, full_digits: &str) -> bool {
    if !masked.contains('*') || full_digits.is_empty() {
        return false;
    }
    let (head, tail) = match masked.split_once('*') {
        Some((h, rest)) => (digits_of(h), digits_of(rest.trim_start_matches('*'))),
        None => return false,
    };
    if head.len() + tail.len() < MASK_MIN_VISIBLE {
        return false;
    }
    // The hidden middle must have somewhere to be.
    if full_digits.len() < head.len() + tail.len() {
        return false;
    }
    full_digits.starts_with(&head) && full_digits.ends_with(&tail)
}

/// Two written forms of the same number.
fn same_number(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let tail = |x: &str| -> String {
        let n = x.len().min(9);
        x[x.len() - n..].to_string()
    };
    // ★ Only when there are enough digits to be a number rather than a
    //   coincidence. A four-digit card mask must never claim a phone.
    a.len() >= 9 && b.len() >= 9 && tail(a) == tail(b)
}

/// How many characters a reference must have to be worth joining on.
///
/// ★★ Real M-Pesa codes are ten characters. Anything much shorter is a field
/// that happened to be called `ref` rather than a transaction's identity, and
/// joining two unrelated movements of money would be worse than joining
/// neither.
const MIN_REF_LEN: usize = 8;

/// What learning a skip did.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SkipLearned {
    pub key: Option<String>,
    /// How many already-waiting messages the new rule cleared.
    pub cleared: u32,
    /// ★★ True when this message cannot teach a rule: no parser recognised it,
    /// so the only shape it could describe is "everything I cannot read".
    pub unlearnable: bool,
}

/// A spend that landed, and where it currently sits.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FiledSpend {
    pub message_id: String,
    pub pocket: String,
    pub amount: f64,
    pub counterparty: String,
    pub raw: String,
    pub seq: u64,
}

/// What the bank charged for a move, if the text says.
///
/// ★ Absent is 0.0, not a guess. A shape whose rule extracts no cost is a shape
/// we do not know the cost of, and inventing one would be worse than omitting it.
fn fee_of(m: &IngestedMessage) -> f64 {
    m.parsed_fields
        .get("transaction_cost")
        .and_then(|v| {
            v.as_f64().or_else(|| v.as_str().and_then(|s| s.replace(',', "").parse().ok()))
        })
        .filter(|f| *f > 0.0)
        .unwrap_or(0.0)
}

/// A balance a bank itself reported, and when.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Reported {
    pub balance: f64,
    pub at: i64,
}

/// The running balance a message states, and **which account it belongs to**.
///
/// ★★★ **One sender is not one account.** Pochi la Biashara and M-Shwari arrive
/// from the SAME M-Pesa sender as the main wallet, so keying a reported balance
/// by `source_id` collapses three different pots into one and shows whichever
/// text landed last. The rules already separate them — they carry an
/// `instrument` — so the account is read from the message rather than guessed
/// from the sender.
///
/// ★★ **`loan_balance` is deliberately not a balance here.** KCB's loan texts
/// state what is OWED, and a household that saw its debt reported as money it
/// holds would be looking at the most dangerous wrong number this app could
/// print. Absent from the list on purpose, not by omission.
///
/// ★ Order matters: an instrument's own field is read before `balance_after`,
/// because `mpesa_pochi_received` states the **business** balance in
/// `balance_after` — so the field name alone would attribute it to the wrong
/// pot, and the instrument is what settles it.
fn reported_account_of(m: &IngestedMessage) -> Option<(String, f64)> {
    let number = |v: &serde_json::Value| {
        v.as_f64().or_else(|| v.as_str().and_then(|s| s.replace(',', "").parse().ok()))
    };
    let balance = ["pochi_balance", "mshwari_balance", "actual_balance", "balance_after"]
        .iter()
        .find_map(|f| m.parsed_fields.get(*f).and_then(number))?;

    // The pot this message is about: its own instrument, or the sender's main
    // wallet when it names none.
    let account = m
        .parsed_fields
        .get("instrument")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| m.source_id.clone());

    Some((account, balance))
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
    /// When it last spoke, in real time rather than in sequence.
    ///
    /// ★★ A sequence number says which capture came last; it cannot say how
    /// long ago. Staleness is a question about elapsed time, so it needs one.
    #[serde(default)]
    pub last_seen_ms: Option<i64>,
    /// The gaps this source has actually kept between messages.
    ///
    /// ★★★ Its OWN history, which is what makes a learned threshold not a
    /// guess. Bounded, because a source that has spoken ten thousand times does
    /// not need all ten thousand gaps to say what its cadence is — and an
    /// unbounded list is a file that grows forever.
    #[serde(default)]
    pub recent_gaps_ms: Vec<i64>,
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
    /// Older than where this household's record begins.
    ///
    /// ★★★ NOT stored, not queued, not counted as a rejection: it never
    /// crossed the boundary at all. A phone carries years of texts, and
    /// starting Sustena on Tuesday does not mean the household began when the
    /// handset did.
    BeforeStart { at: i64, start: i64 },
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
    fn skips_path(&self) -> PathBuf {
        self.root.join("skip_rules.json")
    }

    fn own_path(&self) -> PathBuf {
        self.root.join("own_identifiers.json")
    }

    fn history_path(&self) -> PathBuf {
        self.root.join("history.json")
    }
    fn marks_path(&self) -> PathBuf {
        self.root.join("read_marks.json")
    }

    /// **Is this source still alive?**
    ///
    /// ★★★ A declared cadence wins over a learned one — a promise beats an
    /// estimate. With neither, the answer is **unknown**: not healthy, because
    /// that would hide a dead capture path, and not stale, because that would
    /// cry wolf about a source that is simply new.
    pub fn liveness_of(&self, source_id: &str, now_ms: i64) -> StoreResult<sustena_core::Liveness> {
        let Some(source) = self.sources()?.into_iter().find(|s| s.id == source_id) else {
            return Ok(sustena_core::Liveness::Unknown {
                why: "no such source has ever reported".into(),
            });
        };
        let Some(last) = source.last_seen_ms else {
            return Ok(sustena_core::Liveness::Unknown {
                why: "it has never reported a time".into(),
            });
        };
        let heartbeat = source.expected_interval_minutes.map(|m| {
            let every = m as i64 * 60_000;
            // ★★ A tenth of the promised interval as grace. Declared here as a
            //    policy of this node rather than defaulted inside the core,
            //    where nobody would see it.
            sustena_core::Heartbeat::new(every, every / 10)
        });
        Ok(sustena_core::source_liveness(
            now_ms - last,
            heartbeat,
            &source.recent_gaps_ms,
            STALENESS_PERCENTILE,
        ))
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
    /// Where this household's record begins.
    ///
    /// ★ Read from disk each time rather than cached: it changes from a
    /// surface, and a capture running on the cached old value would admit
    /// exactly the backlog the person just excluded.
    pub fn window(&self) -> sustena_core::intake_window::IntakeWindow {
        std::fs::read_to_string(self.window_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Move where the record begins. ★ Atomically, like every other setting
    /// here: a half-written cutoff would read as open and re-admit the backlog.
    pub fn set_window(
        &self,
        window: sustena_core::intake_window::IntakeWindow,
    ) -> StoreResult<()> {
        let path = self.window_path();
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&window)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        std::fs::write(&tmp, text).map_err(|e| StoreError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(())
    }

    fn window_path(&self) -> PathBuf {
        self.root.join("intake_window.json")
    }

    pub fn capture(
        &self,
        sustain_id: &str,
        source_id: &str,
        raw: &str,
        extra_rules: &[ParseRule],
    ) -> StoreResult<Capture> {
        self.capture_at(sustain_id, source_id, raw, extra_rules, None)
    }

    /// Capture, carrying when the phone says the message arrived.
    pub fn capture_at(
        &self,
        sustain_id: &str,
        source_id: &str,
        raw: &str,
        extra_rules: &[ParseRule],
        sent_at_ms: Option<i64>,
    ) -> StoreResult<Capture> {
        // ★★★ **The intake boundary, in time — before τ runs at all.**
        //
        //     The Ingest paper makes τ total over what crosses the boundary.
        //     This decides what crosses. A message from before the household
        //     existed is not classified-and-skipped, it is not admitted: no
        //     record, no queue entry, and a classify queue that does not open
        //     with two thousand decisions nobody asked for.
        //
        // ★ Deliberately ahead of the secret gate too. Not admitting is
        //   strictly less than refusing, and costs a parse we do not need.
        let window = self.window();
        let admission = window.admission(sent_at_ms.map(|ms| ms / 1000));
        if let sustena_core::intake_window::Admission::Before { at, start } = admission {
            return Ok(Capture::BeforeStart { at, start });
        }

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
            netted_with: None,
            same_event_as: None,
            reclaimed: false,
            deferred_at: None,
            filed: Vec::new(),
            sent_at_ms,
            // ★★ The offset is declared here, at the host, because it is a fact
            //    about THIS node's senders rather than something the core
            //    should assume on everyone's behalf.
            event_at_ms: sustena_core::event_time(&t.parsed_fields())
                .map(|at| at.to_epoch_ms(SENDER_UTC_OFFSET_MINUTES)),
            seq,
        };
        self.append_message(&message)?;
        // ★★ When the message says it happened, falling back to when the
        //    device read it. Either is a real reading; neither is invented.
        self.mark_seen(source_id, seq, message.event_at_ms.or(message.sent_at_ms))?;
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

    // ── who you are, to your own bank ────────────────────────────────────────
    //
    // ★★★ Sending money from KCB to M-Pesa produces two texts that each look
    // like an ordinary transaction: one says money left, the other says money
    // arrived. Taking them at face value books an expense and an income that
    // never happened, and the household's income figure grows every time
    // someone moves their own money.
    //
    // Telling that apart from a real payment to someone else needs one fact
    // nothing else can supply: which numbers are HIS. A phone number and an
    // account number are the addresses of a person, so they stay on the device,
    // are never sent anywhere, and are only ever typed by him.

    /// The phone and account numbers this household recognises as its own.
    pub fn own_identifiers(&self) -> StoreResult<OwnIdentifiers> {
        let path = self.own_path();
        if !path.exists() {
            return Ok(OwnIdentifiers::default());
        }
        Ok(serde_json::from_slice(&fs::read(&path)?).unwrap_or_default())
    }

    /// Record them. Digits only, so a number typed with spaces or a `+` still
    /// matches the bare digits a bank prints.
    pub fn set_own_identifiers(&self, own: &OwnIdentifiers) -> StoreResult<()> {
        let cleaned = OwnIdentifiers {
            mpesa: own.mpesa.iter().map(|x| digits_of(x)).filter(|x| !x.is_empty()).collect(),
            kcb: own.kcb.iter().map(|x| digits_of(x)).filter(|x| !x.is_empty()).collect(),
        };
        let tmp = self.own_path().with_extension("json.tmp");
        let text =
            serde_json::to_string_pretty(&cleaned).map_err(|e| StoreError::Io(e.to_string()))?;
        fs::write(&tmp, text)?;
        fs::rename(&tmp, self.own_path())?;
        Ok(())
    }

    /// A text that names both ends of a move, where both ends are his.
    ///
    /// ★★★ BOTH must be his. A text saying money went from his account to
    /// someone else's is a real payment, and calling it a transfer would erase
    /// the expense. Requiring both ends is what keeps a masked number -- which
    /// only ever proves a prefix and a suffix -- from being enough on its own.
    fn self_move_of(&self, m: &IngestedMessage, own: &OwnIdentifiers) -> Option<SelfMove> {
        let from = m.parsed_fields.get("from_account")?.as_str()?;
        let to = m.parsed_fields.get("to_account")?.as_str()?;
        let amount = m.amount()?;
        if !own.claims(from) || !own.claims(to) {
            return None;
        }
        // Which side is which, read off what he declared rather than assumed
        // from the text's own wording.
        let from_account = if own.is_kcb(from) { "kcb" } else { "mpesa" };
        let to_account = if own.is_mpesa(to) { "mpesa" } else { "kcb" };
        if from_account == to_account {
            // Both ends resolved to the same account, so nothing would move.
            return None;
        }
        Some(SelfMove {
            message: m.id.clone(),
            from_account: from_account.to_string(),
            to_account: to_account.to_string(),
            amount,
            fee: fee_of(m),
            undo_income: None,
            reclaimed_from_skip: false,
        })
    }

    /// Find the pairs of texts that are really one move between his own accounts.
    ///
    /// ★★★ A transfer is recognised by ADDRESS, not by wording. "Sent to
    /// 254712345678" and "sent to MARY" are the same sentence; the only thing
    /// that separates a move between his own accounts from a payment to a
    /// friend is whether that number is his. Nothing is matched at all until he
    /// has said which numbers those are.
    ///
    /// ★★ Both halves must be present. One leg alone stays an ordinary
    /// question: booking it as a transfer would move money into an account no
    /// text ever confirmed it reached.
    pub fn find_transfers(&self, sustain_id: &str) -> StoreResult<TransferReport> {
        let own = self.own_identifiers()?;
        let mut report = TransferReport::default();
        if own.is_empty() {
            // Nothing has been declared his, so nothing can be his own move.
            return Ok(report);
        }

        let everything: Vec<IngestedMessage> =
            self.current()?.into_iter().filter(|m| m.sustain_id == sustain_id).collect();
        let queue: Vec<IngestedMessage> =
            everything.iter().filter(|m| m.needs_attention()).cloned().collect();

        // ★★★ Legs he SKIPPED are candidates again, but only by reference.
        //
        //     He waved past both halves of a real transfer impatiently, and
        //     they are gone from the queue for good under the old rule. A
        //     shared transaction reference is proof they were one movement of
        //     money, and proof is allowed to overturn a judgement where a
        //     resemblance is not. Anything genuinely not a transaction quotes
        //     no reference another message shares, so it stays where he put it.
        let reclaimable: Vec<IngestedMessage> = everything
            .iter()
            .filter(|m| Self::was_skipped(m) && m.reference().is_some())
            .cloned()
            .collect();

        // Only the legs that name one of his own numbers are in play at all.
        let mine: Vec<&IngestedMessage> = queue
            .iter()
            .chain(reclaimable.iter())
            .filter(|m| addresses_own(m, &own))
            .collect();
        let all_mine: Vec<&IngestedMessage> =
            everything.iter().filter(|m| addresses_own(m, &own)).collect();

        let dir = |m: &IngestedMessage| -> Option<String> {
            m.parsed_fields.get("direction").and_then(|v| v.as_str()).map(str::to_string)
        };

        // ── one text, both ends ──────────────────────────────────────────
        let mut taken: BTreeSet<String> = BTreeSet::new();
        for m in &mine {
            let Some(mut mv) = self.self_move_of(m, &own) else { continue };
            // ★★ A single text naming both ends is already proof enough of
            //    WHAT it is; but if he set it aside, only a reference gets it
            //    back, and `reclaimable` is already filtered on having one.
            mv.reclaimed_from_skip = Self::was_skipped(m);
            if mv.reclaimed_from_skip {
                report.reclaimed += 1;
            }
            // ★★ The other side may already have filed itself as income: an
            //    M-Pesa "you have received" for the same money, captured first.
            //    Left standing it would count his own money as earnings, so the
            //    entry to undo is named here and undone by the caller.
            let candidates: Vec<&IngestedMessage> = everything
                .iter()
                .filter(|o| o.id != m.id && o.applied && o.netted_with.is_none())
                .filter(|o| o.operator.as_deref() == Some("budget.record_income"))
                .collect();
            // ★★★ The ref first, and it is not merely the better guess — it is
            //     a different kind of answer. Amount plus account asks whether
            //     these COULD be the same movement; a shared reference says
            //     they ARE. Only when neither text quoted one does this fall
            //     back to the weaker question.
            mv.undo_income = m
                .reference()
                .and_then(|r| candidates.iter().find(|o| o.reference() == Some(r)))
                .or_else(|| {
                    candidates
                        .iter()
                        .filter(|o| o.source_id == mv.to_account)
                        .find(|o| o.amount().map(|a| same_amount(a, mv.amount)).unwrap_or(false))
                })
                .map(|o| o.id.clone());
            taken.insert(m.id.clone());
            report.self_moves.push(mv);
        }

        // ── two texts, one each ──────────────────────────────────────────
        for out in mine.iter().filter(|m| dir(m).as_deref() == Some("sent")) {
            if taken.contains(&out.id) {
                continue;
            }
            let Some(amount) = out.amount() else {
                report.unpaired += 1;
                continue;
            };
            // The other half: money arriving, same amount, a different account.
            // The partner may already have been filed as income at capture
            // time. Look for that too, so the answer is "found, blocked"
            // rather than a bare nothing.
            let applied_partner = all_mine
                .iter()
                .filter(|m| m.id != out.id && !m.needs_attention())
                .filter(|m| dir(m).as_deref() == Some("received"))
                .filter(|m| m.source_id != out.source_id)
                .filter(|m| m.amount().map(|a| same_amount(a, amount)).unwrap_or(false))
                .count();

            let open: Vec<&&IngestedMessage> = mine
                .iter()
                .filter(|m| !taken.contains(&m.id) && m.id != out.id)
                .filter(|m| dir(m).as_deref() == Some("received"))
                .filter(|m| m.source_id != out.source_id)
                .collect();

            // ★★★ A shared reference is proof; amount alone is a coincidence
            //     waiting to happen. Two identical transfers to the same person
            //     on the same day are hopeless for the second test and trivial
            //     for the first.
            let by_ref: Vec<&&IngestedMessage> = match out.reference() {
                Some(r) => open.iter().filter(|m| m.reference() == Some(r)).copied().collect(),
                None => Vec::new(),
            };
            let partners: Vec<&&IngestedMessage> = if by_ref.is_empty() {
                // ★★★ The weaker question may only be asked of messages still
                //     in the queue. Overturning a skip on a resemblance would
                //     be undoing his decision on a guess.
                open.into_iter()
                    .filter(|m| !Self::was_skipped(m))
                    .filter(|m| m.amount().map(|a| same_amount(a, amount)).unwrap_or(false))
                    .collect()
            } else {
                by_ref
            };

            match partners.len() {
                1 => {
                    let inn = partners[0];
                    taken.insert(out.id.clone());
                    taken.insert(inn.id.clone());
                    report.matched.push(MatchedTransfer {
                        out_leg: out.id.clone(),
                        in_leg: inn.id.clone(),
                        from_account: out.source_id.clone(),
                        to_account: inn.source_id.clone(),
                        amount,
                        fee: fee_of(out),
                        // Both legs are still open, so nothing has been filed.
                        undo_income: None,
                    });
                }
                0 if applied_partner > 0 => {
                    // ★★★ The arriving leg already filed itself as income. That
                    //     is not a reason to do nothing -- doing nothing IS the
                    //     overstatement, and it sits on his totals until
                    //     somebody notices. There is an inverse now, so the
                    //     move can be booked properly: take the income back
                    //     off, then record the transfer.
                    //
                    // ★★ ONLY on a shared reference, never on amount alone.
                    //    This path UNDOES something already on the books, so
                    //    it may act only on proof that the two texts are one
                    //    event. A wrong join here would delete a real income.
                    let filed = out.reference().and_then(|r| {
                        all_mine
                            .iter()
                            .filter(|m| m.id != out.id && m.applied && m.netted_with.is_none())
                            .filter(|m| m.operator.as_deref() == Some("budget.record_income"))
                            .filter(|m| m.source_id != out.source_id)
                            .find(|m| m.reference() == Some(r))
                    });
                    match filed {
                        Some(inn) => {
                            taken.insert(out.id.clone());
                            taken.insert(inn.id.clone());
                            report.matched.push(MatchedTransfer {
                                out_leg: out.id.clone(),
                                in_leg: inn.id.clone(),
                                from_account: out.source_id.clone(),
                                to_account: inn.source_id.clone(),
                                amount,
                                fee: fee_of(out),
                                undo_income: Some(inn.id.clone()),
                            });
                        }
                        // Amount-only resemblance to something already filed:
                        // still reported, still not acted on.
                        None => report.blocked_by_applied_income += 1,
                    }
                }
                0 => report.unpaired += 1,
                _ => report.ambiguous += 1,
            }
        }
        Ok(report)
    }

    /// Record that a single text was a whole move, so it stops asking.
    pub fn mark_self_moved(&self, id: &str) -> StoreResult<()> {
        // ★ Paired with itself: the fact recorded is "this was a move", and
        //   there is no second message to point at.
        self.mark_netted(id, id)
    }

    /// Record that two texts were one move, so neither asks again.
    ///
    /// ★★ Called only after the transfer really committed, the same discipline
    /// the refunds follow: marking a pair settled before the gate has spoken
    /// would leave them looking handled with no money moved.
    pub fn mark_transferred(&self, out_leg: &str, in_leg: &str) -> StoreResult<()> {
        self.mark_netted(out_leg, in_leg)?;
        self.mark_netted(in_leg, out_leg)
    }

    /// The balance each source most recently reported, and when it said so.
    ///
    /// ★★★ Every real M-Pesa and KCB text ends by stating the account's new
    /// balance. That is the bank's own word for what is there, and it is the
    /// one figure in this whole system that does not come from our own
    /// arithmetic -- which makes it the only thing that can check it.
    ///
    /// ★★ Ordered by when the message was SENT, never by capture order. A
    /// backlog is read newest-first, so trusting capture order would take the
    /// OLDEST balance as the freshest word and report drift that is really
    /// just history. A message with no arrival time is skipped rather than
    /// assumed recent, for the same reason.
    pub fn reported_balances(&self, sustain_id: &str) -> StoreResult<BTreeMap<String, Reported>> {
        let mut out: BTreeMap<String, Reported> = BTreeMap::new();
        for m in self.current()? {
            if m.sustain_id != sustain_id {
                continue;
            }
            let (Some(at), Some((account, balance))) = (m.sent_at_ms, reported_account_of(&m))
            else {
                continue;
            };
            let e = out.entry(account).or_insert(Reported { balance, at });
            if at > e.at {
                *e = Reported { balance, at };
            }
        }
        Ok(out)
    }

    /// A message already on file that describes this same real transaction.
    ///
    /// ★★★ The cross-source double count, and the reason it stayed open. One
    /// payment can produce two texts from two senders — M-Pesa's own
    /// confirmation and the bank's notice about the same movement. The dedup
    /// key is a hash of source plus raw text, so the two hash differently and
    /// both are captured and both applied: the money counted twice.
    ///
    /// It was deferred for want of a reliable correlation key, and the
    /// reference IS that key. Two texts quoting the same transaction reference
    /// are the same transaction, whoever sent them and however differently
    /// they word it.
    ///
    /// ★★ Only ever ACROSS sources. The same source repeating a reference is
    /// its own business — a balance notice quoting an earlier transaction, say
    /// — and the raw-text dedup already covers a literal repeat.
    pub fn same_event_already_applied(
        &self,
        sustain_id: &str,
        source_id: &str,
        reference: &str,
    ) -> StoreResult<Option<String>> {
        self.same_fact_already_applied(sustain_id, source_id, reference, None)
    }

    /// The same question, with the AMOUNT as part of the key.
    ///
    /// ★★★ **Matching on a reference alone is not matching on a fact**, and
    /// getting this wrong is worse than not having it. Reference codes are
    /// allocated by different systems and can collide; when they do, a real
    /// separate transaction is silently treated as already-applied and
    /// **vanishes without trace** — nobody finds out, because the whole effect
    /// of the join is that nothing is shown.
    ///
    /// ★★ So the amount has to agree too. Two payments of forty thousand on one
    /// day are ordinary; a shared reference is ordinary; both together is a
    /// coincidence worth acting on. This strictly *narrows* what gets joined,
    /// which is the safe direction — a missed join shows a duplicate a person
    /// can undo, a wrong join hides a transaction nobody knows to look for.
    pub fn same_fact_already_applied(
        &self,
        sustain_id: &str,
        source_id: &str,
        reference: &str,
        amount: Option<f64>,
    ) -> StoreResult<Option<String>> {
        if reference.len() < MIN_REF_LEN {
            return Ok(None);
        }
        Ok(self
            .current()?
            .into_iter()
            .filter(|m| m.sustain_id == sustain_id && m.source_id != source_id)
            .filter(|m| m.applied)
            .filter(|m| m.reference() == Some(reference))
            .find(|m| match (amount, message_amount(m)) {
                // ★★ Both sides know the amount: they must agree.
                (Some(want), Some(have)) => (want - have).abs() <= 0.005,
                // ★★ One side does not: fall back to the reference alone rather
                //    than refusing to join. That is the behaviour that shipped,
                //    and narrowing it further would start missing real pairs
                //    for messages whose rules extract no amount.
                _ => true,
            })
            .map(|m| m.id))
    }

    /// The spends this household has filed, newest first.
    ///
    /// ★★★ Built from what each message recorded it DID, not from the state
    /// it produced. `filed` has carried the operator and its params since the
    /// refund work needed to undo one; a spend that landed is therefore
    /// already on the record with its pocket and its amount, and this reads it
    /// back rather than inferring it from a balance.
    ///
    /// ★★ Only spends, and only the ones still standing. An allocation is not
    /// a filing anyone can get wrong in this sense, and a message already
    /// corrected or netted is not the one to correct again.
    pub fn filed_spends(&self, sustain_id: &str, limit: usize) -> StoreResult<Vec<FiledSpend>> {
        let mut out: Vec<FiledSpend> = Vec::new();
        for m in self.current()? {
            if m.sustain_id != sustain_id || !m.resolved || m.netted_with.is_some() {
                continue;
            }
            // The LAST spend recorded against the message is where it stands
            // now — a correction appends, so the newest one is current.
            let Some(f) = m.filed.iter().rev().find(|f| f.operator == "budget.spend") else {
                continue;
            };
            let Some(pocket) = f.params.get("pocket_name").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(amount) = f.params.get("amount").and_then(|v| v.as_f64()) else {
                continue;
            };
            out.push(FiledSpend {
                message_id: m.id.clone(),
                pocket: pocket.to_string(),
                amount,
                counterparty: m.counterparty().unwrap_or("").to_string(),
                raw: m.raw_payload.clone(),
                seq: m.seq,
            });
        }
        out.sort_by_key(|m| std::cmp::Reverse(m.seq));
        out.truncate(limit);
        Ok(out)
    }

    /// Note that a filing was corrected, so the list shows where it stands now.
    ///
    /// ★★ Appended, like everything else here. The original filing stays in
    /// `filed`; this adds the move after it, and `filed_spends` reads the last
    /// one — so the record keeps both and the screen shows the current answer.
    pub fn record_reclassification(
        &self,
        id: &str,
        to_pocket: &str,
        amount: f64,
    ) -> StoreResult<()> {
        let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        params.insert("pocket_name".into(), serde_json::json!(to_pocket));
        params.insert("amount".into(), serde_json::json!(amount));
        self.record_filing(id, "budget.spend", &params)
    }

    /// Which account a captured message's money moved in.
    ///
    /// ★ The source id IS the account name: a text from M-Pesa is money moving
    /// in M-Pesa. That identity is what makes attribution free rather than one
    /// more question per message.
    pub fn source_of_message(&self, id: &str) -> Option<String> {
        self.current().ok()?.into_iter().find(|m| m.id == id).map(|m| m.source_id)
    }

    /// Note a real operator call made about this message.
    ///
    /// ★★ Deliberately does NOT resolve it. Filing a past charge takes two
    /// calls, and the message is only dealt with after the second. Resolution
    /// stays `record_outcome`'s decision alone.
    pub fn record_filing(
        &self,
        id: &str,
        operator: &str,
        params: &BTreeMap<String, serde_json::Value>,
    ) -> StoreResult<()> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(());
        };
        m.filed.push(Filing { operator: operator.to_string(), params: params.clone() });
        self.append_message(&m)
    }

    /// Note that this message describes a transaction already applied.
    ///
    /// ★★ It keeps its text and its own reason. Nothing is deleted and nothing
    /// is silently swallowed: "your other bank already told me about this" is
    /// a real answer, and one worth being able to read.
    pub fn note_same_event(&self, id: &str, first: &str) -> StoreResult<()> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(());
        };
        m.same_event_as = Some(first.to_string());
        m.reason = "Already recorded from your other bank — the same transaction, \
                    reported twice."
            .to_string();
        self.append_message(&m)
    }

    // ── learned skips ────────────────────────────────────────────────────────
    //
    // ★★★ Across two and a half thousand messages, the same handful of shapes
    // repeat: a promo, a balance notice, an advert. Setting each aside one at a
    // time is the same decision made hundreds of times, and a person who has
    // already answered it twice is right to be annoyed the third time.
    //
    // ★★ Keyed on the SOURCE and the PARSER, never on the raw text. Raw text
    // never repeats exactly — the amounts and dates differ — so a text key
    // would learn nothing. The parser name is the shape the transducer
    // recognised, which is exactly "messages like this one".
    //
    // ★★★ An unparsed message can never teach a skip. `parser_name` is empty
    // for anything no rule matched, so a rule keyed on it would mean "skip
    // everything I cannot read" — which is precisely the pile that most needs
    // a person's eyes.

    /// The shapes this household has said it never wants to see.
    pub fn skip_rules(&self) -> StoreResult<BTreeSet<String>> {
        let path = self.skips_path();
        if !path.exists() {
            return Ok(BTreeSet::new());
        }
        Ok(serde_json::from_slice(&fs::read(&path)?).unwrap_or_default())
    }

    /// The key a message would be skipped by, if it can teach one at all.
    pub fn skip_key(m: &IngestedMessage) -> Option<String> {
        let parser = m.parser_name.trim();
        if !parser.is_empty() {
            return Some(format!("{}::{}", m.source_id, parser));
        }
        // ★★★ **A shape nothing can READ can still be one he never wants to be
        //     asked about.** This returned None for an unparsed message, so
        //     "never ask me about these again" was impossible for precisely the
        //     messages that ask most: his inbox holds a cluster of 232 alike
        //     texts nothing recognises, and the only answer available for them
        //     was to dismiss them one at a time.
        //
        // ★★ The skeleton is the same shape key the corpus clusters on, so the
        //    rule he teaches covers exactly the group he was shown -- not a
        //    wider net he did not agree to.
        let shape = sustena_core::corpus::skeleton(&m.raw_payload);
        if shape.is_empty() {
            return None;
        }
        use sha2::{Digest as _, Sha256};
        let digest = format!("{:x}", Sha256::digest(shape.as_bytes()));
        Some(format!("{}::shape::{}", m.source_id, &digest[..16]))
    }

    /// Learn "never ask me about these again", and apply it to what is already
    /// waiting.
    ///
    /// ★★ Retroactive on purpose. He is answering this question in the middle
    /// of a backlog full of the same shape; a rule that only applied to future
    /// messages would leave the pile it was meant to clear exactly as it was.
    /// Returns how many it cleared, because a silent sweep of the queue is
    /// alarming.
    pub fn learn_skip(&self, sustain_id: &str, from_message: &str) -> StoreResult<SkipLearned> {
        let all = self.current()?;
        let Some(source) = all.iter().find(|m| m.id == from_message) else {
            return Ok(SkipLearned::default());
        };
        let Some(key) = Self::skip_key(source) else {
            return Ok(SkipLearned { unlearnable: true, ..Default::default() });
        };

        let mut rules = self.skip_rules()?;
        rules.insert(key.clone());
        let text = serde_json::to_string_pretty(&rules)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let tmp = self.skips_path().with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, self.skips_path())?;

        let mut cleared = 0u32;
        for m in all {
            if m.sustain_id != sustain_id || !m.needs_attention() {
                continue;
            }
            if Self::skip_key(&m).as_deref() == Some(key.as_str()) {
                self.ignore(&m.id)?;
                cleared += 1;
            }
        }
        Ok(SkipLearned { key: Some(key), cleared, unlearnable: false })
    }

    /// Does a learned rule already cover this message?
    pub fn is_skipped(&self, m: &IngestedMessage) -> StoreResult<bool> {
        let Some(key) = Self::skip_key(m) else { return Ok(false) };
        Ok(self.skip_rules()?.contains(&key))
    }

    /// Forget a learned skip. Nothing already set aside comes back on its own.
    pub fn forget_skip(&self, key: &str) -> StoreResult<bool> {
        let mut rules = self.skip_rules()?;
        if !rules.remove(key) {
            return Ok(false);
        }
        let text = serde_json::to_string_pretty(&rules)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let tmp = self.skips_path().with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, self.skips_path())?;
        Ok(true)
    }

    /// Take back a skip, because its other half turned up.
    ///
    /// ★★★ Only ever called on a REFERENCE match. Amount and merchant say two
    /// texts could be the same movement; a shared reference says they are, and
    /// nothing weaker is allowed to overturn a decision he made. A genuine
    /// "not a transaction" — a promo, a balance notice — quotes no reference
    /// that another message shares, so it stays skipped for good.
    pub fn reclaim(&self, id: &str) -> StoreResult<bool> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(false);
        };
        if !m.ignored {
            return Ok(false);
        }
        m.ignored = false;
        m.reclaimed = true;
        self.append_message(&m)?;
        Ok(true)
    }

    /// Was this set aside by a person, rather than answered or netted?
    pub fn was_skipped(m: &IngestedMessage) -> bool {
        m.ignored && m.netted_with.is_none() && !m.resolved
    }

    /// Put a message off until he remembers what it was.
    ///
    /// ★★ It stays in the queue. Deferring is not answering, so
    /// `needs_attention` is deliberately untouched — what changes is only the
    /// ORDER it comes back in.
    pub fn defer(&self, id: &str, at_seq: u64) -> StoreResult<bool> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(false);
        };
        m.deferred_at = Some(at_seq);
        self.append_message(&m)?;
        Ok(true)
    }

    /// The queue in the order he should meet it.
    ///
    /// ★★★ **NEWEST FIRST within the waiting queue.** The just-arrived
    /// transaction is the one he was notified about and the one he still
    /// remembers making, so it is the cheapest to answer — recall is at its
    /// best within minutes and decays fast. An oldest-first queue asks the
    /// hardest question first and makes the pile feel like homework.
    ///
    /// ★★ Deferred still leads the waiting group, oldest deferral first. He
    /// put those off because he could not answer them yet, and burying them
    /// under new arrivals would make deferring indistinguishable from
    /// discarding. Newest-first applies **within** each group rather than
    /// flattening the two — the ordering rule is about recency among things
    /// competing equally, not about overriding a decision he already made.
    /// (A notification tap does not depend on this order at all: it carries
    /// its own target and opens that card directly.)
    ///
    /// ★★ Recently answered messages come BEFORE both, so the back arrow
    /// reaches them. A filing he wants to change is otherwise unreachable, and
    /// a correction path with nothing to correct from is not a path.
    pub fn navigable(&self, sustain_id: &str, done: usize, limit: usize) -> StoreResult<Vec<IngestedMessage>> {
        let all: Vec<IngestedMessage> =
            self.current()?.into_iter().filter(|m| m.sustain_id == sustain_id).collect();

        let mut processed: Vec<IngestedMessage> = all
            .iter()
            .filter(|m| m.resolved && m.netted_with.is_none())
            .cloned()
            .collect();
        processed.sort_by_key(|m| m.seq);
        // Only the tail of what he has done: the back arrow is for changing his
        // mind about something recent, not for browsing a year.
        if processed.len() > done {
            processed.drain(..processed.len() - done);
        }

        let mut waiting: Vec<IngestedMessage> =
            all.into_iter().filter(|m| m.needs_attention()).collect();
        waiting.sort_by_key(|m| (m.deferred_at.is_none(), m.deferred_at, Reverse(m.seq)));

        // ★★★ **The cap must never hide a capture that just arrived.**
        //     Order is unchanged -- something put off still comes back first,
        //     which is the whole meaning of deferring. What was wrong is that
        //     `take(limit)` counted them together: press "later" enough and
        //     the postponed ones fill the window, so a just-captured text
        //     falls off the end and looks to a household exactly like a
        //     capture that never happened. That was the report.
        //
        // ★★ So the postponed group yields room rather than the fresh one.
        //    A thing set aside is a thing he has already seen; a thing that
        //    just arrived, he has not.
        let (put_off, fresh): (Vec<IngestedMessage>, Vec<IngestedMessage>) =
            waiting.into_iter().partition(|m| m.deferred_at.is_some());
        let room = limit.saturating_sub(fresh.len());
        let mut out = processed;
        out.extend(put_off.into_iter().take(room));
        out.extend(fresh.into_iter().take(limit));
        Ok(out)
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
                last_seen_ms: None,
                recent_gaps_ms: Vec::new(),
                captures: 0,
            }),
        }
        self.write_sources(&all)
    }

    fn mark_seen(&self, id: &str, seq: u64, at_ms: Option<i64>) -> StoreResult<()> {
        let mut all = self.sources()?;
        match all.iter_mut().find(|s| s.id == id) {
            Some(s) => {
                // ★ Stamped on EVERY capture — mapped, unmapped or unparsed
                //   alike. Staleness is about whether a source is alive, not
                //   whether any one message happened to parse.
                // ★★ The gap BEFORE overwriting, or there is nothing to
                //    subtract from.
                if let (Some(previous), Some(now)) = (s.last_seen_ms, at_ms) {
                    let gap = now - previous;
                    // ★★★ A negative gap is a clock that moved backwards, not a
                    //     cadence. Recording it would poison the percentile with
                    //     a number no source ever kept.
                    if gap > 0 {
                        s.recent_gaps_ms.push(gap);
                        if s.recent_gaps_ms.len() > MAX_TRACKED_GAPS {
                            let excess = s.recent_gaps_ms.len() - MAX_TRACKED_GAPS;
                            s.recent_gaps_ms.drain(..excess);
                        }
                    }
                }
                s.last_seen_seq = Some(seq);
                if at_ms.is_some() {
                    s.last_seen_ms = at_ms;
                }
                s.captures += 1;
            }
            None => all.push(Source {
                id: id.to_string(),
                label: id.to_string(),
                expected_interval_minutes: None,
                last_seen_seq: Some(seq),
                last_seen_ms: at_ms,
                recent_gaps_ms: Vec::new(),
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

    /// Read the messages nothing could read, now that a rule exists for them.
    ///
    /// ★★★ **It improves READABILITY and never moves money.** A fresh capture
    /// that maps to income applies itself, and that is right for a text that
    /// just arrived. Doing the same to a backlog is not the same act: these
    /// messages have been sitting for days, some may already have been recorded
    /// by hand, and applying eleven of them because one rule was taught could
    /// double-count a household's month in a single tap. So a re-parsed message
    /// is only ever left `parsed_unmapped` -- readable, with its operator and
    /// amount filled in, and still requiring the person to confirm.
    ///
    /// ★★ That keeps the money-safety asymmetry pointing the same way it
    /// always has: the engine may read without asking, and may not spend
    /// without asking.
    ///
    /// ★ Untouched: anything ignored, resolved, or already parsed. Re-reading
    /// a message somebody already dealt with would undo their answer.
    ///
    /// Returns how many became readable.
    pub fn reparse_unparsed(
        &self,
        sustain_id: &str,
        rules: &[ParseRule],
    ) -> StoreResult<usize> {
        let mut changed = 0usize;
        for m in self.current()? {
            if m.sustain_id != sustain_id || m.status != "unparsed" || m.ignored || m.resolved {
                continue;
            }
            let t = parse_message(&m.raw_payload, Some(&m.source_id), rules);
            // Still unreadable, or a secret: leave it exactly as it was.
            if t.operator().is_none() && t.parsed_fields().is_empty() {
                continue;
            }
            let mut next = m.clone();
            // ★★★ Never "mapped" here -- see above. Readable, not filed.
            next.status = "parsed_unmapped".to_string();
            next.parser_name = t.parser_name().to_string();
            next.reason = t.reason().to_string();
            next.parsed_fields = t.parsed_fields();
            next.external_ref = t.external_ref().map(str::to_string);
            next.operator = t.operator().map(str::to_string);
            next.params = t.operator_params();
            self.append_message(&next)?;
            changed += 1;
        }
        Ok(changed)
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

/// The amount a captured message reported, however its rule spelled it.
fn message_amount(m: &IngestedMessage) -> Option<f64> {
    let v = m.parsed_fields.get("amount")?;
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.replace(',', "").parse().ok()))
}

/// How many gaps a source's cadence is judged from.
///
/// ★★ Enough to have a distribution, few enough that the file does not grow
/// forever and that a source which changed its habit last month is judged on
/// what it does now rather than on what it used to.
const MAX_TRACKED_GAPS: usize = 50;

/// The percentile a learned staleness threshold is taken at.
///
/// ★★ High, because the cost of the two errors is not symmetric: a false
/// "stale" sends somebody to check a phone that is fine, and they stop
/// believing the next one. A missed staleness is caught by the next message
/// that does not arrive.
const STALENESS_PERCENTILE: f64 = 0.95;

/// The offset the senders this node reads are in.
///
/// ★★★ Declared, not detected. Safaricom and KCB both stamp East Africa Time,
/// and a message that says half past four does not say half past four *where* —
/// so somebody has to state it. Here rather than in the core, because it is a
/// fact about this node's own senders.
const SENDER_UTC_OFFSET_MINUTES: i64 = 180;

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
            Self::BeforeStart { at, start } => f
                .debug_struct("BeforeStart")
                .field("at", at)
                .field("start", start)
                .finish(),
            Self::Duplicate(m) => f.debug_struct("Duplicate").field("id", &m.id).finish(),
            Self::Stored(m) => {
                f.debug_struct("Stored").field("id", &m.id).field("status", &m.status).finish()
            }
        }
    }
}

// ── reversal netting ─────────────────────────────────────────────────────────

/// Does this text describe a refund of an earlier charge?
///
/// ★★ Deliberately NOT a transducer tier. The five tiers are about what a
/// message IS; this is about what two messages are to EACH OTHER, which only
/// the queue can answer. Keeping it here leaves the tier partition alone.
pub fn looks_like_reversal(raw: &str) -> bool {
    let t = raw.to_lowercase();
    t.contains("has been reversed")
        || t.contains("was reversed")
        || t.contains("have been reversed")
        || t.contains("reversal of")
}

/// Merchant names, compared the way a person would.
fn same_counterparty(a: &str, b: &str) -> bool {
    let norm = |x: &str| x.to_uppercase().split_whitespace().collect::<Vec<_>>().join(" ");
    let (a, b) = (norm(a), norm(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // One often carries a suffix the other does not: "NAIVAS" and
    // "NAIVAS SUPERMARKET" are the same shop.
    a == b || a.contains(&b) || b.contains(&a)
}

/// Two amounts are the same money.
fn same_amount(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

/// The calls that would undo a filed charge, in the order to make them.
///
/// ★★★ Derived from what the original filing actually DID, never assumed. A
/// charge filed into a pocket that already had room is one spend, and undoing
/// it is one `budget.unspend`. A charge filed the backfill way moved money out
/// of liquid first, and undoing that is the unspend AND an unallocate. Guessing
/// either shape would move real money the wrong way.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Compensation {
    pub operator: String,
    pub params: BTreeMap<String, serde_json::Value>,
}

/// Work out how to undo a filed charge, or say why it cannot be worked out.
///
/// ★★ The inverses run in REVERSE order. Unspending first frees the pocket's
/// room, which is exactly what `budget.unallocate` then needs in order to give
/// the money back to liquid -- the other order refuses itself.
pub fn compensation_for(filed: &[Filing]) -> Vec<Compensation> {
    let mut out = Vec::new();
    for f in filed.iter().rev() {
        let inverse = match f.operator.as_str() {
            "budget.spend" => "budget.unspend",
            "budget.allocate" => "budget.unallocate",
            // ★ Anything else is left alone rather than guessed at. A pocket
            //   that was merely CREATED has nothing to undo, and an unknown
            //   operator has no inverse this code can claim to know.
            _ => continue,
        };
        let mut params = BTreeMap::new();
        if let Some(v) = f.params.get("pocket_name") {
            params.insert("pocket_name".to_string(), v.clone());
        }
        if let Some(v) = f.params.get("amount") {
            params.insert("amount".to_string(), v.clone());
        }
        if params.len() == 2 {
            out.push(Compensation { operator: inverse.to_string(), params });
        }
    }
    out
}

/// One charge cancelled by one refund.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NettedPair {
    pub reversal: String,
    pub original: String,
    pub amount: f64,
    pub counterparty: String,
}

/// A refund of a charge that was already filed, and how to give the money back.
///
/// ★★★ Not applied here. This pass reads; the caller runs the calls through the
/// real gate, because moving money is the gate's business and a store has no
/// right to it. The pairing is recorded either way, so a refusal cannot leave
/// the same refund matched twice.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingCompensation {
    pub reversal: String,
    pub original: String,
    pub amount: f64,
    pub counterparty: String,
    pub calls: Vec<Compensation>,
}

/// What a netting pass did, and what it deliberately would not do.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NettingReport {
    pub netted: Vec<NettedPair>,
    /// ★★★ Case 2: the charge was already filed, so the money has to come back
    /// rather than the pair simply vanishing.
    pub compensations: Vec<PendingCompensation>,
    /// Reversals whose original was filed in a way this cannot undo -- filed
    /// before filings were recorded, or by an operator with no known inverse.
    /// Reported rather than guessed at.
    pub uncompensable: u32,
    /// ★★ Skipped messages a reference brought back into play.
    pub reclaimed: u32,
    /// ★★★ Case 3: no charge anywhere to match. The money came back and
    /// nobody can say from where, so it stays in the queue as a question
    /// rather than being quietly absorbed.
    pub unmatched: u32,
    /// ★★★ Reversals with MORE THAN ONE candidate charge.
    ///
    /// Left alone on purpose. Netting removes two messages and records
    /// nothing, so guessing the wrong pair makes two real transactions
    /// disappear. Where it cannot tell, it does not choose.
    pub ambiguous: u32,
}

impl Ingested {
    /// Cancel refunds against their charges, where both are still unclassified.
    ///
    /// ★★★ Case 1 of the netting design. A charge and its refund net to zero,
    /// so if neither has been filed yet the honest outcome is that both leave
    /// the queue and nothing is recorded. No money moved on balance, and no
    /// event should claim it did.
    ///
    /// Nothing is deleted. Each keeps its text and gains the id of the other,
    /// which is what makes this an entry in the log rather than a silent drop.
    ///
    /// ★★ Matching is amount plus merchant. Time is deliberately NOT required:
    /// messages captured before this existed carry no timestamp, and demanding
    /// one would make the whole backlog unmatchable. The uniqueness rule is
    /// what keeps that safe.
    pub fn net_reversals(&self, sustain_id: &str) -> StoreResult<NettingReport> {
        let all: Vec<IngestedMessage> =
            self.current()?.into_iter().filter(|m| m.sustain_id == sustain_id).collect();

        // Reversals still waiting on an answer. One already paired or set aside
        // has had its answer.
        let reversals: Vec<&IngestedMessage> = all
            .iter()
            .filter(|m| looks_like_reversal(&m.raw_payload))
            .filter(|m| {
                m.needs_attention() || (Self::was_skipped(m) && m.reference().is_some())
            })
            .collect();

        // ★★ Two pools, searched in this order. An unfiled charge cancels for
        //    free and records nothing, so it is always the better match when
        //    both are available.
        //
        // ★★★ A charge he SKIPPED joins the first pool, but only if it quotes
        //     a reference — and the reference test below is the only one that
        //     may reach it. A refund whose charge he waved past is still a
        //     refund; a promo he waved past is still a promo, and nothing in
        //     here can drag it back.
        let unfiled: Vec<&IngestedMessage> = all
            .iter()
            .filter(|m| !looks_like_reversal(&m.raw_payload))
            .filter(|m| {
                m.needs_attention() || (Self::was_skipped(m) && m.reference().is_some())
            })
            .collect();
        let filed: Vec<&IngestedMessage> = all
            .iter()
            .filter(|m| m.netted_with.is_none() && m.resolved && !looks_like_reversal(&m.raw_payload))
            .collect();

        let mut report = NettingReport::default();
        let mut taken: BTreeSet<String> = BTreeSet::new();

        for r in reversals {
            let (Some(amount), Some(who)) = (r.amount(), r.counterparty()) else {
                report.unmatched += 1;
                continue;
            };
            // ★★★ The reference, where the refund quotes one. A reversal
            //     names the charge it undoes, and that is an identity rather
            //     than a resemblance — so where both texts carry it, two
            //     identical charges to the same shop stop being ambiguous and
            //     become a question with one right answer.
            //
            // ★★ Amount and merchant remain the fallback, unchanged, because
            //    plenty of real texts quote no reference at all and the whole
            //    captured backlog predates anyone looking for one.
            let matches = |pool: &[&IngestedMessage]| -> Vec<String> {
                let open: Vec<&&IngestedMessage> =
                    pool.iter().filter(|c| !taken.contains(&c.id)).collect();
                if let Some(r) = r.reference() {
                    let exact: Vec<String> = open
                        .iter()
                        .filter(|c| c.reference() == Some(r))
                        .map(|c| c.id.clone())
                        .collect();
                    if !exact.is_empty() {
                        return exact;
                    }
                }
                // ★★★ Resemblance may only be asked of messages still in the
                //     queue. Overturning his decision on a likeness rather
                //     than an identity is exactly the mistake this guards.
                open.iter()
                    .filter(|c| !Self::was_skipped(c))
                    .filter(|c| c.amount().map(|a| same_amount(a, amount)).unwrap_or(false))
                    .filter(|c| c.counterparty().map(|x| same_counterparty(x, who)).unwrap_or(false))
                    .map(|c| c.id.clone())
                    .collect()
            };

            // ── case 1 ───────────────────────────────────────────────────────
            let free = matches(&unfiled);
            if free.len() > 1 {
                report.ambiguous += 1;
                continue;
            }
            if let Some(id) = free.first() {
                taken.insert(id.clone());
                self.mark_netted(&r.id, id)?;
                self.mark_netted(id, &r.id)?;
                report.netted.push(NettedPair {
                    reversal: r.id.clone(),
                    original: id.clone(),
                    amount,
                    counterparty: who.to_string(),
                });
                continue;
            }

            // ── case 2 ───────────────────────────────────────────────────────
            let done = matches(&filed);
            if done.len() > 1 {
                report.ambiguous += 1;
                continue;
            }
            if let Some(id) = done.first() {
                let original =
                    filed.iter().find(|m| &m.id == id).expect("it came out of this pool");
                let calls = compensation_for(&original.filed);
                if calls.is_empty() {
                    // ★★★ Filed in a way this cannot undo. Left in the queue
                    //     with nothing claimed, because a refund that quietly
                    //     did nothing is worse than one still asking.
                    report.uncompensable += 1;
                    continue;
                }
                taken.insert(id.clone());
                report.compensations.push(PendingCompensation {
                    reversal: r.id.clone(),
                    original: id.clone(),
                    amount,
                    counterparty: who.to_string(),
                    calls,
                });
                continue;
            }

            // ── case 3 ───────────────────────────────────────────────────────
            report.unmatched += 1;
        }
        Ok(report)
    }

    /// Record that a refund and the charge it gave back are settled.
    ///
    /// ★★ Called only after the compensating calls actually committed. Marking
    /// the pair before the gate has spoken would leave a refund looking handled
    /// while the money never moved.
    pub fn mark_compensated(&self, reversal: &str, original: &str) -> StoreResult<()> {
        self.mark_netted(reversal, original)?;
        self.mark_netted(original, reversal)
    }

    fn mark_netted(&self, id: &str, partner: &str) -> StoreResult<()> {
        let Some(mut m) = self.current()?.into_iter().find(|m| m.id == id) else {
            return Ok(());
        };
        // ★★ A pair only ever forms on a reference once a skip is involved,
        //    so settling one is also the moment the skip is overturned. The
        //    record keeps both facts: it was set aside, and then it came back.
        if m.ignored {
            m.ignored = false;
            m.reclaimed = true;
        }
        m.netted_with = Some(partner.to_string());
        self.append_message(&m)
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

#[cfg(test)]
mod netting_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-net-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    /// A real KCB card purchase, and the reversal that undoes it.
    fn charge(amount: &str, who: &str) -> String {
        format!(
            "KES {amount} transaction made on KCB card 1234XXXXXXXX5678 at {who} \
             on 1/8/26 12:25pm, Avail balance KES 59,055.00"
        )
    }
    fn reversal(amount: &str, who: &str) -> String {
        format!(
            "Your card transaction of KES {amount} at {who} on 1/8/26 has been reversed. \
             Avail balance KES 59,155.00"
        )
    }

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let ing = Ingested::at(scratch(name)).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    fn waiting(ing: &Ingested) -> usize {
        ing.current().expect("current").iter().filter(|m| m.needs_attention()).count()
    }

    /// ★★★ Case 1. A charge and its refund both unclassified cancel, both
    /// leave the queue, and nothing is recorded.
    #[test]
    fn a_charge_and_its_refund_cancel_each_other() {
        let (ing, rules) = store("pair");
        ing.capture("h", "kcb", &charge("100.00", "Bolt KE"), &rules).expect("charge");
        ing.capture("h", "kcb", &reversal("100.00", "Bolt KE"), &rules).expect("reversal");
        assert_eq!(waiting(&ing), 2, "both start in the queue");

        let r = ing.net_reversals("h").expect("net");
        assert_eq!(r.netted.len(), 1, "one pair");
        assert_eq!(r.ambiguous, 0);
        assert_eq!(waiting(&ing), 0, "and the queue is clear");
    }

    /// Nothing is deleted, and each names the other.
    #[test]
    fn netting_records_the_pairing_rather_than_deleting() {
        let (ing, rules) = store("record");
        ing.capture("h", "kcb", &charge("250.00", "NAIVAS"), &rules).expect("charge");
        ing.capture("h", "kcb", &reversal("250.00", "NAIVAS"), &rules).expect("reversal");
        ing.net_reversals("h").expect("net");

        let all = ing.current().expect("current");
        assert_eq!(all.len(), 2, "both still on record");
        for m in &all {
            assert!(m.netted_with.is_some(), "each says what it cancelled against");
            assert!(!m.raw_payload.is_empty(), "and keeps its text");
        }
        let ids: Vec<&str> = all.iter().map(|m| m.id.as_str()).collect();
        for m in &all {
            let partner = m.netted_with.as_deref().expect("partner");
            assert!(ids.contains(&partner), "and points at the other one");
            assert_ne!(partner, m.id, "never at itself");
        }
    }

    /// ★★★ THE SAFETY PROPERTY. Two identical charges and one refund is
    /// ambiguous, and netting the wrong one would make a real transaction
    /// disappear. Where it cannot tell, it does not choose.
    #[test]
    fn an_ambiguous_match_is_left_alone() {
        let (ing, rules) = store("ambiguous");
        ing.capture("h", "kcb", &charge("100.00", "Bolt KE"), &rules).expect("a");
        // Same amount, same merchant, different text so it is not a duplicate.
        ing.capture("h", "kcb", &format!("{} ", charge("100.00", "Bolt KE")), &rules)
            .expect("b");
        ing.capture("h", "kcb", &reversal("100.00", "Bolt KE"), &rules).expect("reversal");

        let r = ing.net_reversals("h").expect("net");
        assert_eq!(r.netted.len(), 0, "nothing netted");
        assert_eq!(r.ambiguous, 1, "and it says why");
        assert_eq!(waiting(&ing), 3, "all three still waiting for a person");
    }

    /// A refund with no charge to cancel is left for cases 2 and 3.
    #[test]
    fn a_refund_with_no_charge_is_left_alone() {
        let (ing, rules) = store("orphan");
        ing.capture("h", "kcb", &reversal("70.00", "SOMEWHERE"), &rules).expect("reversal");
        let r = ing.net_reversals("h").expect("net");
        assert_eq!(r.netted.len(), 0);
        assert_eq!(r.unmatched, 1);
        assert_eq!(waiting(&ing), 1, "it still asks");
    }

    /// A different amount is a different transaction.
    #[test]
    fn amounts_must_match() {
        let (ing, rules) = store("amounts");
        ing.capture("h", "kcb", &charge("100.00", "Bolt KE"), &rules).expect("charge");
        ing.capture("h", "kcb", &reversal("90.00", "Bolt KE"), &rules).expect("reversal");
        let r = ing.net_reversals("h").expect("net");
        assert_eq!(r.netted.len(), 0);
        assert_eq!(waiting(&ing), 2);
    }

    /// A different merchant is a different transaction.
    #[test]
    fn merchants_must_match() {
        let (ing, rules) = store("merchants");
        ing.capture("h", "kcb", &charge("100.00", "Bolt KE"), &rules).expect("charge");
        ing.capture("h", "kcb", &reversal("100.00", "NAIVAS"), &rules).expect("reversal");
        let r = ing.net_reversals("h").expect("net");
        assert_eq!(r.netted.len(), 0);
        assert_eq!(waiting(&ing), 2);
    }

    /// Running it twice nets nothing further and disturbs nothing.
    #[test]
    fn a_second_pass_is_a_no_op() {
        let (ing, rules) = store("twice");
        ing.capture("h", "kcb", &charge("100.00", "Bolt KE"), &rules).expect("charge");
        ing.capture("h", "kcb", &reversal("100.00", "Bolt KE"), &rules).expect("reversal");
        assert_eq!(ing.net_reversals("h").expect("first").netted.len(), 1);
        let second = ing.net_reversals("h").expect("second");
        assert_eq!(second.netted.len(), 0, "nothing left to net");
        assert_eq!(second.unmatched, 0, "and the netted one is not counted again");
        assert_eq!(waiting(&ing), 0);
    }

    // One household's reversal never cancels another's charge.
    // ── case 2: the charge was already filed ─────────────────────────────────

    /// File a charge the way the card does: allocate to make room, then spend.
    fn file_backfilled(ing: &Ingested, id: &str, pocket: &str, amount: f64) {
        let p = |a: f64| -> BTreeMap<String, serde_json::Value> {
            BTreeMap::from([
                ("pocket_name".to_string(), serde_json::json!(pocket)),
                ("amount".to_string(), serde_json::json!(a)),
            ])
        };
        ing.record_filing(id, "budget.allocate", &p(amount)).expect("allocate leg");
        ing.record_filing(id, "budget.spend", &p(amount)).expect("spend leg");
        ing.record_outcome(id, true, None).expect("resolve");
    }

    #[test]
    fn a_refund_of_a_filed_charge_asks_for_the_money_back() {
        let (ing, rules) = store("case2");
        let Capture::Stored(c) = ing.capture("h", "kcb", &charge("500.00", "Java House"), &rules)
            .expect("charge") else { panic!("stored") };
        file_backfilled(&ing, &c.id, "food", 500.0);
        ing.capture("h", "kcb", &reversal("500.00", "Java House"), &rules).expect("reversal");

        let r = ing.net_reversals("h").expect("net");
        assert!(r.netted.is_empty(), "nothing to cancel: the charge is already on the books");
        assert_eq!(r.compensations.len(), 1, "one refund to give back");

        let comp = &r.compensations[0];
        assert_eq!(comp.original, c.id);
        assert_eq!(comp.amount, 500.0);
        // ★★★ Both legs, in reverse. Unspend frees the room that unallocate
        //     then needs, and the other order refuses itself.
        let ops: Vec<&str> = comp.calls.iter().map(|c| c.operator.as_str()).collect();
        assert_eq!(ops, vec!["budget.unspend", "budget.unallocate"]);
        for call in &comp.calls {
            assert_eq!(call.params["pocket_name"], serde_json::json!("food"));
            assert_eq!(call.params["amount"], serde_json::json!(500.0));
        }
    }

    #[test]
    fn a_charge_filed_into_a_funded_pocket_only_gives_back_the_spend() {
        // ★★★ The money never came out of liquid for this one, so putting it
        //     back into liquid would invent money. Only the spend is undone.
        let (ing, rules) = store("case2-funded");
        let Capture::Stored(c) = ing.capture("h", "kcb", &charge("80.00", "Naivas"), &rules)
            .expect("charge") else { panic!("stored") };
        let p = BTreeMap::from([
            ("pocket_name".to_string(), serde_json::json!("shopping")),
            ("amount".to_string(), serde_json::json!(80.0)),
        ]);
        ing.record_filing(&c.id, "budget.spend", &p).expect("spend");
        ing.record_outcome(&c.id, true, None).expect("resolve");
        ing.capture("h", "kcb", &reversal("80.00", "Naivas"), &rules).expect("reversal");

        let r = ing.net_reversals("h").expect("net");
        let ops: Vec<&str> =
            r.compensations[0].calls.iter().map(|c| c.operator.as_str()).collect();
        assert_eq!(ops, vec!["budget.unspend"], "no allocation happened, so none is undone");
    }

    #[test]
    fn netting_does_not_settle_the_pair_until_the_money_actually_moved() {
        // ★★★ The store proposes; only the caller, having run the calls through
        //     the gate, may say it is done. Otherwise a refused compensation
        //     would leave a refund looking handled with nothing given back.
        let (ing, rules) = store("case2-unsettled");
        let Capture::Stored(c) = ing.capture("h", "kcb", &charge("200.00", "Uber"), &rules)
            .expect("charge") else { panic!("stored") };
        file_backfilled(&ing, &c.id, "transport", 200.0);
        let Capture::Stored(rev) = ing.capture("h", "kcb", &reversal("200.00", "Uber"), &rules)
            .expect("reversal") else { panic!("stored") };

        ing.net_reversals("h").expect("net");
        let after = ing.current().expect("current");
        let r = after.iter().find(|m| m.id == rev.id).expect("the reversal");
        assert!(r.netted_with.is_none(), "still unsettled until the gate has spoken");
        assert!(r.needs_attention(), "so it comes back next pass");

        // And once it has, both sides record the pairing.
        ing.mark_compensated(&rev.id, &c.id).expect("settle");
        let after = ing.current().expect("current");
        let r = after.iter().find(|m| m.id == rev.id).expect("the reversal");
        assert_eq!(r.netted_with.as_deref(), Some(c.id.as_str()));
        assert!(!r.needs_attention(), "and it stops asking");
    }

    #[test]
    fn a_settled_refund_is_not_offered_again() {
        let (ing, rules) = store("case2-once");
        let Capture::Stored(c) = ing.capture("h", "kcb", &charge("310.00", "Carrefour"), &rules)
            .expect("charge") else { panic!("stored") };
        file_backfilled(&ing, &c.id, "food", 310.0);
        let Capture::Stored(rev) = ing.capture("h", "kcb", &reversal("310.00", "Carrefour"), &rules)
            .expect("reversal") else { panic!("stored") };

        assert_eq!(ing.net_reversals("h").expect("first").compensations.len(), 1);
        ing.mark_compensated(&rev.id, &c.id).expect("settle");
        let second = ing.net_reversals("h").expect("second");
        assert!(second.compensations.is_empty(), "a second pass gives nothing back twice");
    }

    #[test]
    fn a_charge_filed_before_filings_were_recorded_is_reported_not_guessed() {
        // ★★★ The real migration case. Anything he classified before tonight
        //     has `resolved: true` and an empty filing list, so there is no
        //     honest way to know which pocket to give the money back to.
        //     Reported as such, and left in the queue.
        let (ing, rules) = store("case2-legacy");
        let Capture::Stored(c) = ing.capture("h", "kcb", &charge("90.00", "Shell"), &rules)
            .expect("charge") else { panic!("stored") };
        ing.record_outcome(&c.id, true, None).expect("resolve with no filing recorded");
        ing.capture("h", "kcb", &reversal("90.00", "Shell"), &rules).expect("reversal");

        let r = ing.net_reversals("h").expect("net");
        assert!(r.compensations.is_empty(), "nothing is invented");
        assert_eq!(r.uncompensable, 1, "and it says so out loud");
        assert_eq!(waiting(&ing), 1, "the refund is still a question for a person");
    }

    #[test]
    fn an_unfiled_charge_is_preferred_over_a_filed_one() {
        // ★★★ Cancelling costs nothing and moves no money, so where both are
        //     available it is the safer answer.
        let (ing, rules) = store("case2-prefer");
        let Capture::Stored(filed) = ing.capture("h", "kcb", &charge("60.00", "Java"), &rules)
            .expect("filed charge") else { panic!("stored") };
        file_backfilled(&ing, &filed.id, "food", 60.0);
        // A second, identical charge that was never classified.
        let raw2 = format!("{} ref 2", charge("60.00", "Java"));
        ing.capture("h", "kcb", &raw2, &rules).expect("unfiled charge");
        ing.capture("h", "kcb", &reversal("60.00", "Java"), &rules).expect("reversal");

        let r = ing.net_reversals("h").expect("net");
        assert_eq!(r.netted.len(), 1, "it cancelled against the unfiled one");
        assert!(r.compensations.is_empty(), "and moved no money");
    }

    #[test]
    fn two_filed_candidates_are_left_alone() {
        // The same safety rule as case 1: where it cannot tell, it does not choose.
        let (ing, rules) = store("case2-ambiguous");
        for tag in ["a", "b"] {
            let raw = format!("{} ref {tag}", charge("45.00", "Bolt"));
            let Capture::Stored(c) = ing.capture("h", "kcb", &raw, &rules).expect("charge")
                else { panic!("stored") };
            file_backfilled(&ing, &c.id, "transport", 45.0);
        }
        ing.capture("h", "kcb", &reversal("45.00", "Bolt"), &rules).expect("reversal");

        let r = ing.net_reversals("h").expect("net");
        assert!(r.compensations.is_empty(), "two candidates, so it gives nothing back");
        assert_eq!(r.ambiguous, 1);
    }

    // ── case 3: nothing to match ─────────────────────────────────────────────

    #[test]
    fn an_orphan_refund_stays_a_question_rather_than_being_absorbed() {
        let (ing, rules) = store("case3");
        ing.capture("h", "kcb", &reversal("777.00", "Nowhere Ltd"), &rules).expect("reversal");

        let r = ing.net_reversals("h").expect("net");
        assert!(r.netted.is_empty());
        assert!(r.compensations.is_empty());
        assert_eq!(r.unmatched, 1, "counted honestly as unmatched");
        assert_eq!(waiting(&ing), 1, "and still in the queue for a person to place");
    }

    // ── the plan itself ──────────────────────────────────────────────────────

    #[test]
    fn compensation_ignores_filings_it_has_no_inverse_for() {
        let p = BTreeMap::from([
            ("pocket_name".to_string(), serde_json::json!("food")),
            ("amount".to_string(), serde_json::json!(10.0)),
        ]);
        let filed = vec![
            Filing { operator: "budget.add_pocket".into(), params: p.clone() },
            Filing { operator: "budget.spend".into(), params: p.clone() },
        ];
        let calls = compensation_for(&filed);
        let ops: Vec<&str> = calls.iter().map(|c| c.operator.as_str()).collect();
        assert_eq!(ops, vec!["budget.unspend"], "creating a pocket has nothing to undo");
    }

    #[test]
    fn a_filing_missing_its_numbers_is_skipped() {
        let filed = vec![Filing {
            operator: "budget.spend".into(),
            params: BTreeMap::from([("pocket_name".to_string(), serde_json::json!("food"))]),
        }];
        assert!(compensation_for(&filed).is_empty(), "no amount, so no claim about one");
    }

    #[test]
    fn netting_does_not_cross_sustains() {
        let (ing, rules) = store("cross");
        ing.capture("h", "kcb", &charge("100.00", "Bolt KE"), &rules).expect("charge");
        ing.capture("other", "kcb", &reversal("100.00", "Bolt KE"), &rules).expect("reversal");
        let r = ing.net_reversals("h").expect("net");
        assert_eq!(r.netted.len(), 0, "different households do not net");
    }
}

#[cfg(test)]
mod attribution_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-attrib-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    /// ★★★ The whole reason attribution is free. The text already knows which
    /// account it is about, so nobody has to be asked which one it was.
    #[test]
    fn a_captured_message_knows_which_account_it_moved_in() {
        let ing = Ingested::at(scratch("source")).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        let Capture::Stored(m) = ing
            .capture("h", "kcb", "KES 500.00 transaction made on KCB card 1234XXXXXXXX5678 \
                                  at Java on 1/8/26 12:25pm, Avail balance KES 9,000.00", &rules)
            .expect("capture")
        else {
            panic!("expected it to be stored");
        };
        assert_eq!(ing.source_of_message(&m.id).as_deref(), Some("kcb"));
    }

    #[test]
    fn a_message_that_does_not_exist_attributes_to_nothing() {
        // ★ None rather than a default. A guess here would put real money in
        //   the wrong account.
        let ing = Ingested::at(scratch("missing")).expect("ingest");
        assert_eq!(ing.source_of_message("nope"), None);
    }
}

#[cfg(test)]
mod reconciliation_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-recon-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let ing = Ingested::at(scratch(name)).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    /// A real M-Pesa text, which like all of them ends by stating the balance.
    fn paid(ref_: &str, amount: &str, balance: &str) -> String {
        format!(
            "{ref_} Confirmed. Ksh{amount} paid to NAIVAS SUPERMARKET on 20/7/26 \
             at 4:30 PM. New M-PESA balance is Ksh{balance}"
        )
    }

    #[test]
    fn the_balance_a_text_reports_is_read_off_it() {
        let (ing, rules) = store("reads");
        ing.capture_at("h", "mpesa", &paid("QGH7XJ4P2Q", "450.00", "12,050.00"), &rules, Some(1_000))
            .expect("capture");
        let got = ing.reported_balances("h").expect("balances");
        assert_eq!(got["mpesa"].balance, 12050.0, "commas and all");
    }

    #[test]
    fn the_freshest_word_is_the_newest_message_not_the_last_one_read() {
        // ★★★ The failure this ordering exists to prevent. A backlog is read
        //     newest-first, so capture order runs BACKWARDS through time. Here
        //     the older text is captured second, exactly as a real read does
        //     it, and the newer balance must still win.
        let (ing, rules) = store("ordering");
        ing.capture_at("h", "mpesa", &paid("AAAAAAAAAA", "100.00", "9,000.00"), &rules,
                       Some(2_000_000))
            .expect("newer, read first");
        ing.capture_at("h", "mpesa", &paid("BBBBBBBBBB", "200.00", "500.00"), &rules,
                       Some(1_000_000))
            .expect("older, read second");

        let got = ing.reported_balances("h").expect("balances");
        assert_eq!(got["mpesa"].balance, 9000.0, "the newer text is the freshest word");
        assert_eq!(got["mpesa"].at, 2_000_000);
    }

    #[test]
    fn a_message_with_no_arrival_time_is_skipped_rather_than_assumed_recent() {
        // ★★ Anything captured before arrival times were carried, and anything
        //    pasted by hand. Guessing it were recent would let it overrule a
        //    genuinely newer text about the same account.
        let (ing, rules) = store("undated");
        ing.capture_at("h", "mpesa", &paid("CCCCCCCCCC", "100.00", "7,777.00"), &rules, None)
            .expect("undated");
        assert!(!ing.reported_balances("h").expect("balances").contains_key("mpesa"));
    }

    #[test]
    fn each_account_is_reconciled_against_its_own_texts() {
        let (ing, rules) = store("per-account");
        ing.capture_at("h", "mpesa", &paid("DDDDDDDDDD", "100.00", "1,111.00"), &rules, Some(10))
            .expect("mpesa");
        ing.capture_at(
            "h",
            "kcb",
            "KES 500.00 transaction made on KCB card 1234XXXXXXXX5678 at Java \
             on 1/8/26 12:25pm, Avail balance KES 2,222.00",
            &rules,
            Some(20),
        )
        .expect("kcb");

        let got = ing.reported_balances("h").expect("balances");
        assert_eq!(got["mpesa"].balance, 1111.0);
        assert_eq!(got["kcb"].balance, 2222.0, "a bank's word is only about its own account");
    }

    #[test]
    fn reconciliation_does_not_cross_households() {
        let (ing, rules) = store("scoped");
        ing.capture_at("h", "mpesa", &paid("EEEEEEEEEE", "100.00", "1,111.00"), &rules, Some(10))
            .expect("one");
        assert!(ing.reported_balances("other").expect("balances").is_empty());
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-xfer-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    const OWN_PHONE: &str = "254712345678";
    const OWN_ACCT: &str = "112233";

    fn waiting(ing: &Ingested) -> usize {
        ing.current().expect("current").iter().filter(|m| m.needs_attention()).count()
    }

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let ing = Ingested::at(scratch(name)).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    fn knows_his_own(ing: &Ingested) {
        ing.set_own_identifiers(&OwnIdentifiers {
            mpesa: vec![OWN_PHONE.into()],
            kcb: vec![OWN_ACCT.into()],
        })
        .expect("own");
    }

    /// M-Pesa's own words for money leaving, addressed to a phone.
    fn sent_to(phone: &str, amount: &str) -> String {
        format!(
            "QGH7XJ4P2Q Confirmed. Ksh{amount} sent to MARY WANJIRU {phone} on 2/8/26 \
             at 9:00 AM. New M-PESA balance is Ksh12,050.00"
        )
    }

    /// M-Pesa's own words for money leaving toward a bank account.
    fn paid_to_account(account: &str, amount: &str) -> String {
        format!(
            "QGH7XJ4P2Q Confirmed. Ksh{amount} sent to KCB BANK for account {account} \
             on 2/8/26 at 9:00 AM. New M-PESA balance is Ksh12,050.00"
        )
    }


    /// KCB's own words for that same money arriving. Its real shipped shape.
    fn kcb_account_credited(account: &str, amount: &str) -> String {
        format!(
            "Ksh{amount} sent to KCB account JOHN KAMAU {account} has been received \
             on 01/08/2026. M-PESA Ref UH1B91GYNX"
        )
    }



    // ── the real one, from his phone ─────────────────────────────────────────
    //
    // His KCB app sends this when he moves his own money to his own M-Pesa.
    // It names both ends, which is what lets it be recognised on its own
    // rather than waiting for a partner text that may never be read.
    // The digits are masked here exactly as his bank masks them.
    const HIS_KCB: &str = "135***140";
    const HIS_MPESA_MASKED: &str = "254***143";

    fn his_self_transfer(amount: &str) -> String {
        format!(
            "SEND TO M-PESA request of KES {amount} from {HIS_KCB} to {HIS_MPESA_MASKED} - \
             BONVENTURE NGUGI MAINA has been received for processing."
        )
    }

    /// The full numbers behind those masks, as he would type them in.
    fn knows_his_real_numbers(ing: &Ingested) {
        ing.set_own_identifiers(&OwnIdentifiers {
            mpesa: vec!["254700000143".into()],
            kcb: vec!["1350000140".into()],
        })
        .expect("own");
    }

    #[test]
    fn his_stuck_transaction_is_recognised_as_a_move_between_his_own_accounts() {
        // ★★★ The exact message he was stuck on, which was being asked "which
        //     pocket?" -- a question with no right answer, because no pocket
        //     was involved.
        let (ing, rules) = store("his-real");
        knows_his_real_numbers(&ing);
        ing.capture("h", "kcb", &his_self_transfer("2,000"), &rules).expect("capture");

        let r = ing.find_transfers("h").expect("transfers");
        assert_eq!(r.self_moves.len(), 1, "one move, recognised from one text");
        let mv = &r.self_moves[0];
        assert_eq!(mv.from_account, "kcb");
        assert_eq!(mv.to_account, "mpesa");
        assert_eq!(mv.amount, 2000.0);
        assert!(mv.undo_income.is_none(), "nothing had filed the other side yet");
    }

    #[test]
    fn the_same_transaction_is_an_ordinary_payment_before_he_says_the_numbers_are_his() {
        // ★★★ Nothing about the wording says whose numbers those are. Without
        //     his own, this is a payment to a stranger who shares his name, and
        //     treating it as a transfer would erase a real expense.
        let (ing, rules) = store("his-real-undeclared");
        ing.capture("h", "kcb", &his_self_transfer("2,000"), &rules).expect("capture");
        assert!(ing.find_transfers("h").expect("transfers").self_moves.is_empty());
    }

    #[test]
    fn money_leaving_his_account_for_someone_elses_stays_a_payment() {
        // ★★★ The failure that would cost him most: a real expense quietly
        //     reclassified as his own money moving, and gone from his spending.
        let (ing, rules) = store("outbound");
        knows_his_real_numbers(&ing);
        let to_someone_else = format!(
            "SEND TO M-PESA request of KES 2,000 from {HIS_KCB} to 254***999 - \
             JANE DOE has been received for processing."
        );
        ing.capture("h", "kcb", &to_someone_else, &rules).expect("capture");

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.self_moves.is_empty(), "the far end is not his, so it is a payment");
        assert_eq!(waiting(&ing), 1, "and it is still his to classify");
    }

    #[test]
    fn a_mask_must_show_enough_digits_to_mean_anything() {
        // ★★ A mask proves a prefix and a suffix and nothing else, so a short
        //    one is a coincidence rather than an identity.
        let own = OwnIdentifiers { mpesa: vec!["254700000143".into()], kcb: vec![] };
        assert!(own.claims("254***143"), "six visible digits is his bank's own masking");
        assert!(!own.claims("2***3"), "two digits could be anyone");
        assert!(!own.claims("254***999"), "the tail has to match");
        assert!(!own.claims("999***143"), "and so does the head");
    }

    #[test]
    fn a_settled_self_move_stops_asking_and_is_not_moved_twice() {
        let (ing, rules) = store("his-real-once");
        knows_his_real_numbers(&ing);
        ing.capture("h", "kcb", &his_self_transfer("2,000"), &rules).expect("capture");
        let mv = ing.find_transfers("h").expect("first").self_moves[0].clone();

        ing.mark_self_moved(&mv.message).expect("settle");
        assert_eq!(waiting(&ing), 0, "it stops asking which pocket");
        assert!(
            ing.find_transfers("h").expect("second").self_moves.is_empty(),
            "and a second pass does not move the money again"
        );
    }

    #[test]
    fn an_income_already_filed_for_the_far_side_is_named_for_undoing() {
        // ★★★ The double count this exists to prevent. M-Pesa's own "you have
        //     received" arrives and files itself as income before the KCB text
        //     reveals the money was his all along.
        let (ing, rules) = store("double");
        knows_his_real_numbers(&ing);
        let received = "QGH7XJ4P2Q Confirmed. You have received Ksh2,000.00 from KCB BANK \
                        254700000143 on 2/8/26 at 10:15 AM. New M-PESA balance is Ksh12,050.00";
        let Capture::Stored(inc) = ing.capture("h", "mpesa", received, &rules).expect("income")
        else {
            panic!("stored")
        };
        assert_eq!(inc.status, "mapped", "it files itself, which is the whole problem");
        // Mark it applied, as the engine does once the operator commits.
        ing.record_outcome(&inc.id, true, None).expect("applied");

        ing.capture("h", "kcb", &his_self_transfer("2,000"), &rules).expect("the other half");

        let r = ing.find_transfers("h").expect("transfers");
        assert_eq!(r.self_moves.len(), 1);
        assert_eq!(
            r.self_moves[0].undo_income.as_deref(),
            Some(inc.id.as_str()),
            "the income to take back is named, not left standing"
        );
    }

    #[test]
    fn nothing_is_a_transfer_until_he_says_which_numbers_are_his() {
        // ★★★ The safety property. Without his numbers there is no difference
        //     between paying a friend and moving his own money, and guessing
        //     would silently erase a real expense.
        let (ing, rules) = store("undeclared");
        ing.capture("h", "mpesa", &paid_to_account(OWN_ACCT, "2,000.00"), &rules).expect("out");
        ing.capture("h", "kcb", &kcb_account_credited(OWN_ACCT, "2,000.00"), &rules).expect("in");

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.matched.is_empty(), "no identifiers declared, so nothing is his own move");
        assert_eq!(r.unpaired, 0, "and nothing is even a candidate");
    }

    #[test]
    fn a_number_that_is_not_his_is_an_ordinary_payment() {
        // ★★★ The other half of the same property: paying someone else must
        //     stay an expense.
        let (ing, rules) = store("someone-else");
        knows_his_own(&ing);
        ing.capture("h", "mpesa", &paid_to_account("999999", "2,000.00"), &rules).expect("out");
        ing.capture("h", "kcb", &kcb_account_credited("999999", "2,000.00"), &rules).expect("in");

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.matched.is_empty(), "that is a payment to a person, not a move");
    }

    #[test]
    fn one_leg_alone_stays_a_question() {
        // ★★★ Recording half a transfer would take money out of one account
        //     and put it into one no text ever confirmed it reached.
        let (ing, rules) = store("half");
        knows_his_own(&ing);
        ing.capture("h", "mpesa", &sent_to(OWN_PHONE, "2,000.00"), &rules).expect("out");

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.matched.is_empty());
        assert_eq!(r.unpaired, 1, "counted, and left in the queue");
    }


    #[test]
    fn a_partner_already_filed_as_income_is_reported_not_silently_missed() {
        // ★★★ The real state of things, found by testing rather than assumed.
        //
        // Every KCB text saying money arrived is MAPPED, so it is applied as
        // income the instant it is captured -- long before the outgoing M-Pesa
        // text that would reveal it was his own money moving. By the time both
        // halves are in hand the income is already on the books, which is
        // precisely the overstated income this mechanism exists to prevent.
        //
        // ★★ These two texts quote DIFFERENT references, so they are joinable
        //    only on amount -- and amount alone may not undo something already
        //    on the books. An inverse exists now, but using it here would risk
        //    deleting a real income on a resemblance. So this stays reported
        //    rather than acted on, which is the safe direction: "found one and
        //    cannot act on it" is a different answer from "no transfers found".
        //    The shared-reference case IS acted on -- see the test below.
        let (ing, rules) = store("blocked");
        knows_his_own(&ing);
        ing.capture("h", "mpesa", &paid_to_account(OWN_ACCT, "2,000.00"), &rules).expect("out");
        let inn = ing
            .capture("h", "kcb", &kcb_account_credited(OWN_ACCT, "2,000.00"), &rules)
            .expect("in");
        // The inbound leg really did apply itself on the way in.
        if let Capture::Stored(m) = &inn {
            assert_eq!(m.status, "mapped", "KCB inbound is mapped, which is why this happens");
        }

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.matched.is_empty(), "it cannot be turned into a transfer yet");
        assert_eq!(r.blocked_by_applied_income, 1, "and it says so rather than staying quiet");
        assert_eq!(r.unpaired, 0, "this is not the same thing as having no partner");
    }

    #[test]
    fn a_settled_pair_stops_asking_and_is_never_paired_twice() {
        // ★★ Marking is separate from finding, and this is the marking half:
        //    once a pair is recorded as one move, neither leg is a question and
        //    neither can be picked up again.
        let (ing, rules) = store("once");
        knows_his_own(&ing);
        let out = ing
            .capture("h", "mpesa", &paid_to_account(OWN_ACCT, "2,000.00"), &rules)
            .expect("out");
        let inn = ing
            .capture("h", "kcb", &kcb_account_credited(OWN_ACCT, "2,000.00"), &rules)
            .expect("in");
        let (Capture::Stored(o), Capture::Stored(i)) = (&out, &inn) else { panic!("stored") };

        ing.mark_transferred(&o.id, &i.id).expect("settle");
        let after = ing.current().expect("current");
        assert!(after.iter().all(|m| !m.needs_attention()), "neither leg still asks");
        let second = ing.find_transfers("h").expect("second");
        assert!(second.matched.is_empty(), "and it is not moved twice");
        assert_eq!(second.blocked_by_applied_income, 0, "nor reported as blocked once settled");
    }


    #[test]
    fn a_phone_number_matches_however_it_was_written() {
        // ★★ The same phone is printed three ways depending on who prints it.
        let own = OwnIdentifiers { mpesa: vec!["0712345678".into()], kcb: vec![] };
        assert!(own.claims("254712345678"), "with a country code");
        assert!(own.claims("+254 712 345678"), "with spaces and a plus");
        assert!(own.claims("0712345678"), "as he typed it");
        assert!(!own.claims("254799999999"), "and a different number is different");
    }

    #[test]
    fn a_short_number_never_claims_a_long_one() {
        // ★★★ A four-digit card mask must not match a phone by its tail.
        let own = OwnIdentifiers { mpesa: vec!["5678".into()], kcb: vec![] };
        assert!(!own.claims("254712345678"), "four digits is a coincidence, not an identity");
    }

    #[test]
    fn identifiers_are_stored_as_digits_however_they_were_typed() {
        let (ing, _) = store("digits");
        ing.set_own_identifiers(&OwnIdentifiers {
            mpesa: vec!["+254 712 345 678".into()],
            kcb: vec!["  1234-5678  ".into()],
        })
        .expect("set");
        let back = ing.own_identifiers().expect("get");
        assert_eq!(back.mpesa, vec!["254712345678".to_string()]);
        assert_eq!(back.kcb, vec!["12345678".to_string()]);
    }

    #[test]
    fn a_household_with_no_identifiers_reads_as_empty_rather_than_failing() {
        let (ing, _) = store("fresh");
        assert!(ing.own_identifiers().expect("get").is_empty());
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-ref-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let ing = Ingested::at(scratch(name)).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    const REF: &str = "UHPB9480T9";
    const HIS_KCB: &str = "135***140";
    const HIS_MPESA: &str = "254***143";

    fn knows_his_numbers(ing: &Ingested) {
        ing.set_own_identifiers(&OwnIdentifiers {
            mpesa: vec!["254700000143".into()],
            kcb: vec!["1350000140".into()],
        })
        .expect("own");
    }

    /// His KCB app's text, quoting the M-Pesa reference.
    fn kcb_leg(amount: &str, reference: &str) -> String {
        format!(
            "SEND TO M-PESA request of KES {amount} from {HIS_KCB} to {HIS_MPESA} - \
             BONVENTURE NGUGI MAINA has been received for processing. M-PESA REF: {reference}"
        )
    }

    /// M-Pesa's own text about the same money, opening with that same code.
    fn mpesa_leg(amount: &str, reference: &str) -> String {
        format!(
            "{reference} Confirmed. You have received Ksh{amount} from KCB BANK \
             254700000143 on 2/8/26 at 10:15 AM. New M-PESA balance is Ksh12,050.00"
        )
    }

    #[test]
    fn both_halves_of_one_transfer_quote_the_same_reference() {
        // ★★★ The insight this whole join rests on, checked against the real
        //     shapes rather than assumed: the code KCB prints as "M-PESA REF"
        //     is the code M-Pesa's own confirmation opens with.
        let (ing, rules) = store("shared");
        let Capture::Stored(k) = ing.capture("h", "kcb", &kcb_leg("2,000", REF), &rules)
            .expect("kcb") else { panic!("stored") };
        let Capture::Stored(m) = ing.capture("h", "mpesa", &mpesa_leg("2,000.00", REF), &rules)
            .expect("mpesa") else { panic!("stored") };

        assert_eq!(k.reference(), Some(REF), "KCB quotes it");
        assert_eq!(m.reference(), Some(REF), "and M-Pesa opens with it");
    }

    #[test]
    fn the_income_to_undo_is_found_by_reference_not_by_amount() {
        // ★★★ Why the ref is better rather than merely different. Here the
        //     amounts are equal but so is a DECOY's, and only the reference
        //     can say which of them was this transfer's other half.
        let (ing, rules) = store("by-ref");
        knows_his_numbers(&ing);

        // A real, unrelated income of the same amount, applied first.
        let Capture::Stored(decoy) = ing
            .capture("h", "mpesa", &mpesa_leg("2,000.00", "QQQQQQQQQQ"), &rules)
            .expect("decoy") else { panic!("stored") };
        ing.record_outcome(&decoy.id, true, None).expect("applied");

        // The transfer's own far side, also applied.
        let Capture::Stored(real) = ing
            .capture("h", "mpesa", &mpesa_leg("2,000.00", REF), &rules)
            .expect("far side") else { panic!("stored") };
        ing.record_outcome(&real.id, true, None).expect("applied");

        ing.capture("h", "kcb", &kcb_leg("2,000", REF), &rules).expect("near side");

        let r = ing.find_transfers("h").expect("transfers");
        assert_eq!(r.self_moves.len(), 1);
        assert_eq!(
            r.self_moves[0].undo_income.as_deref(),
            Some(real.id.as_str()),
            "the reference picked the right one out of two identical amounts"
        );
    }

    #[test]
    fn a_text_with_no_reference_still_matches_on_what_it_has() {
        // ★★ The fallback is not dead code. Plenty of real texts quote no
        //    reference, and the whole captured backlog predates anyone looking
        //    for one.
        let (ing, rules) = store("fallback");
        knows_his_numbers(&ing);
        let no_ref = format!(
            "SEND TO M-PESA request of KES 2,000 from {HIS_KCB} to {HIS_MPESA} - \
             BONVENTURE NGUGI MAINA has been received for processing."
        );
        let Capture::Stored(k) = ing.capture("h", "kcb", &no_ref, &rules).expect("kcb")
            else { panic!("stored") };
        assert_eq!(k.reference(), None, "there is none to read");

        let Capture::Stored(inc) = ing
            .capture("h", "mpesa", &mpesa_leg("2,000.00", "ZZZZZZZZZZ"), &rules)
            .expect("income") else { panic!("stored") };
        ing.record_outcome(&inc.id, true, None).expect("applied");

        let r = ing.find_transfers("h").expect("transfers");
        assert_eq!(r.self_moves.len(), 1);
        assert_eq!(
            r.self_moves[0].undo_income.as_deref(),
            Some(inc.id.as_str()),
            "amount and account still answer it when no reference was quoted"
        );
    }

    #[test]
    fn a_reference_too_short_to_be_one_is_ignored() {
        // ★★★ A four-character field called `ref` is a field that happens to
        //     be called that, not a transaction's identity. Joining two
        //     unrelated movements of money is worse than joining neither.
        let mut m = IngestedMessage {
            id: "x".into(),
            sustain_id: "h".into(),
            source_id: "kcb".into(),
            raw_payload: "".into(),
            dedup_key: "k".into(),
            status: "parsed_unmapped".into(),
            parser_name: "p".into(),
            reason: "".into(),
            parsed_fields: Default::default(),
            external_ref: None,
            operator: None,
            params: Default::default(),
            applied: false,
            gate_reason: None,
            resolved: false,
            netted_with: None,
            same_event_as: None,
            reclaimed: false,
            deferred_at: None,
            filed: Vec::new(),
            ignored: false,
            sent_at_ms: None,
            event_at_ms: None,
            seq: 1,
        };
        m.parsed_fields.insert("ref".into(), serde_json::json!("AB12"));
        assert_eq!(m.reference(), None);
        m.parsed_fields.insert("ref".into(), serde_json::json!(REF));
        assert_eq!(m.reference(), Some(REF));
    }

    // ── the cross-source double count ────────────────────────────────────────

    #[test]
    fn one_transaction_reported_by_two_banks_is_applied_once() {
        // ★★★ The gap that stayed open for want of a reliable correlation key.
        //     The dedup key hashes source plus raw text, so two banks writing
        //     differently about one payment both get in, and both applying it
        //     counts the money twice. The reference is that key.
        let (ing, rules) = store("cross");
        let Capture::Stored(first) = ing
            .capture("h", "mpesa", &mpesa_leg("2,000.00", REF), &rules)
            .expect("first") else { panic!("stored") };
        assert_eq!(first.status, "mapped", "M-Pesa's own text applies itself");
        ing.record_outcome(&first.id, true, None).expect("applied");

        let already = ing
            .same_event_already_applied("h", "kcb", REF)
            .expect("lookup")
            .expect("the same event, from the other bank");
        assert_eq!(already, first.id);
    }

    #[test]
    fn the_same_bank_repeating_a_reference_is_its_own_business() {
        // ★★ Only ACROSS sources. One sender quoting an earlier transaction --
        //    a balance notice, say -- is not a second report of it, and the
        //    raw-text dedup already covers a literal repeat.
        let (ing, rules) = store("same-source");
        let Capture::Stored(first) = ing
            .capture("h", "mpesa", &mpesa_leg("2,000.00", REF), &rules)
            .expect("first") else { panic!("stored") };
        ing.record_outcome(&first.id, true, None).expect("applied");

        assert!(
            ing.same_event_already_applied("h", "mpesa", REF).expect("lookup").is_none(),
            "the same sender is not a second bank"
        );
    }

    #[test]
    fn an_unapplied_first_text_does_not_block_the_second() {
        // ★★★ The guard is against double APPLYING, not against two texts
        //     existing. If the first was never applied there is nothing to
        //     double, and blocking the second would lose the transaction.
        let (ing, rules) = store("unapplied");
        ing.capture("h", "kcb", &kcb_leg("2,000", REF), &rules).expect("kcb, unapplied");
        assert!(
            ing.same_event_already_applied("h", "mpesa", REF).expect("lookup").is_none(),
            "nothing has been recorded yet, so nothing would be recorded twice"
        );
    }

    #[test]
    fn a_second_report_stops_asking_but_keeps_its_text() {
        let (ing, rules) = store("noted");
        let Capture::Stored(first) = ing
            .capture("h", "mpesa", &mpesa_leg("2,000.00", REF), &rules)
            .expect("first") else { panic!("stored") };
        ing.record_outcome(&first.id, true, None).expect("applied");
        let Capture::Stored(second) = ing.capture("h", "kcb", &kcb_leg("2,000", REF), &rules)
            .expect("second") else { panic!("stored") };

        ing.note_same_event(&second.id, &first.id).expect("note");
        let after = ing.current().expect("current");
        let s = after.iter().find(|m| m.id == second.id).expect("still there");
        assert_eq!(s.same_event_as.as_deref(), Some(first.id.as_str()));
        assert!(!s.needs_attention(), "it is not a decision anyone has to make");
        assert!(!s.raw_payload.is_empty(), "and the text is kept");
    }
}

#[cfg(test)]
mod skip_rule_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-skip-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let ing = Ingested::at(scratch(name)).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    /// A card purchase — a real shape, and one that repeats.
    fn card(amount: &str, who: &str) -> String {
        format!(
            "KES {amount} transaction made on KCB card 1234XXXXXXXX5678 at {who} \
             on 1/8/26 12:25pm, Avail balance KES 59,055.00"
        )
    }

    fn waiting(ing: &Ingested) -> usize {
        ing.current().expect("current").iter().filter(|m| m.needs_attention()).count()
    }

    #[test]
    fn learning_a_skip_clears_the_ones_already_waiting() {
        // ★★★ The whole point. He answers this in the middle of a pile of the
        //     same shape, so a rule that only covered future messages would
        //     leave the pile it was meant to clear exactly as it was.
        let (ing, rules) = store("retro");
        let mut first = None;
        for who in ["Java", "Naivas", "Shell", "Uber"] {
            let c = ing.capture("h", "kcb", &card("100.00", who), &rules).expect("capture");
            if first.is_none() {
                if let Capture::Stored(m) = c {
                    first = Some(m.id);
                }
            }
        }
        assert_eq!(waiting(&ing), 4);

        let out = ing.learn_skip("h", &first.expect("one")).expect("learn");
        assert!(!out.unlearnable);
        assert_eq!(out.cleared, 4, "all four, including the one he was looking at");
        assert_eq!(waiting(&ing), 0);
    }

    #[test]
    fn a_message_with_no_shape_at_all_teaches_nothing() {
        // ★★★ The safety property, and what it now rests on.
        //
        //     It used to rest on `parser_name`, which is empty for anything
        //     unparsed — so silencing an unreadable message would have meant
        //     "skip everything I cannot read", and that pile is exactly the one
        //     that needs a person's eyes. Keying on the SHAPE removes the
        //     danger instead of accepting it: two unreadable texts that read
        //     differently have different skeletons, so silencing one says
        //     nothing about the other (see the test below).
        //
        //     What is still refused is a text with no skeleton to key on. There
        //     is nothing there to recognise a second message by, so a rule
        //     would be a guess, and it is not made.
        let (ing, rules) = store("unparsed");
        let Capture::Stored(m) = ing
            .capture("h", "kcb", "!!! ??? ...", &rules)
            .expect("capture") else { panic!("stored") };
        assert_eq!(m.status, "unparsed");

        let out = ing.learn_skip("h", &m.id).expect("learn");
        assert!(out.unlearnable, "it refuses rather than learning a rule it cannot key");
        assert_eq!(out.cleared, 0);
        assert_eq!(waiting(&ing), 1, "and it is still there for him to look at");
        assert!(ing.skip_rules().expect("rules").is_empty(), "nothing was written");
    }

    #[test]
    fn a_later_message_of_a_learned_shape_is_recognised() {
        let (ing, rules) = store("future");
        let Capture::Stored(m) = ing.capture("h", "kcb", &card("100.00", "Java"), &rules)
            .expect("capture") else { panic!("stored") };
        ing.learn_skip("h", &m.id).expect("learn");

        let Capture::Stored(later) = ing.capture("h", "kcb", &card("250.00", "Carrefour"), &rules)
            .expect("later") else { panic!("stored") };
        assert!(ing.is_skipped(&later).expect("check"), "a different amount, the same shape");
    }

    #[test]
    fn a_different_shape_is_untouched() {
        // ★★★ The failure that would cost him most: one skip quietly
        //     swallowing transactions he does care about.
        let (ing, rules) = store("narrow");
        let Capture::Stored(m) = ing.capture("h", "kcb", &card("100.00", "Java"), &rules)
            .expect("capture") else { panic!("stored") };
        ing.learn_skip("h", &m.id).expect("learn");

        let spend = "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 \
                     at 4:30 PM. New M-PESA balance is Ksh12,050.00";
        let Capture::Stored(other) = ing.capture("h", "mpesa", spend, &rules).expect("other")
            else { panic!("stored") };
        assert!(!ing.is_skipped(&other).expect("check"), "a real spend still asks");
        assert!(other.needs_attention());
    }

    #[test]
    fn the_same_shape_from_a_different_bank_is_a_different_rule() {
        // ★★ The key carries the source. "I never want KCB's card notices" is
        //    not "I never want anything shaped like that from anyone".
        let (ing, rules) = store("per-source");
        let Capture::Stored(m) = ing.capture("h", "kcb", &card("100.00", "Java"), &rules)
            .expect("capture") else { panic!("stored") };
        ing.learn_skip("h", &m.id).expect("learn");
        let key = Ingested::skip_key(&m).expect("a key");
        assert!(key.starts_with("kcb::"), "the source is part of it: {key}");
    }

    #[test]
    fn learning_does_not_reach_into_another_household() {
        let (ing, rules) = store("scoped");
        let Capture::Stored(mine) = ing.capture("h", "kcb", &card("100.00", "Java"), &rules)
            .expect("mine") else { panic!("stored") };
        ing.capture("other", "kcb", &card("100.00", "Java"), &rules).expect("theirs");

        let out = ing.learn_skip("h", &mine.id).expect("learn");
        assert_eq!(out.cleared, 1, "only this household's");
        let theirs = ing
            .current()
            .expect("current")
            .into_iter()
            .find(|m| m.sustain_id == "other")
            .expect("still there");
        assert!(theirs.needs_attention(), "another household is not his to clear");
    }

    #[test]
    fn a_skipped_message_keeps_its_text_and_its_reason() {
        // ★★ Set aside, never discarded. "Why did that vanish?" has an answer.
        let (ing, rules) = store("kept");
        let raw = card("100.00", "Java");
        let Capture::Stored(m) = ing.capture("h", "kcb", &raw, &rules).expect("capture")
            else { panic!("stored") };
        ing.learn_skip("h", &m.id).expect("learn");

        let after = ing.current().expect("current");
        let s = after.iter().find(|x| x.id == m.id).expect("still on record");
        assert!(s.ignored);
        assert_eq!(s.raw_payload, raw);
    }

    #[test]
    fn forgetting_a_rule_stops_it_applying_to_what_comes_next() {
        // ★★ Reversible. Nothing already set aside comes back on its own,
        //    because un-deciding a decision he made is not this to do.
        let (ing, rules) = store("forget");
        let Capture::Stored(m) = ing.capture("h", "kcb", &card("100.00", "Java"), &rules)
            .expect("capture") else { panic!("stored") };
        ing.learn_skip("h", &m.id).expect("learn");
        let key = Ingested::skip_key(&m).expect("a key");

        assert!(ing.forget_skip(&key).expect("forget"));
        assert!(!ing.forget_skip(&key).expect("again"), "already gone");

        let Capture::Stored(later) = ing.capture("h", "kcb", &card("999.00", "Shell"), &rules)
            .expect("later") else { panic!("stored") };
        assert!(!ing.is_skipped(&later).expect("check"), "it asks again");
    }
}

#[cfg(test)]
mod reclaim_tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-reclaim-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let ing = Ingested::at(scratch(name)).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    const REF: &str = "UHPB9480T9";
    const HIS_KCB: &str = "135***140";
    const HIS_MPESA: &str = "254***143";

    fn knows_his_numbers(ing: &Ingested) {
        ing.set_own_identifiers(&OwnIdentifiers {
            mpesa: vec!["254700000143".into()],
            kcb: vec!["1350000140".into()],
        })
        .expect("own");
    }

    fn kcb_leg(reference: &str) -> String {
        format!(
            "SEND TO M-PESA request of KES 2,000 from {HIS_KCB} to {HIS_MPESA} - \
             BONVENTURE NGUGI MAINA has been received for processing. M-PESA REF: {reference}"
        )
    }

    fn charge(amount: &str, who: &str) -> String {
        format!(
            "KES {amount} transaction made on KCB card 1234XXXXXXXX5678 at {who} \
             on 1/8/26 12:25pm, Avail balance KES 59,055.00"
        )
    }

    fn reversal(amount: &str, who: &str) -> String {
        format!(
            "Your card transaction of KES {amount} at {who} on 1/8/26 has been reversed. \
             Avail balance KES 59,155.00"
        )
    }

    #[test]
    fn a_skipped_transfer_comes_back_when_its_reference_matches() {
        // ★★★ Exactly what happened to him: he waved both halves past
        //     impatiently, and under the old rule they were gone for good.
        let (ing, rules) = store("transfer");
        knows_his_numbers(&ing);
        let Capture::Stored(m) = ing.capture("h", "kcb", &kcb_leg(REF), &rules).expect("capture")
            else { panic!("stored") };
        assert!(ing.ignore(&m.id).expect("skip"), "he set it aside");

        let r = ing.find_transfers("h").expect("transfers");
        assert_eq!(r.self_moves.len(), 1, "the reference brought it back");
        assert!(r.self_moves[0].reclaimed_from_skip);
        assert_eq!(r.reclaimed, 1, "and it is counted, not silent");
    }

    #[test]
    fn a_genuine_not_a_transaction_stays_skipped_for_good() {
        // ★★★ The property that makes this safe. A promo quotes no reference
        //     another message shares, so nothing in here can drag it back.
        let (ing, rules) = store("promo");
        knows_his_numbers(&ing);
        let Capture::Stored(m) = ing
            .capture("h", "kcb", "Dear customer, get a loan today. Reply STOP to opt out.", &rules)
            .expect("capture") else { panic!("stored") };
        assert!(ing.ignore(&m.id).expect("skip"));

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.self_moves.is_empty());
        assert_eq!(r.reclaimed, 0);
        let after = ing.current().expect("current");
        assert!(after.iter().find(|x| x.id == m.id).expect("there").ignored, "still set aside");
    }

    #[test]
    fn a_resemblance_never_overturns_a_skip() {
        // ★★★ The heart of it. Amount and merchant say two texts COULD be the
        //     same movement; only a reference says they ARE. A skip is his
        //     judgement, and a likeness is not enough to undo one.
        let (ing, rules) = store("resemblance");
        let Capture::Stored(c) = ing.capture("h", "kcb", &charge("100.00", "Bolt KE"), &rules)
            .expect("charge") else { panic!("stored") };
        assert!(ing.ignore(&c.id).expect("skip"), "he set the charge aside");
        ing.capture("h", "kcb", &reversal("100.00", "Bolt KE"), &rules).expect("refund");

        let r = ing.net_reversals("h").expect("net");
        assert!(
            r.netted.is_empty(),
            "same amount, same merchant, no reference — his decision stands"
        );
        let after = ing.current().expect("current");
        assert!(after.iter().find(|x| x.id == c.id).expect("there").ignored);
    }

    #[test]
    fn reclaiming_records_that_it_was_set_aside_and_came_back() {
        // ★★ Both facts kept. "Why is this here again?" has an answer.
        let (ing, rules) = store("record");
        knows_his_numbers(&ing);
        let Capture::Stored(m) = ing.capture("h", "kcb", &kcb_leg(REF), &rules).expect("capture")
            else { panic!("stored") };
        ing.ignore(&m.id).expect("skip");

        assert!(ing.reclaim(&m.id).expect("reclaim"));
        let after = ing.current().expect("current");
        let x = after.iter().find(|x| x.id == m.id).expect("there");
        assert!(!x.ignored, "back in play");
        assert!(x.reclaimed, "and the record says why");
        assert!(x.needs_attention(), "so it asks again");
    }

    #[test]
    fn reclaiming_something_that_was_never_skipped_does_nothing() {
        let (ing, rules) = store("noop");
        let Capture::Stored(m) = ing.capture("h", "kcb", &charge("100.00", "Java"), &rules)
            .expect("capture") else { panic!("stored") };
        assert!(!ing.reclaim(&m.id).expect("reclaim"), "there was no decision to overturn");
    }

    #[test]
    fn settling_a_reclaimed_pair_keeps_both_facts_on_the_record() {
        // ★★ Netting a pair is also the moment a skip is overturned, and the
        //    message keeps both: it was set aside, and then it came back.
        let (ing, rules) = store("settle");
        let Capture::Stored(c) = ing.capture("h", "kcb", &charge("100.00", "Java"), &rules)
            .expect("charge") else { panic!("stored") };
        let Capture::Stored(r) = ing.capture("h", "kcb", &reversal("100.00", "Java"), &rules)
            .expect("refund") else { panic!("stored") };
        ing.ignore(&c.id).expect("skip");

        // Settle them by hand, as the caller does once the gate has spoken.
        ing.mark_compensated(&r.id, &c.id).expect("settle");
        let after = ing.current().expect("current");
        let x = after.iter().find(|x| x.id == c.id).expect("there");
        assert!(!x.ignored);
        assert!(x.reclaimed, "it says it came back");
        assert_eq!(x.netted_with.as_deref(), Some(r.id.as_str()));
    }

    #[test]
    fn a_skipped_message_with_no_reference_is_not_even_a_candidate() {
        // ★★ The pool is filtered on HAVING a reference, so a skipped message
        //    without one never reaches the matching logic at all.
        let (ing, rules) = store("noref");
        knows_his_numbers(&ing);
        let no_ref = format!(
            "SEND TO M-PESA request of KES 2,000 from {HIS_KCB} to {HIS_MPESA} - \
             BONVENTURE NGUGI MAINA has been received for processing."
        );
        let Capture::Stored(m) = ing.capture("h", "kcb", &no_ref, &rules).expect("capture")
            else { panic!("stored") };
        assert_eq!(m.reference(), None);
        ing.ignore(&m.id).expect("skip");

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.self_moves.is_empty(), "no reference, no way back");
        assert_eq!(r.reclaimed, 0);
    }
}

#[cfg(test)]
mod filed_tests {
    use super::*;
    use std::env;

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let dir = env::temp_dir().join(format!("sustena-filed-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let ing = Ingested::at(dir).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    fn spend_params(pocket: &str, amount: f64) -> BTreeMap<String, serde_json::Value> {
        BTreeMap::from([
            ("pocket_name".to_string(), serde_json::json!(pocket)),
            ("amount".to_string(), serde_json::json!(amount)),
        ])
    }

    fn a_spend(ing: &Ingested, rules: &[ParseRule], who: &str) -> String {
        let raw = format!(
            "QGH7XJ4P2Q Confirmed. Ksh90.00 paid to {who} on 20/7/26 at 4:30 PM. \
             New M-PESA balance is Ksh12,050.00"
        );
        let Capture::Stored(m) = ing.capture("h", "mpesa", &raw, rules).expect("capture")
            else { panic!("stored") };
        m.id
    }

    #[test]
    fn a_filed_spend_can_be_found_again() {
        // ★★★ Without this there is nothing to correct FROM. A tx could be
        //     classified and then never touched again.
        let (ing, rules) = store("find");
        let id = a_spend(&ing, &rules, "NAIVAS");
        ing.record_filing(&id, "budget.spend", &spend_params("rent", 90.0)).expect("filed");
        ing.record_outcome(&id, true, None).expect("resolved");

        let filed = ing.filed_spends("h", 12).expect("filed");
        assert_eq!(filed.len(), 1);
        assert_eq!(filed[0].pocket, "rent");
        assert_eq!(filed[0].amount, 90.0);
        assert_eq!(filed[0].counterparty, "NAIVAS");
    }

    #[test]
    fn a_correction_appends_and_the_list_shows_where_it_stands_now() {
        // ★★★ The original filing is not rewritten. `filed` keeps both, and
        //     the list reads the last one — so the record can show he changed
        //     his mind while the screen shows the current answer.
        let (ing, rules) = store("append");
        let id = a_spend(&ing, &rules, "NAIVAS");
        ing.record_filing(&id, "budget.spend", &spend_params("rent", 90.0)).expect("filed");
        ing.record_outcome(&id, true, None).expect("resolved");

        ing.record_reclassification(&id, "food", 90.0).expect("corrected");

        let m = ing.current().expect("current").into_iter().find(|m| m.id == id).expect("there");
        assert_eq!(m.filed.len(), 2, "both filings are on the record");
        assert_eq!(m.filed[0].params["pocket_name"], serde_json::json!("rent"), "the first stands");
        assert_eq!(m.filed[1].params["pocket_name"], serde_json::json!("food"));

        let filed = ing.filed_spends("h", 12).expect("filed");
        assert_eq!(filed[0].pocket, "food", "and the list shows where it is now");
    }

    #[test]
    fn an_allocation_is_not_something_to_correct() {
        // ★★ Only spends. An allocation is not a filing anyone gets wrong in
        //    this sense.
        let (ing, rules) = store("alloc");
        let id = a_spend(&ing, &rules, "NAIVAS");
        ing.record_filing(&id, "budget.allocate", &spend_params("rent", 90.0)).expect("filed");
        ing.record_outcome(&id, true, None).expect("resolved");
        assert!(ing.filed_spends("h", 12).expect("filed").is_empty());
    }

    #[test]
    fn a_netted_or_unresolved_message_is_not_offered() {
        let (ing, rules) = store("skip");
        // Still in the queue.
        let open = a_spend(&ing, &rules, "PENDING");
        ing.record_filing(&open, "budget.spend", &spend_params("rent", 90.0)).expect("filed");
        assert!(ing.filed_spends("h", 12).expect("filed").is_empty(), "not resolved");

        // Resolved, then netted against its refund.
        let netted = a_spend(&ing, &rules, "REFUNDED");
        ing.record_filing(&netted, "budget.spend", &spend_params("rent", 90.0)).expect("filed");
        ing.record_outcome(&netted, true, None).expect("resolved");
        ing.mark_compensated(&netted, &open).expect("netted");
        assert!(
            ing.filed_spends("h", 12).expect("filed").iter().all(|f| f.message_id != netted),
            "a cancelled spend is not one to re-file"
        );
    }

    #[test]
    fn the_newest_filings_come_first_and_the_list_is_bounded() {
        let (ing, rules) = store("order");
        for n in 0..5 {
            let raw = format!(
                "QGH7XJ4P2Q Confirmed. Ksh{n}0.00 paid to SHOP{n} on 20/7/26 at 4:30 PM. \
                 New M-PESA balance is Ksh12,050.00"
            );
            let Capture::Stored(m) = ing.capture("h", "mpesa", &raw, &rules).expect("capture")
                else { panic!("stored") };
            ing.record_filing(&m.id, "budget.spend", &spend_params("rent", 10.0)).expect("filed");
            ing.record_outcome(&m.id, true, None).expect("resolved");
        }
        let filed = ing.filed_spends("h", 3).expect("filed");
        assert_eq!(filed.len(), 3, "bounded");
        assert!(filed[0].seq > filed[1].seq, "newest first");
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    use std::env;

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let dir = env::temp_dir().join(format!("sustena-queue-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let ing = Ingested::at(dir).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    fn capture(ing: &Ingested, rules: &[ParseRule], who: &str) -> String {
        let raw = format!(
            "QGH7XJ4P2Q Confirmed. Ksh90.00 paid to {who} on 20/7/26 at 4:30 PM. \
             New M-PESA balance is Ksh12,050.00"
        );
        let Capture::Stored(m) = ing.capture("h", "mpesa", &raw, rules).expect("capture")
            else { panic!("stored") };
        m.id
    }

    fn ids(ms: &[IngestedMessage]) -> Vec<String> {
        ms.iter().map(|m| m.id.clone()).collect()
    }

    #[test]
    fn the_queue_runs_newest_first_when_nothing_has_been_put_off() {
        // ★★★ The just-arrived transaction is the one he was notified about
        //     and the one he still remembers making, so it is the cheapest to
        //     answer. Recall decays fast; an oldest-first queue asks the
        //     hardest question first and makes the pile feel like homework.
        let (ing, rules) = store("plain");
        let a = capture(&ing, &rules, "SHOPA");
        let b = capture(&ing, &rules, "SHOPB");
        assert_eq!(ids(&ing.navigable("h", 5, 50).expect("q")), vec![b, a]);
    }

    #[test]
    fn something_he_put_off_comes_back_at_the_top() {
        // ★★★ The whole point of an honest defer. Burying it under new
        //     arrivals would make putting something off indistinguishable from
        //     throwing it away.
        let (ing, rules) = store("defer-first");
        let first = capture(&ing, &rules, "SHOPA");
        let second = capture(&ing, &rules, "SHOPB");
        let third = capture(&ing, &rules, "SHOPC");
        assert!(ing.defer(&second, 7).expect("defer"));

        let q = ids(&ing.navigable("h", 5, 50).expect("q"));
        assert_eq!(q[0], second, "the one he put off is first next time");
        // ★★ Newest-first applies WITHIN the group that has not been put off,
        //    rather than flattening the two. Recency orders things competing
        //    equally; it does not override a decision he already made.
        assert_eq!(q[1..], [third, first][..], "and the rest are newest-first");
    }

    #[test]
    fn the_longest_put_off_comes_back_before_the_rest() {
        let (ing, rules) = store("defer-order");
        let a = capture(&ing, &rules, "SHOPA");
        let b = capture(&ing, &rules, "SHOPB");
        ing.defer(&b, 3).expect("deferred earlier");
        ing.defer(&a, 9).expect("deferred later");
        assert_eq!(ids(&ing.navigable("h", 5, 50).expect("q"))[0], b);
    }

    #[test]
    fn putting_something_off_is_not_answering_it() {
        // ★★★ It stays in the queue. Deferring changes the ORDER, never
        //     whether it still needs him — which is what separates it from
        //     "not a transaction".
        let (ing, rules) = store("still-waiting");
        let id = capture(&ing, &rules, "SHOPA");
        ing.defer(&id, 5).expect("defer");
        let m = ing.current().expect("cur").into_iter().find(|m| m.id == id).expect("there");
        assert!(m.needs_attention(), "still his to answer");
        assert!(!m.ignored, "and not marked as junk");
        assert_eq!(m.deferred_at, Some(5));
    }

    #[test]
    fn what_he_has_answered_stays_reachable_by_going_back() {
        // ★★★ Without this the back arrow has nowhere to go and a filing he
        //     got wrong is unreachable the moment it lands.
        let (ing, rules) = store("reachable");
        let done = capture(&ing, &rules, "SHOPA");
        ing.record_outcome(&done, true, None).expect("answered");
        let pending = capture(&ing, &rules, "SHOPB");

        let q = ing.navigable("h", 5, 50).expect("q");
        assert_eq!(ids(&q), vec![done.clone(), pending.clone()]);
        // And the card opens on work, not on history.
        assert_eq!(q.iter().position(|m| !m.resolved), Some(1));
    }

    #[test]
    fn only_a_short_tail_of_answered_messages_is_reachable() {
        // ★★ Changing your mind about this morning is a real need; walking
        //    back through a year is a different screen.
        let (ing, rules) = store("tail");
        for n in 0..8 {
            let id = capture(&ing, &rules, &format!("SHOP{n}"));
            ing.record_outcome(&id, true, None).expect("answered");
        }
        let q = ing.navigable("h", 3, 50).expect("q");
        assert_eq!(q.len(), 3, "the three most recent");
    }

    #[test]
    fn a_cancelled_pair_is_not_offered_for_review() {
        // ★★ A charge that netted against its refund is not a filing anyone
        //    needs to revisit.
        let (ing, rules) = store("netted");
        let a = capture(&ing, &rules, "SHOPA");
        let b = capture(&ing, &rules, "SHOPB");
        ing.record_outcome(&a, true, None).expect("answered");
        ing.mark_compensated(&a, &b).expect("netted");
        assert!(ing.navigable("h", 5, 50).expect("q").iter().all(|m| m.id != a));
    }

    #[test]
    fn the_queue_does_not_cross_households() {
        let (ing, rules) = store("scoped");
        let mine = capture(&ing, &rules, "SHOPA");
        let raw = "QGH7XJ4P2Q Confirmed. Ksh10.00 paid to OTHER on 20/7/26 at 4:30 PM. \
                   New M-PESA balance is Ksh1.00";
        ing.capture("other", "mpesa", raw, &rules).expect("theirs");
        assert_eq!(ids(&ing.navigable("h", 5, 50).expect("q")), vec![mine]);
    }
}

#[cfg(test)]
mod fact_key_tests {
    //! Ingest §IV — a reference is not a fact on its own.
    use super::*;
    use std::env;

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let dir = env::temp_dir().join(format!("sustena-fact-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let ing = Ingested::at(dir).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    /// A real M-Pesa received text, so the amount is genuinely parsed.
    fn received(reference: &str, amount: &str) -> String {
        format!(
            "{reference} Confirmed. You have received Ksh{amount} from MARY NGIGI 0726***961 \
             on 2/8/26 at 9:14 AM New M-PESA balance is Ksh5,000.00"
        )
    }

    fn capture(ing: &Ingested, rules: &[ParseRule], source: &str, raw: &str) -> IngestedMessage {
        let Capture::Stored(m) = ing.capture("h", source, raw, rules).expect("capture") else {
            panic!("stored")
        };
        *m
    }

    #[test]
    fn one_transfer_from_two_senders_is_still_joined() {
        // The behaviour that shipped, and it must keep working.
        let (ing, rules) = store("joined");
        let first = capture(&ing, &rules, "mpesa", &received("QGH7XJ4P2Q", "1,000.00"));
        ing.record_outcome(&first.id, true, None).expect("applied");

        let found = ing
            .same_fact_already_applied("h", "kcb", "QGH7XJ4P2Q", Some(1000.0))
            .expect("lookup");
        assert_eq!(found.as_deref(), Some(first.id.as_str()));
    }

    #[test]
    fn a_reference_collision_no_longer_swallows_a_real_transaction() {
        // ★★★ The finding. Matching on a reference ALONE meant a collision
        //     silently treated a separate transaction as already-applied, and
        //     it vanished without trace — nobody finds out, because the whole
        //     effect of the join is that nothing is shown.
        let (ing, rules) = store("collision");
        let first = capture(&ing, &rules, "mpesa", &received("QGH7XJ4P2Q", "1,000.00"));
        ing.record_outcome(&first.id, true, None).expect("applied");

        let joined = ing
            .same_fact_already_applied("h", "kcb", "QGH7XJ4P2Q", Some(40_000.0))
            .expect("lookup");
        assert!(joined.is_none(), "a different amount is a different transaction");
    }

    #[test]
    fn an_unknown_amount_falls_back_to_the_reference_rather_than_refusing() {
        // ★★ Narrowing further would start missing real pairs for messages
        //    whose rules extract no amount — and a missed join shows a
        //    duplicate a person can undo, which is the safe failure.
        let (ing, rules) = store("unknown-amount");
        let first = capture(&ing, &rules, "mpesa", &received("QGH7XJ4P2Q", "1,000.00"));
        ing.record_outcome(&first.id, true, None).expect("applied");

        let found =
            ing.same_fact_already_applied("h", "kcb", "QGH7XJ4P2Q", None).expect("lookup");
        assert_eq!(found.as_deref(), Some(first.id.as_str()));
    }

    #[test]
    fn the_old_entry_point_still_answers_the_old_question() {
        let (ing, rules) = store("compat");
        let first = capture(&ing, &rules, "mpesa", &received("QGH7XJ4P2Q", "1,000.00"));
        ing.record_outcome(&first.id, true, None).expect("applied");
        assert!(ing
            .same_event_already_applied("h", "kcb", "QGH7XJ4P2Q")
            .expect("lookup")
            .is_some());
    }
}

#[cfg(test)]
mod event_time_tests {
    //! Ingest §VIII — the message's own clock, at the door.
    use super::*;
    use std::env;

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let dir = env::temp_dir().join(format!("sustena-tevent-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let ing = Ingested::at(dir).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    fn capture(ing: &Ingested, rules: &[ParseRule], raw: &str) -> IngestedMessage {
        let Capture::Stored(m) = ing.capture("h", "mpesa", raw, rules).expect("capture") else {
            panic!("stored")
        };
        *m
    }

    #[test]
    fn a_captured_message_carries_the_moment_it_says_it_happened() {
        // ★★★ The second clock. Without it every capture carried only the
        //     moment the phone read it.
        let (ing, rules) = store("carries");
        let m = capture(
            &ing,
            &rules,
            "QGH7XJ4P2Q Confirmed. Ksh90.00 paid to NAIVAS on 20/7/26 at 4:30 PM. \
             New M-PESA balance is Ksh12,050.00",
        );
        let at = m.event_at_ms.expect("the message said when");
        // 2026-07-20 16:30 EAT is 13:30 UTC.
        let expect = sustena_core::LocalStamp {
            year: 2026,
            month: 7,
            day: 20,
            hour: 16,
            minute: 30,
        }
        .to_epoch_ms(180);
        assert_eq!(at, expect);
    }

    #[test]
    fn a_backfill_read_in_one_afternoon_still_spans_the_month_it_covers() {
        // ★★★ The failure this ends: a month of spending stamped with one
        //     afternoon, every window closing on the wrong side, and nothing
        //     able to tell because there was no second reading.
        let (ing, rules) = store("spread");
        let early = capture(
            &ing,
            &rules,
            "QGH7XJ4P2Q Confirmed. Ksh10.00 paid to SHOPA on 3/7/26 at 9:00 AM. \
             New M-PESA balance is Ksh1.00",
        );
        let late = capture(
            &ing,
            &rules,
            "QGH7XJ4P2R Confirmed. Ksh10.00 paid to SHOPB on 3/8/26 at 9:00 AM. \
             New M-PESA balance is Ksh1.00",
        );
        let gap = late.event_at_ms.unwrap() - early.event_at_ms.unwrap();
        assert!(gap > 30 * 86_400_000, "a month apart, not one afternoon");
    }

    #[test]
    fn a_message_with_no_readable_time_carries_none_rather_than_now() {
        // ★★★ `now` would make skew exactly zero — the one value that looks
        //     healthy — for precisely the messages nobody could read.
        let (ing, rules) = store("unreadable");
        let m = capture(&ing, &rules, "Dear customer, thank you for visiting our branch.");
        assert!(m.event_at_ms.is_none());
    }
}

#[cfg(test)]
mod liveness_tests {
    //! Ingest §VIII — never guess a cadence, at the door.
    use super::*;
    use std::env;

    const HOUR: i64 = 3_600_000;

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let dir = env::temp_dir().join(format!("sustena-live-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        let ing = Ingested::at(dir).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    /// A real M-Pesa text on a given day, so `event_at_ms` is genuinely parsed.
    fn on(day: u32, reference: &str) -> String {
        format!(
            "{reference} Confirmed. Ksh10.00 paid to SHOP on {day}/7/26 at 9:00 AM. \
             New M-PESA balance is Ksh1.00"
        )
    }

    fn refs(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("QGH7XJ4P{i:02}")).collect()
    }

    #[test]
    fn a_source_with_too_little_history_is_unknown_rather_than_healthy() {
        // ★★★ Calling it healthy hides a dead capture path; calling it stale
        //     cries wolf about a source that is simply new.
        let (ing, rules) = store("new");
        ing.capture("h", "mpesa", &on(1, "QGH7XJ4P01"), &rules).expect("capture");
        let l = ing.liveness_of("mpesa", i64::MAX / 4).expect("reads");
        assert!(matches!(l, sustena_core::Liveness::Unknown { .. }), "{l:?}");
    }

    #[test]
    fn a_source_that_has_kept_a_cadence_can_be_judged_against_it() {
        // ★★ Its OWN history, which is what makes the threshold not a guess.
        let (ing, rules) = store("cadence");
        for (i, r) in refs(8).into_iter().enumerate() {
            ing.capture("h", "mpesa", &on(i as u32 + 1, &r), &rules).expect("capture");
        }
        let last = ing.sources().expect("sources")[0].last_seen_ms.expect("timed");

        // A day later is what it always does; a fortnight is not.
        assert!(!ing.liveness_of("mpesa", last + 24 * HOUR).expect("reads").is_stale());
        assert!(ing.liveness_of("mpesa", last + 14 * 24 * HOUR).expect("reads").is_stale());
    }

    #[test]
    fn a_declared_cadence_is_watchable_from_the_first_message() {
        // ★★★ A promise beats an estimate, and needs no warm-up at all.
        let (ing, rules) = store("promised");
        ing.declare_source("mpesa", "M-Pesa", Some(60)).expect("declared");
        ing.capture("h", "mpesa", &on(1, "QGH7XJ4P01"), &rules).expect("capture");
        let last = ing.sources().expect("sources")[0].last_seen_ms.expect("timed");
        assert!(ing.liveness_of("mpesa", last + 3 * HOUR).expect("reads").is_stale());
        assert!(!ing.liveness_of("mpesa", last + 30 * 60_000).expect("reads").is_stale());
    }

    #[test]
    fn a_source_nobody_has_heard_from_is_unknown_not_stale() {
        let (ing, _) = store("silent");
        let l = ing.liveness_of("kcb", 0).expect("reads");
        assert!(matches!(l, sustena_core::Liveness::Unknown { .. }));
    }

    #[test]
    fn a_message_nobody_could_parse_still_counts_as_the_source_speaking() {
        // ★★★ Otherwise a source looks dead the moment its wording changes,
        //     which sends somebody to check the phone when the answer is "the
        //     format changed".
        let (ing, rules) = store("unparsed");
        ing.capture("h", "mpesa", "something no rule has ever seen", &rules).expect("capture");
        let source = &ing.sources().expect("sources")[0];
        assert_eq!(source.captures, 1);
        assert!(source.last_seen_seq.is_some(), "it spoke, whatever it said");
    }
}

#[cfg(test)]
mod intake_window_tests {
    use super::*;
    use sustena_core::intake_window::IntakeWindow;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mycelium-window-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    const START_MS: i64 = 1_788_000_000_000;
    const MSG: &str = "QGH7XJ2K9L Confirmed. You have received Ksh500.00 from A B \
                       on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh1,500.00";

    #[test]
    fn with_no_start_the_backlog_is_captured_as_before() {
        // ★★★ Open by default. Somebody who never touches this setting must
        //     lose nothing, so the old behaviour has to be the default one.
        let i = Ingested::at(scratch("open")).expect("store");
        assert!(i.window().is_open());
        let out = i.capture_at("h", "mpesa", MSG, &[], Some(1_000)).expect("capture");
        assert!(matches!(out, Capture::Stored(_)), "got {out:?}");
    }

    #[test]
    fn a_message_from_before_the_start_is_never_stored() {
        // ★★★ The whole feature: starting today must not drag in years of
        //     texts. NOT stored, not queued, not counted as a refusal.
        let i = Ingested::at(scratch("before")).expect("store");
        i.set_window(IntakeWindow::starting_at(START_MS / 1000)).expect("set");

        let out = i
            .capture_at("h", "mpesa", MSG, &[], Some(START_MS - 86_400_000))
            .expect("capture");
        match out {
            Capture::BeforeStart { at, start } => assert_eq!(start - at, 86_400),
            other => panic!("expected BeforeStart, got {other:?}"),
        }
        assert!(i.messages().expect("messages").is_empty(), "nothing was written");
    }

    #[test]
    fn a_message_at_or_after_the_start_is_captured() {
        let i = Ingested::at(scratch("after")).expect("store");
        i.set_window(IntakeWindow::starting_at(START_MS / 1000)).expect("set");

        let at_start = i.capture_at("h", "mpesa", MSG, &[], Some(START_MS)).expect("capture");
        assert!(matches!(at_start, Capture::Stored(_)), "the start itself: {at_start:?}");

        let later = "QGH7XJ4P2Q Confirmed. You have received Ksh900.00 from C D \
                     on 21/7/26 at 9:00 AM. New M-PESA balance is Ksh2,400.00";
        let after = i
            .capture_at("h", "mpesa", later, &[], Some(START_MS + 3_600_000))
            .expect("capture");
        assert!(matches!(after, Capture::Stored(_)), "after: {after:?}");
        assert_eq!(i.messages().expect("messages").len(), 2);
    }

    #[test]
    fn an_undated_message_is_admitted_rather_than_lost() {
        // ★★★ Refusing needs certainty; admitting only needs doubt. Losing a
        //     real payment to a missing timestamp would be far worse than one
        //     extra question in the queue.
        let i = Ingested::at(scratch("undated")).expect("store");
        i.set_window(IntakeWindow::starting_at(START_MS / 1000)).expect("set");
        let out = i.capture_at("h", "mpesa", MSG, &[], None).expect("capture");
        assert!(matches!(out, Capture::Stored(_)), "got {out:?}");
    }

    #[test]
    fn the_start_survives_a_restart() {
        // ★★★ A cutoff that forgot itself on restart would re-open the backlog
        //     the person had just closed -- and they would have no way to know.
        let dir = scratch("persist");
        {
            let i = Ingested::at(&dir).expect("store");
            i.set_window(IntakeWindow::starting_at(START_MS / 1000)).expect("set");
        }
        let again = Ingested::at(&dir).expect("reopen");
        assert_eq!(again.window().start_at, Some(START_MS / 1000));
        let out = again
            .capture_at("h", "mpesa", MSG, &[], Some(START_MS - 1_000))
            .expect("capture");
        assert!(matches!(out, Capture::BeforeStart { .. }), "still excluded: {out:?}");
    }

    #[test]
    fn moving_the_start_never_deletes_what_was_already_captured() {
        // ★★★ A boundary that retroactively erased a household's record would
        //     be far worse than the backlog it was set to avoid. It governs
        //     what is admitted FROM NOW ON, and nothing else.
        let i = Ingested::at(scratch("no-retro")).expect("store");
        i.capture_at("h", "mpesa", MSG, &[], Some(START_MS - 86_400_000)).expect("capture");
        assert_eq!(i.messages().expect("messages").len(), 1);

        i.set_window(IntakeWindow::starting_at(START_MS / 1000)).expect("set");
        assert_eq!(
            i.messages().expect("messages").len(),
            1,
            "the record it already had is untouched"
        );
    }
}

#[cfg(test)]
mod per_account_balance_tests {
    //! One sender is not one account: M-Pesa, Pochi and M-Shwari all arrive
    //! from the same sender and are three different pots.
    use super::*;

    fn msg(source: &str, fields: &[(&str, serde_json::Value)]) -> IngestedMessage {
        let mut m = IngestedMessage {
            id: "x".into(),
            sustain_id: "h".into(),
            source_id: source.into(),
            raw_payload: String::new(),
            dedup_key: "k".into(),
            status: "parsed_unmapped".into(),
            parser_name: "p".into(),
            reason: String::new(),
            parsed_fields: Default::default(),
            external_ref: None,
            operator: None,
            params: Default::default(),
            applied: false,
            gate_reason: None,
            resolved: false,
            netted_with: None,
            same_event_as: None,
            reclaimed: false,
            deferred_at: None,
            filed: Vec::new(),
            ignored: false,
            sent_at_ms: None,
            event_at_ms: None,
            seq: 1,
        };
        for (k, v) in fields {
            m.parsed_fields.insert((*k).into(), v.clone());
        }
        m
    }

    #[test]
    fn the_main_wallet_is_keyed_by_its_sender() {
        let m = msg("mpesa", &[("balance_after", serde_json::json!(12154.47))]);
        assert_eq!(reported_account_of(&m), Some(("mpesa".into(), 12154.47)));
    }

    #[test]
    fn pochi_and_mshwari_are_their_own_pots_not_the_wallet() {
        // ★★★ The whole point. Both arrive from the M-Pesa sender; keying by
        //     sender would collapse three balances into whichever text landed
        //     last, and the household would be shown one number for three pots.
        let pochi = msg(
            "mpesa",
            &[("instrument", serde_json::json!("pochi")), ("pochi_balance", serde_json::json!(3400.0))],
        );
        assert_eq!(reported_account_of(&pochi), Some(("pochi".into(), 3400.0)));

        let mshwari = msg(
            "mpesa",
            &[("instrument", serde_json::json!("mshwari")), ("mshwari_balance", serde_json::json!(20000.0))],
        );
        assert_eq!(reported_account_of(&mshwari), Some(("mshwari".into(), 20000.0)));
    }

    #[test]
    fn a_pochi_receipt_states_the_business_balance_in_balance_after() {
        // ★★★ The trap. `mpesa_pochi_received` puts the BUSINESS balance in
        //     `balance_after`, so reading the field name alone would file it as
        //     the main wallet. The instrument is what settles it.
        let m = msg(
            "mpesa",
            &[("instrument", serde_json::json!("pochi")), ("balance_after", serde_json::json!(880.0))],
        );
        assert_eq!(reported_account_of(&m), Some(("pochi".into(), 880.0)));
    }

    #[test]
    fn a_loan_balance_is_never_reported_as_money_held() {
        // ★★★ The most dangerous wrong number this app could print. KCB's loan
        //     texts state what is OWED; showing that as a balance would tell a
        //     household it has money it does not have.
        let m = msg("kcb", &[("loan_balance", serde_json::json!(5000.0))]);
        assert_eq!(reported_account_of(&m), None, "debt is not a balance");
    }

    #[test]
    fn a_message_stating_no_balance_reports_none() {
        assert_eq!(reported_account_of(&msg("mpesa", &[])), None);
    }

    #[test]
    fn a_comma_formatted_balance_is_read_as_a_number() {
        // ★ Real texts write "Ksh12,154.47"; the field arrives as a string.
        let m = msg("kcb", &[("actual_balance", serde_json::json!("59,055.00"))]);
        assert_eq!(reported_account_of(&m), Some(("kcb".into(), 59_055.00)));
    }

    #[test]
    fn fuliza_states_no_balance_so_it_contributes_no_account() {
        // ★ It carries an instrument but no balance field -- an instrument
        //   alone must not conjure a pot with an unknown figure in it.
        let m = msg("mpesa", &[("instrument", serde_json::json!("fuliza"))]);
        assert_eq!(reported_account_of(&m), None);
    }
}

#[cfg(test)]
mod queue_cap_tests {
    //! The window is finite; what it drops must never be the thing that just
    //! arrived and has not been seen.
    use super::*;

    fn waiting(seq: u64, deferred: Option<u64>) -> IngestedMessage {
        IngestedMessage {
            id: format!("m{seq}"),
            sustain_id: "h".into(),
            source_id: "mpesa".into(),
            raw_payload: format!("text {seq}"),
            dedup_key: format!("k{seq}"),
            status: "parsed_unmapped".into(),
            parser_name: "p".into(),
            reason: String::new(),
            parsed_fields: Default::default(),
            external_ref: None,
            operator: None,
            params: Default::default(),
            applied: false,
            gate_reason: None,
            resolved: false,
            netted_with: None,
            same_event_as: None,
            reclaimed: false,
            deferred_at: deferred,
            filed: Vec::new(),
            ignored: false,
            sent_at_ms: None,
            event_at_ms: None,
            seq,
        }
    }

    /// `navigable`'s selection, over an already-sorted waiting list.
    fn window(waiting: Vec<IngestedMessage>, limit: usize) -> Vec<String> {
        let mut w = waiting;
        w.sort_by_key(|m| (m.deferred_at.is_none(), m.deferred_at, Reverse(m.seq)));
        let (put_off, fresh): (Vec<_>, Vec<_>) = w.into_iter().partition(|m| m.deferred_at.is_some());
        let room = limit.saturating_sub(fresh.len());
        let mut out: Vec<String> = put_off.into_iter().take(room).map(|m| m.id).collect();
        out.extend(fresh.into_iter().take(limit).map(|m| m.id));
        out
    }

    #[test]
    fn a_just_captured_message_survives_a_full_window() {
        // ★★★ The reported failure. Enough postponed messages to fill the
        //     window, then one that just arrived -- which must still be there.
        let mut all: Vec<IngestedMessage> =
            (1..=5).map(|i| waiting(i, Some(i * 100))).collect();
        all.push(waiting(99, None));

        let got = window(all, 5);
        assert!(got.contains(&"m99".to_string()), "the new capture is reachable: {got:?}");
    }

    #[test]
    fn what_was_put_off_still_comes_first_when_there_is_room() {
        // ★★ The documented behaviour, unchanged: deferring means it comes
        //    back, and it comes back at the top.
        let got = window(vec![waiting(1, None), waiting(2, Some(50)), waiting(3, None)], 10);
        assert_eq!(got, vec!["m2", "m3", "m1"]);
    }

    #[test]
    fn the_postponed_group_is_what_yields_when_space_runs_out() {
        // ★ A thing set aside has already been seen; a thing that just arrived
        //   has not. So the trimming falls on the group he has looked at.
        let mut all: Vec<IngestedMessage> = (1..=4).map(|i| waiting(i, Some(i))).collect();
        all.push(waiting(50, None));
        all.push(waiting(51, None));

        let got = window(all, 3);
        assert!(got.contains(&"m51".to_string()) && got.contains(&"m50".to_string()));
        assert_eq!(got.len(), 3, "the window is still respected");
    }
}

#[cfg(test)]
mod reparse_tests {
    //! Teaching a format makes the messages already in that format readable --
    //! and never files them.
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("mycelium-reparse-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("scratch");
        p
    }

    /// A rule that reads a shape the shipped set does not.
    fn house_rule() -> ParseRule {
        serde_json::from_value(serde_json::json!({
            "id": "learned_test_shape",
            "source": "mpesa",
            "version": 1,
            "pattern": r"CHAMA dues Ksh\s*(?P<amount>[\d,]+(?:\.\d{1,2})?) received",
            "extract": {
                "amount": { "type": "amount", "group": "amount", "value": null },
                "direction": { "type": "literal", "group": null, "value": "received" }
            },
            "status": "parsed_unmapped",
            "operator": null,
            "params": {},
            "reason_template": "a chama contribution"
        }))
        .expect("a well-formed rule")
    }

    fn capture_unreadable(ing: &Ingested, raw: &str) -> String {
        match ing.capture_at("h", "mpesa", raw, &[], Some(1_788_000_000_000)).expect("capture") {
            Capture::Stored(m) => {
                assert_eq!(m.status, "unparsed", "the shipped rules must not read this");
                m.id
            }
            other => panic!("expected a stored message, got {other:?}"),
        }
    }

    #[test]
    fn the_messages_already_in_that_shape_become_readable() {
        // ★★★ The promise the shape card makes. Three sitting unreadable; one
        //     rule; all three readable, without three separate corrections.
        let ing = Ingested::at(scratch("siblings")).expect("ingest");
        let a = capture_unreadable(&ing, "CHAMA dues Ksh 500.00 received today");
        let b = capture_unreadable(&ing, "CHAMA dues Ksh 1,200.00 received today");
        let c = capture_unreadable(&ing, "CHAMA dues Ksh 80.00 received today");

        let n = ing.reparse_unparsed("h", &[house_rule()]).expect("reparse");
        assert_eq!(n, 3, "every sibling was re-read");

        let now = ing.current().expect("current");
        for id in [&a, &b, &c] {
            let m = now.iter().rev().find(|m| &m.id == id).expect("the message");
            assert_eq!(m.status, "parsed_unmapped", "readable");
            assert!(m.parsed_fields.contains_key("amount"), "with its amount read");
        }
    }

    #[test]
    fn a_reparse_never_files_anything() {
        // ★★★ The safety property, and the reason this is not just "re-run
        //     capture". These have been sitting for days and some may already
        //     have been recorded by hand; applying them because one rule was
        //     taught could double-count a household's month in a single tap.
        let ing = Ingested::at(scratch("never-files")).expect("ingest");
        capture_unreadable(&ing, "CHAMA dues Ksh 500.00 received today");

        ing.reparse_unparsed("h", &[house_rule()]).expect("reparse");

        let m = ing.current().expect("current").pop().expect("the message");
        assert!(!m.applied, "nothing was filed");
        assert_ne!(m.status, "mapped", "and it was not marked as if it had been");
        assert!(m.needs_attention(), "it still asks the person");
    }

    #[test]
    fn a_message_somebody_already_answered_is_left_alone() {
        // ★★ Re-reading a message he has dealt with would undo his answer.
        let ing = Ingested::at(scratch("resolved")).expect("ingest");
        let id = capture_unreadable(&ing, "CHAMA dues Ksh 500.00 received today");
        assert!(ing.resolve(&id).expect("resolve"));

        assert_eq!(ing.reparse_unparsed("h", &[house_rule()]).expect("reparse"), 0);
        let m = ing.current().expect("current").into_iter().rev().find(|m| m.id == id).expect("m");
        assert!(m.resolved, "his answer stands");
    }

    #[test]
    fn a_message_no_rule_reads_is_untouched() {
        // ★ A rule that does not match must change nothing at all.
        let ing = Ingested::at(scratch("no-match")).expect("ingest");
        capture_unreadable(&ing, "Some notice that matches nothing at all");
        assert_eq!(ing.reparse_unparsed("h", &[house_rule()]).expect("reparse"), 0);
    }
}

#[cfg(test)]
mod paired_transfer_undo_tests {
    //! A transfer must net to zero however the two texts arrive.
    //!
    //! ★★★ The arriving leg files itself as income the moment it is captured --
    //! every text saying money arrived reads the same whether it came from an
    //! employer or from his own other account. If the leaving leg is read
    //! afterwards, the income is already on the books, and booking the transfer
    //! on top of it counts the money twice.
    use super::*;

    const REF: &str = "UHQB94FQ88";

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let p = std::env::temp_dir().join(format!("mycelium-pairundo-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("scratch");
        let ing = Ingested::at(p).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    fn his_numbers(ing: &Ingested) {
        ing.set_own_identifiers(&OwnIdentifiers { mpesa: vec!["254700000143".into()], kcb: vec!["112233".into()] })
            .expect("identifiers");
    }

    /// Money leaving M-Pesa for his own KCB account, quoting the shared ref.
    fn out_leg() -> String {
        format!(
            "{REF} Confirmed. Ksh2,000.00 sent to KCB BANK for account 112233 on 2/8/26 \
             at 9:00 AM. New M-PESA balance is Ksh12,050.00"
        )
    }

    /// KCB's own words for that same money arriving, quoting the SAME ref.
    fn in_leg() -> String {
        format!(
            "Ksh2,000.00 sent to KCB account PLACEHOLDER NAME 112233 has been received \
             on 01/08/2026. M-PESA Ref {REF}"
        )
    }

    #[test]
    fn a_filed_income_is_named_for_undoing_when_the_refs_agree() {
        // ★★★ The order that used to overstate him: the arriving leg lands and
        //     files itself, the leaving leg is read later. The pair must still
        //     become one move, with the income named so it comes back off.
        let (ing, rules) = store("undo");
        his_numbers(&ing);

        let inn = ing.capture("h", "kcb", &in_leg(), &rules).expect("in");
        let in_id = match inn {
            Capture::Stored(m) => m.id,
            other => panic!("expected a stored message, got {other:?}"),
        };
        // It really did apply itself on the way in.
        ing.record_outcome(&in_id, true, None).expect("applied");

        ing.capture("h", "mpesa", &out_leg(), &rules).expect("out");

        let r = ing.find_transfers("h").expect("transfers");
        assert_eq!(r.matched.len(), 1, "the pair is one move: {r:?}");
        assert_eq!(
            r.matched[0].undo_income.as_deref(),
            Some(in_id.as_str()),
            "and the income already filed is named so it can come back off",
        );
        assert_eq!(r.blocked_by_applied_income, 0, "it is no longer merely reported");
    }

    #[test]
    fn the_order_does_not_change_the_answer() {
        // ★★★ Leaving leg first this time. The arriving leg still maps and
        //     files itself the instant it is captured -- which makes the undo
        //     the NORMAL path here, not an edge case -- so the answer must be
        //     the same move with the same income named, either way round. A
        //     total that depends on which text arrived first is not a total.
        let (ing, rules) = store("order");
        his_numbers(&ing);
        ing.capture("h", "mpesa", &out_leg(), &rules).expect("out");
        let inn = ing.capture("h", "kcb", &in_leg(), &rules).expect("in");
        let in_id = match inn {
            Capture::Stored(m) => m.id,
            other => panic!("expected a stored message, got {other:?}"),
        };
        ing.record_outcome(&in_id, true, None).expect("applied");

        let r = ing.find_transfers("h").expect("transfers");
        assert_eq!(r.matched.len(), 1, "one move either way round: {r:?}");
        assert_eq!(r.matched[0].undo_income.as_deref(), Some(in_id.as_str()));
    }

    #[test]
    fn an_amount_lookalike_that_was_filed_is_not_undone() {
        // ★★★ The dangerous direction, refused. Undoing an income deletes
        //     something real, so it may act only on a shared reference -- never
        //     on two texts that merely happen to agree about a number.
        let (ing, rules) = store("lookalike");
        his_numbers(&ing);

        let inn = ing
            .capture(
                "h",
                "kcb",
                "Ksh2,000.00 sent to KCB account PLACEHOLDER NAME 112233 has been received \
                 on 01/08/2026. M-PESA Ref ZZZZ99ZZZZ",
                &rules,
            )
            .expect("in");
        if let Capture::Stored(m) = inn {
            ing.record_outcome(&m.id, true, None).expect("applied");
        }
        ing.capture("h", "mpesa", &out_leg(), &rules).expect("out");

        let r = ing.find_transfers("h").expect("transfers");
        assert!(r.matched.is_empty(), "a resemblance is not proof");
        assert_eq!(r.blocked_by_applied_income, 1, "reported, and left alone");
    }
}

#[cfg(test)]
mod skip_by_shape_tests {
    //! "Never ask me about these again" has to work for the messages that ask
    //! most -- the ones nothing can read.
    use super::*;

    fn store(name: &str) -> (Ingested, Vec<ParseRule>) {
        let p = std::env::temp_dir().join(format!("mycelium-skipshape-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("scratch");
        let ing = Ingested::at(p).expect("ingest");
        let rules = ing.effective_rules().expect("rules");
        (ing, rules)
    }

    /// Two texts of one unreadable shape, differing only in their particulars.
    fn notice(amount: &str, day: &str) -> String {
        format!("UHOB943IFK Notice: Ksh{amount} of your limit was reviewed on {day}/8/26 by SYSTEM")
    }

    #[test]
    fn a_shape_nothing_reads_can_still_be_silenced() {
        // ★★★ The case that was impossible. His inbox holds hundreds of alike
        //     texts nothing recognises, and the only answer available was to
        //     dismiss them one at a time.
        let (ing, rules) = store("silence");
        let first = match ing.capture("h", "mpesa", &notice("100", "1"), &rules).expect("a") {
            Capture::Stored(m) => {
                assert_eq!(m.status, "unparsed", "nothing reads it, which is the point");
                m.id
            }
            other => panic!("expected a stored message, got {other:?}"),
        };
        ing.capture("h", "mpesa", &notice("250", "2"), &rules).expect("b");
        ing.capture("h", "mpesa", &notice("999", "3"), &rules).expect("c");

        let learned = ing.learn_skip("h", &first).expect("skip");
        assert!(!learned.unlearnable, "an unread shape is still a shape");
        assert_eq!(learned.cleared, 3, "and the pile it was meant to clear is cleared");
        assert_eq!(waiting(&ing), 0);
    }

    #[test]
    fn silencing_one_shape_leaves_the_others_alone() {
        // ★★★ The rule must cover exactly the group he was shown and no wider
        //     net. Silencing one kind of notice must not silence his money.
        let (ing, rules) = store("narrow");
        let first = match ing.capture("h", "mpesa", &notice("100", "1"), &rules).expect("a") {
            Capture::Stored(m) => m.id,
            other => panic!("unexpected {other:?}"),
        };
        ing.capture("h", "mpesa", "Some entirely different text about something else", &rules)
            .expect("other");

        ing.learn_skip("h", &first).expect("skip");
        assert_eq!(waiting(&ing), 1, "the unrelated message still asks");
    }

    fn waiting(ing: &Ingested) -> usize {
        ing.current().expect("current").iter().filter(|m| m.needs_attention()).count()
    }
}
