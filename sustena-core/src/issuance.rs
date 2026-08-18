//! Ongoing issuance — the **governed mint** (PAWA-8; the Pawa paper §6.2).
//!
//! ★★★ **Two minting paths, and they stay distinct.**
//!
//! [`crate::juul::Genesis`] (PAWA-7) is the **one-time seed**: a declared
//! distribution, opened by a *constructor*, with no method that mints into a
//! ledger that already exists. Its answer to *who may mint* is structural —
//! **nobody, afterwards**.
//!
//! Issuance is the **other** path: **ongoing**, and safe only because its rate
//! is a **governed parameter** (PAWA-11). It is deliberately *not* a second
//! genesis — a genesis is a value you read once and audit forever, while
//! issuance is a rule that keeps applying, so it needs the revision machinery a
//! one-time seed does not.
//!
//! ★★ Both paths produce the **same visible circulation-raiser**,
//! [`crate::juul::Entry::Mint`], because MYC-6's point is that *a mint must be
//! visible as a mint*. What differs is the **authority** each names —
//! [`crate::juul::MintAuthority`] — so *where did this juul come from* stays
//! answerable, and answers differently for the two.
//!
//! ## ★★ Denominated in work
//!
//! `issued = rate × pawa_served`. New juul enters by **rewarding whoever served
//! the pawa** — did the compute — and the amount is a function of a real
//! [`PawaReading`], never a number a caller chose.
//! [`JuulLedger::issue`](crate::juul::JuulLedger::issue) takes the reading
//! itself, the same discipline as
//! [`JuulLedger::charge`](crate::juul::JuulLedger::charge): **there is no
//! `mint(amount)` anywhere, so there is no issuance without work behind it.**
//!
//! ★ The caller pays and the **server** earns, and they are different
//! principals: the reading's own principal is *who ran the operator and is
//! charged for it*, while the server is *whose machine did the work*. So an
//! issuance is not a refund.
//!
//! ★★ Which host served a run is a fact the core **cannot know** — it has no
//! I/O by ADR-0001 — so the serving principal is host-supplied, exactly as
//! `at` is. **On a single host there is one server**, and it earns the whole
//! issuance; distributing a reward across several is PAWA-9's row, not this
//! one, and nothing here pretends otherwise.
//!
//! ## ★★ The rate is governed; the schedule TYPE is declared
//!
//! PAWA-11's `ParameterSpec` bounds **one scalar between a min and a max**.
//! The issuance **rate** fits that exactly, so it is a declared, governed,
//! revisable-only-through-an-Enzyme parameter
//! ([`crate::governance::ISSUANCE_RATE`]) — and changing it is the *only* way
//! the rate moves.
//!
//! ★ The schedule **type** does not fit, and [`Schedule`] has **one variant**
//! for that reason rather than three with two unimplemented. See its own docs.
//!
//! ## ★★★ Internal ≠ a real coin — restated here, where the word invites it
//!
//! Issuance mints the **internal accounting unit** inside one host's ledger. It
//! creates **no real value and no redeemable claim**: juul is still not
//! convertible into anything, there is still no cash-out and **no rail**, and
//! issuing a real, public, transferable token remains a *distinct, later,
//! human-authorised act* (ADR-0001 D5, PAWA-13). A slice about creating money
//! is exactly where that line has to be said again, not assumed.

use crate::governance::Parameters;
use crate::pawa::PawaReading;

/// The identity of a declared issuance.
///
/// ★★ **No public constructor**, exactly like
/// [`GenesisId`](crate::juul::GenesisId): the only way to obtain one is
/// [`Issuance::declared`], so a mint cannot name an issuance that was never
/// declared. The same mechanism, applied to the *ongoing* authority to create
/// juul rather than the one-time one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IssuanceId(String);

