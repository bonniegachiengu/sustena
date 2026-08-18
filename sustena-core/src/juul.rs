//! The juul ledger — `balance ← balance − pawa`, admitted only if
//! `balance ≥ pawa` (PAWA-2; the Pawa paper §2).
//!
//! **JUUL is the coin, PAWA is the gas.** [`crate::pawa`] measures what a run
//! cost; this module is the balance that cost is charged against.
//!
//! ## ★★★ The hard boundary — internal accounting, and nothing else
//!
//! ADR-0001 D5 and PAWA-13: a juul balance is an **internal accounting fact on
//! a single host**. It is **never real money, never a transferable asset, never
//! a payment rail**. Issuing a real, public, transferable token is a *distinct,
//! later, human-authorised act* and is out of scope.
//!
//! ★★ **Here that is structural, not a promise.** This module has:
//!
//! - **no transfer** — there is no method moving juul from one principal to
//!   another. A [`charge`](JuulLedger::charge) has **no counterparty**: it is a
//!   cost debit, not a payment. (PAWA-5's royalty flow is a later slice, and
//!   internal even then.)
//! - **two mints, both declared** — juul comes into existence only from a
//!   one-time [`Genesis`] (PAWA-7) or from ongoing, **governed**
//!   [`issuance`](crate::issuance) (PAWA-8), and both write the *same visible*
//!   [`Entry::Mint`], naming which authority allowed it. There is **no
//!   `mint(amount)`**: a genesis is a declared distribution and an issuance is
//!   a function of measured work.
//! - **no rail** — there is no I/O here at all, by ADR-0001.
//!
//! ## ★★★ A debit cannot exist without a measurement behind it
//!
//! [`JuulLedger::charge`] takes a **[`PawaReading`]**, not a number. And
//! `PawaReading` has no public constructor: the only way to obtain one is
//! [`crate::pawa::meter`], from an `Execution` that **committed**. So a debit is
//! always backed by a real, measured, genuinely-committed run — **there is no
//! `debit(amount)` that could invent a charge**, and a caller who wants to
//! spend juul must first have done work that cost it.
//!
//! ★ That is a real sharpening over the reference, whose `deduct(user_id,
//! amount, reason)` takes a bare integer — see the divergence note in
//! `conformance/vectors/juul.json`.
//!
//! ## ★★ The balance is a FOLD, never a mutable number
//!
//! Exactly like `state = fold(events)`: [`JuulLedger`] holds an **append-only**
//! list of [`Entry`] values, and the balance is their sum. There is no stored
//! `balance` field that could drift from the entries that produced it, and
//! [`JuulLedger::rebuild`] recomputes it from scratch — the same
//! `rebuild_state() == get_state()` discipline, one layer up.
//!
//! ## Refusal changes nothing
//!
//! ★★ A charge larger than the balance returns [`Charge::Insufficient`] — an
//! **honest refusal, not an error** — and appends **no entry**, so the balance
//! is byte-identical afterwards. ★ It never **clamps to zero**: clamping would
//! silently lose the overspend and report a debit that did not happen.

use std::collections::BTreeMap;

use crate::governance::Parameters;
use crate::issuance::{Issuance, Issued};
use crate::pawa::PawaReading;

/// The identity of a declared genesis.
///
/// ★★★ **No public constructor.** The only way to obtain one is
/// [`Genesis::declared`], so an [`Entry::Mint`] cannot be written down without
/// a real declaration behind it — the same mechanism as `Capability`, `Score`,
/// `VisualSpec` and `PawaReading`, applied to the authority to create money.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GenesisId(String);

impl GenesisId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GenesisId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// `g : principals → Juul` — the declared genesis distribution.
///
/// ★★ *Declared in the open once, and auditable forever.* It is a **value**,
/// not a procedure: a fixed map anyone can read, hash and check a ledger
/// against ([`Genesis::audit`]). Its answer to *who may mint* is structural
/// rather than governed — **nobody may, afterwards**, because
/// [`JuulLedger::from_genesis`] is a constructor and there is no method that
/// mints into a ledger that already exists.
///
/// ★ Ongoing minting is a different question and a different row: issuance
/// (PAWA-8) needs a **declared, revisable schedule changed only through
/// governance** (PAWA-11), which is exactly the machinery a one-time seed does
/// not need and therefore does not have here.
#[derive(Debug, Clone, PartialEq)]
pub struct Genesis {
    id: GenesisId,
    /// Ordered, so the declaration reads and hashes the same way twice.
    distribution: BTreeMap<String, f64>,
}

impl Genesis {
    /// Declare the distribution. ★ Refuses a negative allocation: a genesis
    /// that took juul from someone would not be a genesis.
    pub fn declared(id: &str, distribution: &[(&str, f64)]) -> Option<Genesis> {
        if id.trim().is_empty() || distribution.iter().any(|(_, a)| *a < 0.0 || !a.is_finite()) {
            return None;
        }
        Some(Genesis {
            id: GenesisId(id.to_string()),
            distribution: distribution.iter().map(|(p, a)| ((*p).to_string(), *a)).collect(),
        })
    }

    pub fn id(&self) -> &GenesisId {
        &self.id
    }

    /// What `g` allocates to a principal — `0.0` for anyone it does not name.
    pub fn allocation_for(&self, principal: &str) -> f64 {
        self.distribution.get(principal).copied().unwrap_or(0.0)
    }

    /// Every principal `g` names, with its allocation.
    pub fn distribution(&self) -> &BTreeMap<String, f64> {
        &self.distribution
    }

    /// `Σ g` — all the juul that has ever existed under this declaration.
    pub fn total(&self) -> f64 {
        self.distribution.values().sum()
    }

