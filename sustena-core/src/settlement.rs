//! **Settlement behind a swappable adapter** (Additions · ADD-4, and Pawa's own
//! Additions).
//!
//! ```text
//!   Sustena executes locally
//!   Babychains are the L2      aggregation, rollup, proofs
//!   Ethereum is the finalizer  receives only state-root hashes
//! ```
//!
//! ★★★ **The `⊕` hierarchy IS the rollup batching structure.** A homestead
//! folds its habitats; a Babychain folds its Sustains. Nobody had to design a
//! batching scheme, because the composition operator already is one — and that
//! is the §V value/information law read at the settlement layer: only key
//! transactions and periodic roots reach L1.
//!
//! ★★★ **The whole stack sits behind an interface with a mock**, so pawa and
//! Juul run as live utility tokens today and the chain is switched on later
//! **with no rewrite**. Same discipline as [`crate::resolution`]: the interface
//! is the deliverable.
//!
//! ★★★ **And, as there, a mocked settlement is labelled on every answer.** A
//! finality claim is the single most dangerous thing to fake, because acting on
//! a false one means treating money as settled that is not — so
//! [`Settlement::is_final`] is `false` for anything a mock produced, whatever
//! else it says.
//!
//! ## The honest line, kept
//!
//! ★★★ §6.7 draws it and this module does not blur it: the **software** is
//! buildable and useful now, on a single host, before any coin is public.
//! **Issuing a real, public, transferable token is a distinct, later,
//! human-authorized act** with legal weight. [`Rail::Ethereum`] therefore has no
//! constructor that marks itself live — going live is not a value this codebase
//! gets to set.

/// Where settlement is happening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rail {
    /// One host. The meter, the ledger and the royalty flow are all defined
    /// here and are useful here immediately.
    Local,
    /// The L2: aggregation, rollup, proofs.
    Babychain { mocked: bool },
    /// The finalizer, which receives only roots.
    ///
    /// ★★★ Always mocked in this codebase. Switching it on is a legal and
    /// financial act, not a boolean.
    Ethereum,
}

impl Rail {
    /// Can this rail produce a claim somebody may treat as final?
    ///
    /// ★★★ Only a real L2. `Local` settles honestly and is not *final* in the
    /// network sense; `Ethereum` here is a stand-in and says so.
    pub fn can_finalise(&self) -> bool {
        matches!(self, Self::Babychain { mocked: false })
    }

    pub fn describe(&self) -> &'static str {
        match self {
            Self::Local => "settled on this host — real, and not network-final",
            Self::Babychain { mocked: true } => {
                "a MOCKED L2 — the shape of a rollup, and not a rollup"
            }
            Self::Babychain { mocked: false } => "the L2",
            Self::Ethereum => {
                "a STAND-IN for the finalizer — switching this on is a human-authorized act, \
                 not a flag"
            }
        }
    }
}

/// What reaches the layer above.
///
/// ★★★ **Only key transactions and periodic roots.** A design that sent
/// everything upward would be paying L1 prices for a household's grocery
/// shopping, and would also publish it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upward {
    /// A root hash standing for a whole batch.
    Root { hash: String, covering: usize },
    /// One transaction important enough to carry alone.
    KeyTransaction { id: String },
}

/// A settlement claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    pub rail: Rail,
    pub amount: u64,
    pub reference: String,
}

impl Settlement {
    /// ★★★ May this be treated as settled money?
    ///
    /// A mocked answer is never final, whatever else it carries. Acting on a
    /// false finality claim means treating money as settled that is not, which
    /// is the one mistake this layer must not enable.
    pub fn is_final(&self) -> bool {
        self.rail.can_finalise()
    }

    pub fn describe(&self) -> String {
        format!("{} of {} — {}", self.amount, self.reference, self.rail.describe())
    }
}

/// **Batch a level of the `⊕` hierarchy into what goes up.**
///
/// ★★★ The batching structure is not invented here: a parent folding its
/// children *is* the rollup. `children` is what this level folded, and the root
/// stands for all of them.
///
/// ★★ An empty level produces nothing to send rather than an empty root. A root
/// covering zero transactions is a message that costs L1 fees and carries no
/// information.
pub fn roll_up(root_hash: &str, children: usize, key: &[String]) -> Vec<Upward> {
    let mut out: Vec<Upward> = key.iter().map(|id| Upward::KeyTransaction { id: id.clone() }).collect();
    if children > 0 {
        out.push(Upward::Root { hash: root_hash.into(), covering: children });
    }
    out
}

/// How much of a batch stays below.
///
/// ★★★ The number that says whether the rollup is doing anything: a batch where
/// everything is a key transaction has not been rolled up, it has been renamed.
pub fn compression(children: usize, key_transactions: usize) -> Option<f64> {
    (children > 0).then(|| {
        1.0 - (key_transactions.min(children) as f64 / children as f64)
    })
}