impl IssuanceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IssuanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How much juul a served pawa earns.
///
/// ★★ **One variant, deliberately.** The row names three schedule shapes —
/// fixed, decaying, target-rate — and only the fixed one is expressible with
/// what this core has:
///
/// - a **decaying** schedule needs an epoch or an elapsed time to decay
///   *against*, and this core has **no clock** (ADR-0001);
/// - a **target-rate** schedule needs the rate to be *derived* from current
///   circulation against an inflation target, so the governed value would no
///   longer be the rate itself — it would be a target, and the rate a function
///   of ledger state that `ParameterSpec` (a static min/max on one path) cannot
///   bound.
///
/// ★ So the honest shape is one variant plus a named extension point, not three
/// variants where two would be **declared but inert** — which in this codebase
/// reads as built, and is exactly the defect being avoided.
#[derive(Debug, Clone, PartialEq)]
pub enum Schedule {
    /// `issued = rate × pawa_served`, at the **governed** rate.
    ///
    /// ★ Carries **no rate of its own**. The rate lives only in
    /// [`Parameters`], so there is nowhere else for it to be — the same
    /// one-truth discipline PAWA-11 enforced by deleting `compute_pawa`.
    Fixed,
}

/// A declared, ongoing issuance.
///
/// ★ A **value**, like a [`Genesis`](crate::juul::Genesis) — anyone can read it
/// and check a ledger against it. What it does *not* carry is the rate: that is
/// governed, so the declaration says *how* juul is issued and governance says
/// *how much*.
#[derive(Debug, Clone, PartialEq)]
pub struct Issuance {
    id: IssuanceId,
    schedule: Schedule,
}

impl Issuance {
    pub fn declared(id: &str, schedule: Schedule) -> Option<Issuance> {
        if id.trim().is_empty() {
            return None;
        }
        Some(Issuance { id: IssuanceId(id.to_string()), schedule })
    }

    pub fn id(&self) -> &IssuanceId {
        &self.id
    }

    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// What serving this reading earns, under the **governed** parameters.
    ///
    /// ★★ A pure function of the measured work and the governed rate — of
    /// nothing else. It reads the rate from the [`Parameters`] it is handed, so
    /// a caller cannot price an issuance against anything but the value in
    /// force.
    pub fn issued_for(&self, reading: &PawaReading, parameters: &Parameters) -> f64 {
        match self.schedule {
            Schedule::Fixed => parameters.issuance_rate() * reading.pawa(),
        }
    }
}

/// What an issuance did.
///
/// ★★ `Nothing` is a **normal outcome, not an error** — the same shape as
/// [`Charge::Insufficient`](crate::juul::Charge::Insufficient). A governed rate
/// of zero is a legitimate policy (issuance switched off), and work that cost
/// nothing earns nothing; in both cases **no entry is appended**, because a
/// zero mint would be a line in an append-only log that raised circulation by
/// nothing and told a later reader nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Issued {
    /// Minted to the server, and here is its balance afterwards.
    Minted { amount: f64, balance: f64, pawa_served: f64, rate: f64 },
    /// ★ Nothing was created and **nothing was appended**.
    Nothing { pawa_served: f64, rate: f64 },
}

impl Issued {
    pub fn minted(&self) -> bool {
        matches!(self, Issued::Minted { .. })
    }