    /// ★★★ **Audit a ledger against this declaration.**
    ///
    /// *Declared in the open once, auditable forever*: every mint in the ledger
    /// must name **this** genesis and match **its** allocation, and every
    /// allocation must appear. So a ledger's entire money supply is explained by
    /// a value anyone can read — and a mint that exceeded, invented or omitted
    /// an allocation is **named**, not merely absent from a total.
    pub fn audit(&self, ledger: &JuulLedger) -> Audit {
        let mut minted: BTreeMap<&str, f64> = BTreeMap::new();
        let mut foreign = Vec::new();
        let mut issued: BTreeMap<String, f64> = BTreeMap::new();
        for e in ledger.entries() {
            if let Entry::Mint { principal, amount, authority } = e {
                match authority {
                    // ★★ An ISSUANCE mint is not foreign — it is legitimate, and
                    // a different question. Naming it keeps the audit's promise
                    // (the whole supply is explained) once there are two ways to
                    // raise circulation: silently skipping it would leave juul
                    // unaccounted for, and calling it foreign would report a
                    // governed mint as a defect.
                    MintAuthority::Issued(id) => {
                        *issued.entry(id.as_str().to_string()).or_insert(0.0) += *amount;
                        continue;
                    }
                    MintAuthority::Genesis(g) if g != &self.id => {
                        foreign.push(g.as_str().to_string());
                        continue;
                    }
                    MintAuthority::Genesis(_) => {}
                }
                *minted.entry(principal.as_str()).or_insert(0.0) += *amount;
            }
        }

        let mut mismatched = Vec::new();
        for (principal, declared) in &self.distribution {
            let got = minted.remove(principal.as_str()).unwrap_or(0.0);
            if got != *declared {
                mismatched.push((principal.clone(), *declared, got));
            }
        }
        // Anything left minted under this genesis was never declared in it.
        let undeclared: Vec<String> = minted.keys().map(|k| (*k).to_string()).collect();

        Audit { foreign, mismatched, undeclared, issued: issued.into_iter().collect() }
    }
}

/// What an audit found. ★ Empty on every field means the ledger's money supply
/// is **exactly** the declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Audit {
    /// Mints naming a different genesis than the one audited against.
    pub foreign: Vec<String>,
    /// `(principal, declared, minted)` where the two disagree.
    pub mismatched: Vec<(String, f64, f64)>,
    /// Principals minted to that `g` never named.
    pub undeclared: Vec<String>,
    /// ★★ `(issuance id, total minted under it)` — the juul this ledger holds
    /// that genesis **does not** explain, and is not supposed to.
    ///
    /// Not a fault: an issuance mint is legitimate and governed (PAWA-8). It is
    /// listed so the audit still accounts for the **whole** supply — the honest
    /// statement is now *exactly this genesis, plus this much issued* rather
    /// than *exactly this genesis*.
    pub issued: Vec<(String, f64)>,
}

impl Audit {
    pub fn clean(&self) -> bool {
        self.foreign.is_empty() && self.mismatched.is_empty() && self.undeclared.is_empty()
    }

    /// ★ What this genesis does not explain, and does not have to.
    pub fn issued_total(&self) -> f64 {
        self.issued.iter().map(|(_, a)| *a).sum()
    }

    pub fn describe(&self) -> String {
        if self.clean() {
            let issued = self.issued_total();
            if issued > 0.0 {
                return format!(
                    "the ledger's money supply is its declared genesis plus {issued} juul issued                      under {} declaration(s)",
                    self.issued.len()
                );
            }
            return "the ledger's money supply is exactly its declared genesis".to_string();
        }
        format!(
            "{} foreign mint(s), {} mismatched allocation(s), {} undeclared principal(s)",
            self.foreign.len(),
            self.mismatched.len(),
            self.undeclared.len()
        )
    }
}

/// What allowed a mint.
///
/// ★★★ **The two minting paths, distinguished in the entry itself.** A mint is
/// always the same visible circulation-raiser ([`Entry::Mint`]) — MYC-6's point
/// — but *where did this juul come from* has two different true answers, and
/// collapsing them would lose the difference between **a one-time seed anyone
/// can audit against a fixed value** and **an ongoing rule that keeps
/// applying**. Neither id has a public constructor, so neither answer can be
/// written down without a real declaration behind it.
#[derive(Debug, Clone, PartialEq)]
pub enum MintAuthority {
    /// PAWA-7's one-time seed: `g`, declared once, audited forever.
    Genesis(GenesisId),
    /// PAWA-8's ongoing mint, at a rate that moves **only** through governance.
    Issued(crate::issuance::IssuanceId),
}

impl MintAuthority {
    /// The declaration's id, whichever kind it is.
    pub fn id(&self) -> &str {
        match self {
            MintAuthority::Genesis(g) => g.as_str(),
            MintAuthority::Issued(i) => i.as_str(),
        }
    }

    pub fn is_genesis(&self) -> bool {
        matches!(self, MintAuthority::Genesis(_))
    }
}