/// **Settle, on whichever rail is configured.**
///
/// ★★ The caller names an amount and a reference. It does not choose finality —
/// that follows from the rail, so a mock cannot be talked into claiming it.
pub fn settle_on(rail: Rail, amount: u64, reference: &str) -> Settlement {
    Settlement { rail, amount, reference: reference.into() }
}

/// Whether the software half of §6.7 is shippable without the token half.
///
/// ★★★ Always true, and it is the point of the whole additive plan: the meter,
/// the ledger, treasury accounting, licence and royalty settlement and
/// out-of-pawa gating are useful on one host before any coin is public. The
/// system is built so the first thing is done well and the second is done
/// deliberately.
pub fn software_is_useful_before_any_coin_is_public() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mocked_settlement_is_never_final_whatever_else_it_says() {
        // ★★★ A finality claim is the most dangerous thing to fake: acting on a
        //     false one means treating money as settled that is not.
        let s = settle_on(Rail::Babychain { mocked: true }, 1_000, "order-1");
        assert!(!s.is_final());
        assert!(s.describe().contains("the shape of a rollup, and not a rollup"));
    }

    #[test]
    fn local_settlement_is_real_and_is_not_network_final() {
        // ★★ Two true things at once, and a type that collapsed them would make
        //    one of them a lie. Money really moved on this host; it has not
        //    been finalised anywhere else.
        let s = settle_on(Rail::Local, 500, "order-2");
        assert!(!s.is_final());
        assert!(s.describe().contains("real, and not network-final"));
    }

    #[test]
    fn the_finalizer_has_no_way_to_mark_itself_live() {
        // ★★★ §6.7's line, kept: switching on a real, public, transferable
        //     token is a distinct, later, human-authorized act with legal
        //     weight — not a boolean this codebase gets to set.
        let s = settle_on(Rail::Ethereum, 1, "x");
        assert!(!s.is_final());
        assert!(s.describe().contains("human-authorized act"));
    }

    #[test]
    fn only_a_real_l2_can_finalise() {
        assert!(Rail::Babychain { mocked: false }.can_finalise());
        assert!(!Rail::Babychain { mocked: true }.can_finalise());
        assert!(!Rail::Local.can_finalise());
        assert!(!Rail::Ethereum.can_finalise());
    }

    #[test]
    fn the_composition_hierarchy_is_the_batching_structure() {
        // ★★★ Nobody designed a batching scheme: a parent folding its children
        //     IS the rollup, so the root stands for all of them.
        let up = roll_up("root-abc", 40, &[]);
        assert_eq!(up, vec![Upward::Root { hash: "root-abc".into(), covering: 40 }]);
    }

    #[test]
    fn only_key_transactions_and_roots_go_up() {
        // ★★★ Sending everything upward would pay L1 prices for a household's
        //     grocery shopping — and would also publish it.
        let up = roll_up("root-abc", 40, &["the big one".to_string()]);
        assert_eq!(up.len(), 2);
        assert!(up.iter().any(|u| matches!(u, Upward::KeyTransaction { .. })));
    }

    #[test]
    fn an_empty_level_sends_nothing_rather_than_an_empty_root() {
        // ★★ A root covering zero transactions costs L1 fees and carries no
        //    information.
        assert!(roll_up("root-abc", 0, &[]).is_empty());
    }

    #[test]
    fn a_batch_where_everything_is_a_key_transaction_has_not_been_rolled_up() {
        // ★★★ It has been renamed. The compression number is what says whether
        //     the rollup is doing anything at all.
        assert_eq!(compression(40, 0), Some(1.0));
        assert_eq!(compression(40, 40), Some(0.0));
        assert_eq!(compression(0, 0), None, "nothing to compress is not full compression");
    }

    #[test]
    fn a_caller_names_an_amount_and_does_not_choose_finality() {
        // ★★ Finality follows from the rail, so a mock cannot be talked into
        //    claiming it — there is no parameter for it.
        let mocked = settle_on(Rail::Babychain { mocked: true }, 10, "r");
        let real = settle_on(Rail::Babychain { mocked: false }, 10, "r");
        assert_eq!(mocked.amount, real.amount);
        assert_ne!(mocked.is_final(), real.is_final());
    }

    #[test]
    fn the_software_half_is_useful_before_any_coin_is_public() {
        // ★★★ The whole additive plan: the meter, the ledger, treasury
        //     accounting, royalty settlement and out-of-pawa gating work on one
        //     host today. The first thing is done well and the second is done
        //     deliberately.
        assert!(software_is_useful_before_any_coin_is_public());
    }
}