    /// The juul created. `0.0` when nothing was.
    pub fn amount(&self) -> f64 {
        match self {
            Issued::Minted { amount, .. } => *amount,
            Issued::Nothing { .. } => 0.0,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Issued::Minted { amount, pawa_served, rate, .. } => {
                format!("issued {amount} juul for {pawa_served} pawa served, at rate {rate}")
            }
            Issued::Nothing { pawa_served, rate } => {
                format!("no juul issued for {pawa_served} pawa served: the rate is {rate}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::Parameters;
    use crate::juul::{Entry, Genesis, JuulLedger, MintAuthority};
    use crate::operator::{execute, Enforcement, Registry};
    use crate::pawa::meter;
    use serde_json::{json, Map};

    fn reading(principal: &str) -> PawaReading {
        let reg = Registry::default();
        let e = Enforcement::default();
        let state = json!({"finances": {
            "liquid": {"balance": 500.0},
            "pockets": {"food": {"allocated": 100.0, "spent": 0.0, "limit": 0.0}},
            "income": {"monthly_total": 0.0, "sources": []}
        }});
        let mut p = Map::new();
        p.insert("amount".into(), json!(100.0));
        p.insert("source".into(), json!("salary"));
        let x = execute(&reg, &reg.names(), &e, &state, "budget.record_income", &p);
        assert!(x.committed(), "the fixture must commit: {:?}", x.result.reason);
        meter(
            &x,
            reg.get("budget.record_income").unwrap(),
            &e,
            &Parameters::genesis(),
            "household",
            principal,
            1_000,
        )
        .expect("a committed run meters")
    }

    fn governed(rate: f64) -> Parameters {
        Parameters::read(&json!({"parameters": {"issuance_rate": rate}}))
    }

    fn issuance() -> Issuance {
        Issuance::declared("iss-1", Schedule::Fixed).expect("a named issuance")
    }

    fn ledger() -> JuulLedger {
        JuulLedger::from_genesis(&Genesis::declared("g-1", &[("bonnie", 500.0)]).unwrap())
    }

    // ── denominated in work ──────────────────────────────────────────────────

    #[test]
    fn the_issued_amount_is_the_rate_times_the_pawa_served() {
        let r = reading("bonnie");
        assert!(r.pawa() > 0.0, "the fixture did real work");
        assert_eq!(issuance().issued_for(&r, &governed(2.0)), 2.0 * r.pawa());
    }

    #[test]
    fn an_issuance_mints_to_the_server_not_to_the_caller() {
        // ★ The caller PAYS and the server EARNS — an issuance is not a refund.
        let mut l = ledger();
        let out = l.issue(&issuance(), &reading("bonnie"), "host-0", &governed(1.5));
        assert!(out.minted());
        assert_eq!(l.balance_of("bonnie"), 500.0, "the caller's balance is untouched");
        assert_eq!(l.balance_of("host-0"), out.amount());
    }

    #[test]
    fn the_mint_names_the_issuance_that_authorised_it_not_a_genesis() {
        let mut l = ledger();
        l.issue(&issuance(), &reading("bonnie"), "host-0", &governed(1.0));
        let last = l.entries().last().unwrap();
        match last {
            Entry::Mint { authority: MintAuthority::Issued(id), .. } => {
                assert_eq!(id.as_str(), "iss-1")
            }
            other => panic!("expected an issuance mint, got {other:?}"),
        }
    }

    // ── the honest nothing ───────────────────────────────────────────────────

    #[test]
    fn a_governed_rate_of_zero_issues_nothing_and_appends_nothing() {
        // ★★ Issuance switched off is a legitimate policy, and it is the
        // GENESIS posture: an economy does not start inflating by default.
        let mut l = ledger();
        let before = l.entries().len();
        let out = l.issue(&issuance(), &reading("bonnie"), "host-0", &Parameters::genesis());
        assert!(!out.minted(), "{}", out.describe());
        assert_eq!(out.amount(), 0.0);
        assert_eq!(l.entries().len(), before, "no entry for a zero mint");
        assert_eq!(l.total_in_circulation(), 500.0);
    }

    #[test]
    fn there_is_no_mint_amount_so_there_is_no_issuance_without_work() {
        // ★★ Structural, not a check: `issue` takes a `PawaReading`, which has
        // no public constructor — the only source is metering a COMMITTED run.
        let mut l = ledger();
        let out = l.issue(&issuance(), &reading("bonnie"), "host-0", &governed(1.0));
        assert_eq!(out.amount(), reading("bonnie").pawa(), "the work is the amount");
    }

    // ── conservation ─────────────────────────────────────────────────────────

    #[test]
    fn issuance_is_the_second_and_only_other_circulation_raiser() {
        let g = Genesis::declared("g-1", &[("bonnie", 500.0)]).unwrap();
        let mut l = JuulLedger::from_genesis(&g);
        let r = reading("bonnie");

        l.issue(&issuance(), &r, "host-0", &governed(2.0));
        assert!(l.charge(&r).charged());
        assert!(l.transfer("bonnie", "host-0", 10.0, "royalty").charged());

        let issued = l.issued_total();
        let debited: f64 = l
            .entries()
            .iter()
            .filter_map(|e| match e {
                Entry::Debit { amount, .. } => Some(*amount),
                _ => None,
            })
            .sum();
        assert_eq!(
            l.total_in_circulation(),
            g.total() + issued - debited,
            "circulation = genesis + issuance − costs, transfers at zero"
        );
        assert_eq!(l.minted_total(), g.total() + issued);
    }

    #[test]
    fn an_empty_id_is_not_a_declaration() {
        assert!(Issuance::declared("  ", Schedule::Fixed).is_none());
    }
}