/// One append-only ledger line.
///
/// ★ `Mint` and `Debit` are deliberately **different variants**, not a signed
/// number: a debit is a *cost that was measured and spent*, a mint is *juul
/// brought into existence under a named declaration*. Collapsing them would
/// make "where did this juul come from" unanswerable.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// ★★★ **A MINT** — juul brought into existence, raising total circulation.
    ///
    /// This variant exists because **a mint must be visible as a mint**
    /// (MYC-6). A transfer is bounded by the payer and self-limiting; a mint is
    /// **unbounded** — it inflates the unit and makes Sybil registration
    /// directly profitable. *Both are legitimate designs; only one of them is
    /// legitimate **undeclared**.* So minting is never a "credit that happens to
    /// go up": it is its own variant, and it **names the declaration that
    /// authorised it**, so *where did this juul come from* stays answerable
    /// forever.
    ///
    /// ★★ Neither [`GenesisId`] nor [`crate::issuance::IssuanceId`] has a
    /// public constructor, so a `Mint` cannot be written down without a real
    /// declaration behind it — a seed or a governed issuance.
    Mint { principal: String, amount: f64, authority: MintAuthority },
    /// A metered cost, spent. ★ Carries the **reading** it was charged from, so
    /// every debit can be traced to the run that incurred it.
    Debit { principal: String, amount: f64, operator: String, sustain: String, at: u64 },
    /// ★★★ A **conserved reallocation** between two balances — PAWA-5's royalty
    /// movement.
    ///
    /// **One entry, not a pair.** A `TransferOut` and a matching `TransferIn`
    /// would be two records that a caller could write half of; a single entry
    /// carrying *both ends* makes an unbalanced transfer **unspellable**. Its
    /// contribution to `from` is `−amount` and to `to` is `+amount`, so
    /// `ΣΔ = 0` **by the shape of the value**, not by a check run over it.
    ///
    /// ★★ And it is the **only** thing here that moves juul between principals.
    /// A [`Entry::Debit`] has no counterparty (a cost leaves circulation); a
    /// transfer has two and changes no total.
    Transfer { from: String, to: String, amount: f64, reason: String },
}

impl Entry {
    /// ★ This entry's signed effect on **one** principal's balance.
    ///
    /// A credit adds, a debit subtracts, and a transfer does **both** — which
    /// is why the signature takes a principal rather than returning a single
    /// number. `0.0` for anyone the entry does not touch.
    pub fn delta_for(&self, principal: &str) -> f64 {
        match self {
            Entry::Mint { principal: p, amount, .. } if p == principal => *amount,
            Entry::Debit { principal: p, amount, .. } if p == principal => -*amount,
            Entry::Transfer { from, amount, .. } if from == principal => -*amount,
            Entry::Transfer { to, amount, .. } if to == principal => *amount,
            _ => 0.0,
        }
    }

    /// Every principal this entry touches — one for a credit or debit, two for
    /// a transfer.
    pub fn touches(&self) -> Vec<&str> {
        match self {
            Entry::Mint { principal, .. } | Entry::Debit { principal, .. } => {
                vec![principal.as_str()]
            }
            Entry::Transfer { from, to, .. } => vec![from.as_str(), to.as_str()],
        }
    }

    /// ★★ This entry's effect on the **total** in circulation.
    ///
    /// ★★★ **Only a [`Entry::Mint`] raises it.** A debit lowers it (a cost has
    /// no counterparty, so spending removes juul from circulation) and a
    /// transfer is **exactly zero** — the conservation law, read straight off
    /// the variant rather than computed and checked.
    ///
    /// So the accounting is exactly:
    ///
    /// ```text
    /// circulation = Σ(genesis mints) + Σ(issuance mints) − Σ(costs debited)
    /// ```
    ///
    /// with transfers contributing nothing. ★★ The two mint terms are the
    /// **only** two ways it rises, and both are declared. There are no burns
    /// yet; if one is ever added it will be **its own variant**, for the same
    /// reason a mint is.
    pub fn circulation_delta(&self) -> f64 {
        match self {
            Entry::Mint { amount, .. } => *amount,
            Entry::Debit { amount, .. } => -*amount,
            Entry::Transfer { .. } => 0.0,
        }
    }

    /// Whether this entry brought juul into existence.
    pub fn is_mint(&self) -> bool {
        matches!(self, Entry::Mint { .. })
    }
}

/// What a charge did.
///
/// ★★ `Insufficient` is a **normal outcome**, not an error — the same shape as
/// a gate refusal. It reports both numbers so a caller can say *how much
/// short*, rather than only that it failed.
#[derive(Debug, Clone, PartialEq)]
pub enum Charge {
    /// Debited, and here is the balance afterwards.
    Charged { spent: f64, balance: f64 },
    /// ★ Refused. **Nothing was appended and the balance is unchanged.**
    Insufficient { required: f64, balance: f64 },
}

impl Charge {
    pub fn charged(&self) -> bool {
        matches!(self, Charge::Charged { .. })
    }

    /// How far short a refusal was. `None` when it was charged.
    pub fn shortfall(&self) -> Option<f64> {
        match self {
            Charge::Insufficient { required, balance } => Some(required - balance),
            Charge::Charged { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Charge::Charged { spent, balance } => {
                format!("charged {spent} juul; {balance} remaining")
            }
            Charge::Insufficient { required, balance } => format!(
                "insufficient juul: this run cost {required} and the balance is {balance}"
            ),
        }
    }
}

/// `balance : Principal → juul`, as the fold of an append-only entry list.
///
/// ★ Named `JuulLedger`, not `Ledger` — [`crate::consensus::Ledger`] is a Paxos
/// acceptor set and [`crate::approval::NonceLedger`] records spent approval
/// nonces. Three ledgers, three genuinely different jobs.
/// **Thirty-seventh collision**; the newcomer takes the longer name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JuulLedger {
    entries: Vec<Entry>,
}

impl JuulLedger {
    pub fn new() -> JuulLedger {
        JuulLedger::default()
    }

