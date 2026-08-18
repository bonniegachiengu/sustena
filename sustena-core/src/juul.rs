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
//! - **no mint** — nothing creates juul from nothing except
//!   [`JuulLedger::credit`], which exists so a starting balance can be
//!   *declared* and is deliberately named apart from a debit. Genesis proper is
//!   PAWA-7 and issuance PAWA-8.
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

use crate::pawa::PawaReading;

/// One append-only ledger line.
///
/// ★ `Debit` and `Credit` are deliberately **different variants**, not a signed
/// number: a debit is a *cost that was measured and spent*, a credit is a
/// *balance that was declared*. Collapsing them would make "where did this juul
/// come from" unanswerable.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// A declared starting balance. ★ Named apart from a debit on purpose —
    /// this is the only thing in the module that increases a balance, and
    /// genesis proper is PAWA-7's row, not this one.
    Credit { principal: String, amount: f64, reason: String },
    /// A metered cost, spent. ★ Carries the **reading** it was charged from, so
    /// every debit can be traced to the run that incurred it.
    Debit { principal: String, amount: f64, operator: String, sustain: String, at: u64 },
}

impl Entry {
    pub fn principal(&self) -> &str {
        match self {
            Entry::Credit { principal, .. } | Entry::Debit { principal, .. } => principal,
        }
    }

    /// The signed effect on a balance. Credits add, debits subtract.
    pub fn delta(&self) -> f64 {
        match self {
            Entry::Credit { amount, .. } => *amount,
            Entry::Debit { amount, .. } => -*amount,
        }
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

    /// Declare a starting balance.
    ///
    /// ★★ The **only** thing here that increases a balance, and it is named
    /// apart from a debit so the two can never be confused. It is not a mint in
    /// the economic sense and not a transfer: nothing is taken from anyone.
    /// Genesis (PAWA-7) and issuance (PAWA-8) are their own rows.
    pub fn credit(&mut self, principal: &str, amount: f64, reason: &str) {
        self.entries.push(Entry::Credit {
            principal: principal.to_string(),
            amount,
            reason: reason.to_string(),
        });
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

    /// The fold, for one principal.
    pub fn balance_of(&self, principal: &str) -> f64 {
        self.entries.iter().filter(|e| e.principal() == principal).map(Entry::delta).sum()
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
            *out.entry(e.principal()).or_insert(0.0) += e.delta();
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
        self.entries.iter().map(Entry::delta).sum()
    }

    /// Every debit charged against one principal, in order.
    pub fn debits_of<'a>(&'a self, principal: &'a str) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |e| e.principal() == principal && matches!(e, Entry::Debit { .. }))
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
        meter(&x, reg.get("budget.record_income").unwrap(), &e, "household", principal, 1_000)
            .expect("it committed")
    }

    fn funded(principal: &str, juul: f64) -> JuulLedger {
        let mut l = JuulLedger::new();
        l.credit(principal, juul, "declared opening balance");
        l
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
        let mut aff = Affordability::Metered {
            ledger: l,
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
        let candidate = candidate_pawa(&x.mutations, &x.events, reg.get("budget.record_income").unwrap(), &e);
        let charged = meter(&x, reg.get("budget.record_income").unwrap(), &e, "household", "bonnie", 1).unwrap();
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
        let mut aff = Affordability::Metered {
            ledger: &mut l, principal: "bonnie", sustain: "household", at: 1,
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
        let mut l = JuulLedger::new();
        l.credit("bonnie", 100.0, "opening");
        l.credit("cira", 100.0, "opening");
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
        assert!(matches!(l.entries()[0], Entry::Credit { .. }));
        assert!(matches!(l.entries()[1], Entry::Debit { .. }));
        assert_eq!(l.debits_of("bonnie").count(), 1, "the credit is not a debit");
    }

    #[test]
    fn charging_one_principal_leaves_every_other_balance_untouched() {
        // ★★★ There is no transfer in this module: nobody is credited by a
        // charge, so no method here can move juul between principals.
        let mut l = JuulLedger::new();
        l.credit("bonnie", 100.0, "opening");
        l.credit("cira", 50.0, "opening");
        let before_total = l.total_in_circulation();
        let r = reading_for("bonnie");
        l.charge(&r);
        assert_eq!(l.balance_of("cira"), 50.0);
        assert_eq!(l.total_in_circulation(), before_total - r.pawa(), "not moved — spent");
    }
}