    /// ★★★ **Open a ledger from its declared genesis — the only mint there is.**
    ///
    /// A **constructor**, deliberately, not a method: you cannot *seed* a
    /// ledger that already exists, so genesis is **one-time by shape** and its
    /// answer to *who may mint* stays **nobody, afterwards**.
    ///
    /// ★★ Ongoing minting is [`JuulLedger::issue`] (PAWA-8) — a **separate**
    /// path, and separate on purpose. It is *not* a second genesis: it mints a
    /// function of measured work at a **governed** rate, so it needs a revision
    /// mechanism that a fixed, once-and-forever declaration does not.
    ///
    /// ★ [`JuulLedger::new`] remains, and gives a ledger with **no juul at all**
    /// — which is the honest empty case, not a shortcut around genesis.
    pub fn from_genesis(genesis: &Genesis) -> JuulLedger {
        JuulLedger {
            entries: genesis
                .distribution
                .iter()
                .map(|(principal, amount)| Entry::Mint {
                    principal: principal.clone(),
                    amount: *amount,
                    authority: MintAuthority::Genesis(genesis.id.clone()),
                })
                .collect(),
        }
    }

    /// Every mint in this ledger, in order. ★ The audit trail: total
    /// circulation can only have risen through these.
    pub fn mints(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.is_mint())
    }

    /// ★★★ **`balance ← balance − pawa`, admitted only if `balance ≥ pawa`.**
    ///
    /// Takes a [`PawaReading`], **not a number** — so a debit is always backed
    /// by a genuinely measured, genuinely committed run, and there is no way to
    /// invent one.
    ///
    /// ★ The principal charged is the reading's own: a run is charged to whoever
    /// made it, and a caller cannot redirect the bill.
    ///
    /// ★★ On refusal **nothing is appended**. The balance never clamps to zero:
    /// clamping would silently lose the overspend and report a debit that did
    /// not happen.
    pub fn charge(&mut self, reading: &PawaReading) -> Charge {
        let principal = reading.principal();
        let required = reading.pawa();
        let balance = self.balance_of(principal);

        if balance < required {
            return Charge::Insufficient { required, balance };
        }

        self.entries.push(Entry::Debit {
            principal: principal.to_string(),
            amount: required,
            operator: reading.operator().to_string(),
            sustain: reading.sustain().to_string(),
            at: reading.at(),
        });
        Charge::Charged { spent: required, balance: balance - required }
    }

    /// ★★★ **Ongoing issuance — the governed mint** (PAWA-8).
    ///
    /// `issued = rate × pawa_served`, minted to whoever **served** the work.
    ///
    /// ★★ Takes a [`PawaReading`], **not an amount** — the same discipline as
    /// [`JuulLedger::charge`], turned the other way round: a debit cannot exist
    /// without measured work behind it, and neither can a mint. There is no
    /// `mint(amount)` on this type, so **there is no issuance without work**.
    ///
    /// ★ `server` is host-supplied because *which machine did the compute* is a
    /// fact the core cannot know (ADR-0001), exactly as `at` is. It is
    /// deliberately **not** the reading's own principal: the caller pays, the
    /// server earns, and an issuance is not a refund. Splitting a reward across
    /// several servers is PAWA-9.
    ///
    /// ★★ Appends **nothing** when the governed rate (or the work) is zero —
    /// see [`Issued::Nothing`].
    pub fn issue(
        &mut self,
        issuance: &Issuance,
        reading: &PawaReading,
        server: &str,
        parameters: &Parameters,
    ) -> Issued {
        let pawa_served = reading.pawa();
        let rate = parameters.issuance_rate();
        let amount = issuance.issued_for(reading, parameters);
        if amount <= 0.0 {
            return Issued::Nothing { pawa_served, rate };
        }
        self.entries.push(Entry::Mint {
            principal: server.to_string(),
            amount,
            authority: MintAuthority::Issued(issuance.id().clone()),
        });
        Issued::Minted { amount, balance: self.balance_of(server), pawa_served, rate }
    }

    /// The fold, for one principal.
    pub fn balance_of(&self, principal: &str) -> f64 {
        self.entries.iter().map(|e| e.delta_for(principal)).sum()
    }

    /// ★★★ Move juul from one balance to another — **the only transfer there
    /// is**, and PAWA-5's royalty movement.
    ///
    /// Appends a single [`Entry::Transfer`], so it is **conserved by the shape
    /// of the entry**: total circulation is provably unchanged, because
    /// `circulation_delta` for that variant is `0.0` and no other code path
    /// creates one.
    ///
    /// ★★ **Internal, and there is no rail.** This moves juul between two
    /// balances in this ledger and nothing else. There is no cash-out, no
    /// external counterparty, no redemption — juul is not convertible into
    /// anything, which is what makes an internal reallocation structurally
    /// distinguishable from a real payment (ADR-0001 D5, PAWA-13).
    ///
    /// Refuses if `from` cannot cover it, leaving the ledger untouched — the
    /// same honest-refusal shape as [`JuulLedger::charge`]. A self-transfer is
    /// refused too: it is not a movement, and recording one would put a
    /// meaningless entry in an append-only log.
    pub fn transfer(&mut self, from: &str, to: &str, amount: f64, reason: &str) -> Charge {
        let balance = self.balance_of(from);
        if from == to || amount < 0.0 {
            return Charge::Insufficient { required: amount, balance };
        }
        if balance < amount {
            return Charge::Insufficient { required: amount, balance };
        }
        self.entries.push(Entry::Transfer {
            from: from.to_string(),
            to: to.to_string(),
            amount,
            reason: reason.to_string(),
        });
        Charge::Charged { spent: amount, balance: balance - amount }
    }

    /// ★★ Recompute every balance from the entries alone.
    ///
    /// There is no stored balance to compare against — the fold **is** the
    /// balance — so this is the `rebuild_state() == get_state()` property made
    /// trivially true by construction rather than kept true by discipline. It
    /// exists so the claim can be asserted, and so a host can rebuild from its
    /// own durable copy of the entries.
    pub fn rebuild(&self) -> BTreeMap<&str, f64> {
        let mut out: BTreeMap<&str, f64> = BTreeMap::new();
        for e in &self.entries {
            for p in e.touches() {
                *out.entry(p).or_insert(0.0) += e.delta_for(p);
            }
        }
        out
    }

    /// The append-only record.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Load a host's durable copy. ★ The same shape `NonceLedger::with_spent`
    /// uses: the core holds the decision, the host holds the durability.
    pub fn with_entries<I: IntoIterator<Item = Entry>>(entries: I) -> JuulLedger {
        JuulLedger { entries: entries.into_iter().collect() }
    }

    /// ★ Total juul in circulation across every principal.
    ///
    /// A **charge lowers this**, because a cost debit has no counterparty —
    /// spending removes juul from circulation rather than moving it to someone
    /// else. That is a deliberate posture and PAWA-6's treasury is where a
    /// destination would be introduced, if one ever is.
    pub fn total_in_circulation(&self) -> f64 {
        self.entries.iter().map(Entry::circulation_delta).sum()
    }

    /// `Σ` every mint — the whole of the juul ever created here.
    ///
    /// ★★ With PAWA-8 the accounting is exactly:
    ///
    /// ```text
    /// circulation = Σ(genesis mints) + Σ(issuance mints) − Σ(costs debited)
    /// ```
    ///
    /// with transfers contributing **zero**. Issuance is the **second, and only
    /// other, circulation-raiser**; `minted_total() - issued_total()` is the
    /// genesis half.
    pub fn minted_total(&self) -> f64 {
        self.entries.iter().filter(|e| e.is_mint()).map(Entry::circulation_delta).sum()
    }

    /// `Σ` the mints that ongoing issuance created.
    pub fn issued_total(&self) -> f64 {
        self.entries
            .iter()
            .filter_map(|e| match e {
                Entry::Mint { amount, authority, .. } if !authority.is_genesis() => Some(*amount),
                _ => None,
            })
            .sum()
    }

    /// Every debit charged against one principal, in order.
    pub fn debits_of<'a>(&'a self, principal: &'a str) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |e| matches!(e, Entry::Debit { principal: p, .. } if p == principal))
    }
}

/// Whether a pawa economy is in force for one call, and against which ledger.
///
/// ★★ **`Unmetered` is the default for every caller written before PAWA-3**,
/// and it is named for the same reason [`crate::operator::Authorization::Unchecked`]
/// and `EffectClass::Unchecked` are: *a bypass that reads as ordinary is the
/// problem; one that has to be spelled out is not.* An absent ledger makes the
/// affordability conjunct **vacuous** — it does **not** fail closed to broke,
/// because a host that has not opted into an economy has not said everyone has
/// zero juul, it has said there is no economy to price against.
///
/// ★ The same opt-in shape `Enforcement::enabled` uses for the invariant gate.
#[derive(Debug)]
pub enum Affordability<'a> {
    /// No economy in force. The clause is vacuous and behaviour is byte-identical
    /// to before PAWA-3.
    Unmetered,
    /// Price the candidate against this ledger, and charge the committed reading
    /// to it.
    ///
    /// `at` is host-supplied because the core has no clock (ADR-0001), exactly as
    /// `EffectClass::Live`'s `now` is.
    Metered {
        ledger: &'a mut JuulLedger,
        /// ★★ The **governed** parameters in force (PAWA-11). Passed rather
        /// than read from a constant, so the price the gate refuses on is the
        /// same one the ledger is charged.
        parameters: &'a crate::governance::Parameters,
        principal: &'a str,
        sustain: &'a str,
        at: u64,
    },
}

impl Affordability<'_> {
    /// Whether an economy is in force at all.
    pub fn metered(&self) -> bool {
        matches!(self, Affordability::Metered { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::Parameters;
    use crate::operator::{execute, Enforcement, Registry};
    use crate::pawa::meter;
    use serde_json::{json, Map, Value};

    fn registry() -> Registry {
        Registry::default()
    }

    fn state() -> Value {
        json!({"finances": {
            "liquid": {"balance": 500.0},
            "pockets": {"food": {"allocated": 100.0, "spent": 0.0, "limit": 0.0}},
            "income": {"monthly_total": 0.0, "sources": []}
        }})
    }

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    /// A real, committed run, metered — the only way to get a `PawaReading`.
    fn reading_for(principal: &str) -> PawaReading {
        let reg = registry();
        let e = Enforcement::default();
        let x = execute(
            &reg,
            &reg.names(),
            &e,
            &state(),
            "budget.record_income",
            &params(&[("amount", json!(100.0)), ("source", json!("salary"))]),
        );
        meter(&x, reg.get("budget.record_income").unwrap(), &e, &Parameters::genesis(), "household", principal, 1_000)
            .expect("it committed")
    }

    fn funded(principal: &str, juul: f64) -> JuulLedger {
        JuulLedger::from_genesis(&Genesis::declared("g0", &[(principal, juul)]).unwrap())
    }

    // ── PAWA-3: the out-of-pawa gate clause ──────────────────────────────────

    use crate::operator::execute_afforded;
    use crate::pawa::candidate_pawa;
    use crate::approval::{EffectClass, NonceLedger};
    use crate::operator::Authorization;

    fn income_params() -> Map<String, Value> {
        params(&[("amount", json!(100.0)), ("source", json!("salary"))])
    }

    /// Run `budget.record_income` through the real gate under an economy.
    fn afforded(l: &mut JuulLedger, principal: &str) -> crate::operator::Execution {
        let reg = Registry::default();
        let names = reg.names();
        let e = Enforcement::default();
        let mut nonces = NonceLedger::new();
        let params_gov = Parameters::genesis();
        let mut aff = Affordability::Metered {
            ledger: l,
            parameters: &params_gov,
            principal,
            sustain: "household",
            at: 1_000,
        };
        execute_afforded(
            &reg,
            &names,
            &e,
            &state(),
            "budget.record_income",
            &income_params(),
            &Authorization::Unchecked,
            &EffectClass::Unchecked,
            &mut nonces,
            &mut aff,
        )
    }

    #[test]
    fn an_affordable_run_commits_and_is_charged_exactly_once() {
        let r = reading_for("bonnie");
        let mut l = funded("bonnie", 100.0);
        let x = afforded(&mut l, "bonnie");
        assert!(x.committed(), "{:?}", x.result.reason);
        assert_eq!(l.debits_of("bonnie").count(), 1, "once, not twice");
        assert_eq!(l.balance_of("bonnie"), 100.0 - r.pawa());
        assert_eq!(l.rebuild()["bonnie"], l.balance_of("bonnie"), "the fold still holds");
    }

    #[test]
    fn an_unaffordable_run_is_refused_and_charges_nothing() {
        // ★★★ The whole point: the gate stops the work BEFORE it costs anything.
        let mut l = funded("bonnie", 1.0);
        let before = l.clone();
        let x = afforded(&mut l, "bonnie");

        assert!(!x.committed(), "refused on cost");
        assert_eq!(x.result.constraint_violated.as_deref(), Some("insufficient_pawa"));
        assert_eq!(l, before, "balance byte-identical — no juul spent on a refused run");
        assert_eq!(x.state, state(), "and state untouched");
        assert!(x.mutations.is_empty() && x.events.is_empty());
    }

    #[test]
    fn it_prices_against_the_real_cost_not_the_declared_estimate() {
        // ★★★ The reference's trap: `budget.record_income` declares
        // `pawa_cost = 0`, so an estimate-priced clause would ALWAYS admit.
        let reg = Registry::default();
        let declared = reg.get("budget.record_income").unwrap().pawa_cost;
        assert_eq!(declared, 0, "the declared estimate really is zero");

        let real = reading_for("bonnie").pawa();
        assert!(real > 0.0, "and the real cost really is not");

        // A balance that covers the ESTIMATE but not the MEASUREMENT.
        let mut l = funded("bonnie", real / 2.0);
        let x = afforded(&mut l, "bonnie");
        assert!(!x.committed(), "refused — so it read the measurement");
        assert_eq!(l.debits_of("bonnie").count(), 0);
    }

    #[test]
    fn the_candidate_price_equals_the_committed_readings_price() {
        // ★★ Pre-commit affordability and the post-commit debit are the SAME
        // number, because both go through `compute_units` + `storage_bytes`.
        let reg = Registry::default();
        let e = Enforcement::default();
        let x = execute(&reg, &reg.names(), &e, &state(), "budget.record_income", &income_params());
        let candidate =
            candidate_pawa(&x.mutations, &x.events, reg.get("budget.record_income").unwrap(), &e, &Parameters::genesis());
        let charged = meter(&x, reg.get("budget.record_income").unwrap(), &e, &Parameters::genesis(), "household", "bonnie", 1).unwrap();
        assert_eq!(candidate, charged.pawa());
    }

    #[test]
    fn an_unmetered_call_behaves_exactly_as_before() {
        // ★ Additive and opt-in-safe: no ledger means the clause is VACUOUS,
        // not failing closed to broke.
        let reg = Registry::default();
        let names = reg.names();
        let e = Enforcement::default();
        let plain = execute(&reg, &names, &e, &state(), "budget.record_income", &income_params());

        let mut nonces = NonceLedger::new();
        let mut aff = Affordability::Unmetered;
        let via = execute_afforded(
            &reg, &names, &e, &state(), "budget.record_income", &income_params(),
            &Authorization::Unchecked, &EffectClass::Unchecked, &mut nonces, &mut aff,
        );
        assert!(via.committed(), "an unfunded, unmetered caller still runs");
        assert_eq!(via.state, plain.state, "byte-identical to the pre-PAWA-3 path");
        assert_eq!(via.mutations, plain.mutations);
        assert!(!aff.metered());
    }

    #[test]
    fn a_run_refused_by_an_earlier_conjunct_never_reaches_the_charge() {
        // ★ The clause is the LAST one before commit, so a guard refusal costs
        // nothing either — and `meter` would refuse to read it regardless.
        let reg = Registry::default();
        let names = reg.names();
        let e = Enforcement::default();
        let mut l = funded("bonnie", 1_000.0);
        let before = l.clone();
        let mut nonces = NonceLedger::new();
        let params_gov = Parameters::genesis();
        let mut aff = Affordability::Metered {
            ledger: &mut l, parameters: &params_gov, principal: "bonnie", sustain: "household", at: 1,
        };
        // `params.amount > 0` is a declared guard on record_income.
        let x = execute_afforded(
            &reg, &names, &e, &state(), "budget.record_income",
            &params(&[("amount", json!(-5.0)), ("source", json!("s"))]),
            &Authorization::Unchecked, &EffectClass::Unchecked, &mut nonces, &mut aff,
        );
        assert!(!x.committed());
        assert_eq!(l, before, "a guard refusal charges nothing either");
    }

    // ── ★★★ PAWA-7 genesis + MYC-6: minting is declared and visible ──────────

    fn g_two() -> Genesis {
        Genesis::declared("genesis-2026", &[("bonnie", 100.0), ("cira", 40.0)]).unwrap()
    }

    #[test]
    fn a_mint_is_its_own_variant_and_names_the_declaration_that_authorised_it() {
        // ★★★ MYC-6: a mint is VISIBLE as a mint, never a silent credit.
        let g = g_two();
        let l = JuulLedger::from_genesis(&g);
        assert_eq!(l.mints().count(), 2);
        for m in l.mints() {
            match m {
                Entry::Mint { authority, .. } => {
                assert_eq!(authority, &MintAuthority::Genesis(g.id().clone()))
            }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn circulation_is_raised_only_by_a_mint() {
        // ★★★ The accounting: circulation = Σ(mints) − Σ(costs debited),
        // transfers contributing nothing.
        let g = g_two();
        let mut l = JuulLedger::from_genesis(&g);
        assert_eq!(l.total_in_circulation(), g.total());

        // A transfer raises nothing.
        assert!(l.transfer("bonnie", "cira", 25.0, "gift").charged());
        assert_eq!(l.total_in_circulation(), g.total());

        // A debit lowers it, and nothing anywhere raises it again.
        let r = reading_for("bonnie");
        assert!(l.charge(&r).charged());
        assert_eq!(l.total_in_circulation(), g.total() - r.pawa());

        let minted: f64 = l.mints().map(|m| m.circulation_delta()).sum();
        assert_eq!(minted, g.total(), "every juul in existence came from a mint");
    }

    #[test]
    fn an_empty_ledger_has_no_juul_because_it_has_no_genesis() {
        // ★ The honest empty case: no declaration, no money — not a shortcut
        // around genesis.
        let l = JuulLedger::new();
        assert_eq!(l.total_in_circulation(), 0.0);
        assert_eq!(l.mints().count(), 0);
    }

    #[test]
    fn genesis_is_one_time_because_it_is_a_constructor_not_a_method() {
        // ★★★ There is no `mint` method: you cannot mint INTO an existing
        // ledger. A second genesis makes a second ledger, not more juul in this
        // one — so the one-time-ness is a property of the shape.
        let g = g_two();
        let l = JuulLedger::from_genesis(&g);
        let before = l.total_in_circulation();

        let other = Genesis::declared("some-other", &[("mallory", 1_000_000.0)]).unwrap();
        let _elsewhere = JuulLedger::from_genesis(&other);
        assert_eq!(l.total_in_circulation(), before, "another declaration is another ledger");
    }

    #[test]
    fn a_genesis_refuses_a_negative_allocation() {
        // ★ A genesis that took juul from someone would not be a genesis.
        assert!(Genesis::declared("g", &[("bonnie", -1.0)]).is_none());
        assert!(Genesis::declared("", &[("bonnie", 1.0)]).is_none(), "and it must be named");
    }

    // ── ★★ auditability: declared once, checkable forever ───────────────────

    #[test]
    fn a_ledgers_money_supply_is_exactly_its_declared_genesis() {
        let g = g_two();
        let mut l = JuulLedger::from_genesis(&g);
        // Trading and spending change balances, never the declaration.
        l.transfer("bonnie", "cira", 30.0, "gift");
        l.charge(&reading_for("cira"));

        let audit = g.audit(&l);
        assert!(audit.clean(), "{}", audit.describe());
        assert_eq!(audit, Audit { foreign: vec![], mismatched: vec![], undeclared: vec![], issued: vec![] });
    }

    #[test]
    fn an_audit_names_a_mint_that_the_declaration_does_not_explain() {
        // ★★ A ledger reloaded from a host's durable copy could carry a mint
        // the declaration never made. The audit NAMES it rather than letting a
        // total quietly absorb it.
        let g = g_two();
        let mut entries = JuulLedger::from_genesis(&g).entries().to_vec();
        entries.push(Entry::Mint {
            principal: "mallory".into(),
            amount: 500.0,
            authority: MintAuthority::Genesis(g.id().clone()),
        });
        let audit = g.audit(&JuulLedger::with_entries(entries));
        assert!(!audit.clean());
        assert_eq!(audit.undeclared, vec!["mallory".to_string()]);
    }

    #[test]
    fn an_audit_names_a_mint_from_a_foreign_declaration() {
        let g = g_two();
        let other = Genesis::declared("not-ours", &[("mallory", 1.0)]).unwrap();
        let mut entries = JuulLedger::from_genesis(&g).entries().to_vec();
        entries.extend(JuulLedger::from_genesis(&other).entries().to_vec());
        let audit = g.audit(&JuulLedger::with_entries(entries));
        assert!(!audit.clean());
        assert_eq!(audit.foreign, vec!["not-ours".to_string()]);
    }

    #[test]
    fn a_balance_is_fully_explained_by_genesis_plus_the_folded_entries() {
        // ★★★ The fold over genesis: every balance equals its allocation plus
        // everything that happened to it, with nothing unexplained.
        let g = g_two();
        let mut l = JuulLedger::from_genesis(&g);
        l.transfer("bonnie", "cira", 30.0, "gift");
        let r = reading_for("cira");
        l.charge(&r);

        for who in ["bonnie", "cira"] {
            let since_genesis: f64 = l
                .entries()
                .iter()
                .filter(|e| !e.is_mint())
                .map(|e| e.delta_for(who))
                .sum();
            assert_eq!(
                l.balance_of(who),
                g.allocation_for(who) + since_genesis,
                "{who}'s balance is genesis plus the fold, exactly"
            );
        }
        assert_eq!(l.rebuild()["bonnie"], l.balance_of("bonnie"), "and rebuild still agrees");
    }

    // ── a debit is backed by a measurement ───────────────────────────────────

    #[test]
    fn a_charge_is_always_backed_by_a_real_metered_run() {
        // ★★★ `charge` takes a `PawaReading`, and `PawaReading` has no public
        // constructor — the only source is `pawa::meter` over a COMMITTED
        // execution. There is no `debit(amount)` anywhere in this module.
        let r = reading_for("bonnie");
        let mut l = funded("bonnie", 100.0);
        assert!(l.charge(&r).charged());
        let debit = l.debits_of("bonnie").next().cloned().expect("one debit");
        match debit {
            Entry::Debit { amount, operator, sustain, .. } => {
                assert_eq!(amount, r.pawa());
                assert_eq!(operator, "budget.record_income");
                assert_eq!(sustain, "household");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_run_is_charged_to_whoever_made_it() {
        // ★ The principal comes off the reading; a caller cannot redirect it.
        let mut l = JuulLedger::from_genesis(
            &Genesis::declared("g0", &[("bonnie", 100.0), ("cira", 100.0)]).unwrap(),
        );
        l.charge(&reading_for("bonnie"));
        assert!(l.balance_of("bonnie") < 100.0);
        assert_eq!(l.balance_of("cira"), 100.0, "somebody else's bill is not theirs");
    }

    // ── the affordability rule ───────────────────────────────────────────────

    #[test]
    fn a_charge_is_admitted_only_if_the_balance_covers_it() {
        let r = reading_for("bonnie");
        let mut rich = funded("bonnie", 100.0);
        match rich.charge(&r) {
            Charge::Charged { spent, balance } => {
                assert_eq!(spent, r.pawa());
                assert_eq!(balance, 100.0 - r.pawa());
            }
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn an_unaffordable_charge_is_refused_and_the_balance_is_byte_identical() {
        // ★★ Honest refusal, not an error — and nothing is appended.
        let r = reading_for("bonnie");
        let mut poor = funded("bonnie", 1.0);
        let before = poor.clone();

        let out = poor.charge(&r);
        assert!(!out.charged(), "{}", out.describe());
        assert_eq!(out.shortfall(), Some(r.pawa() - 1.0));
        assert_eq!(poor, before, "a refused charge appended nothing");
        assert_eq!(poor.balance_of("bonnie"), 1.0);
    }

    #[test]
    fn a_refused_charge_never_clamps_the_balance_to_zero() {
        // ★ Clamping would silently lose the overspend and report a debit that
        // did not happen.
        let r = reading_for("bonnie");
        let mut l = funded("bonnie", 2.0);
        l.charge(&r);
        assert_eq!(l.balance_of("bonnie"), 2.0, "untouched, not zeroed");
        assert_eq!(l.debits_of("bonnie").count(), 0);
    }

    #[test]
    fn a_balance_exactly_equal_to_the_cost_is_affordable() {
        // `balance ≥ pawa`, not `>`.
        let r = reading_for("bonnie");
        let mut l = funded("bonnie", r.pawa());
        assert!(l.charge(&r).charged());
        assert_eq!(l.balance_of("bonnie"), 0.0);
    }

    #[test]
    fn an_unfunded_principal_starts_at_zero_and_cannot_spend() {
        let mut l = JuulLedger::new();
        assert_eq!(l.balance_of("stranger"), 0.0, "a sum over no entries");
        assert!(!l.charge(&reading_for("stranger")).charged());
    }

    // ── the balance is a fold ────────────────────────────────────────────────

    #[test]
    fn the_balance_is_the_fold_of_the_entries_and_rebuilds_identically() {
        // ★★ No stored balance exists to drift — the fold IS the balance.
        let mut l = funded("bonnie", 100.0);
        for _ in 0..3 {
            l.charge(&reading_for("bonnie"));
        }
        let rebuilt = l.rebuild();
        assert_eq!(rebuilt["bonnie"], l.balance_of("bonnie"));
        assert_eq!(l.entries().len(), 4, "one credit and three debits, append-only");
    }

    #[test]
    fn a_hosts_durable_entries_rebuild_the_same_balance() {
        let mut l = funded("bonnie", 100.0);
        l.charge(&reading_for("bonnie"));
        let reloaded = JuulLedger::with_entries(l.entries().to_vec());
        assert_eq!(reloaded, l);
        assert_eq!(reloaded.balance_of("bonnie"), l.balance_of("bonnie"));
    }

    // ── debit is not mint, and there is no transfer ──────────────────────────

    #[test]
    fn a_debit_reduces_circulation_and_creates_nothing() {
        // ★ A cost debit has no counterparty: spending removes juul from
        // circulation rather than moving it to someone else.
        let mut l = funded("bonnie", 100.0);
        assert_eq!(l.total_in_circulation(), 100.0);
        let r = reading_for("bonnie");
        l.charge(&r);
        assert_eq!(l.total_in_circulation(), 100.0 - r.pawa());
    }

    #[test]
    fn a_credit_and_a_debit_are_different_entries_not_a_signed_number() {
        // ★ So "where did this juul come from" stays answerable.
        let mut l = funded("bonnie", 100.0);
        l.charge(&reading_for("bonnie"));
        assert!(matches!(l.entries()[0], Entry::Mint { .. }));
        assert!(matches!(l.entries()[1], Entry::Debit { .. }));
        assert_eq!(l.debits_of("bonnie").count(), 1, "the credit is not a debit");
    }

    #[test]
    fn charging_one_principal_leaves_every_other_balance_untouched() {
        // ★★★ There is no transfer in this module: nobody is credited by a
        // charge, so no method here can move juul between principals.
        let mut l = JuulLedger::from_genesis(
            &Genesis::declared("g0", &[("bonnie", 100.0), ("cira", 50.0)]).unwrap(),
        );
        let before_total = l.total_in_circulation();
        let r = reading_for("bonnie");
        l.charge(&r);
        assert_eq!(l.balance_of("cira"), 50.0);
        assert_eq!(l.total_in_circulation(), before_total - r.pawa(), "not moved — spent");
    }
}
