//! The economy, as the host holds it — **internal points only**.
//!
//! ## ★★★ The hard boundary, restated where the code lives
//!
//! ADR-0001 D5 and PAWA-13: juul is an **internal accounting unit on a single
//! host**. It is never real money, never a transferable asset, never a payment
//! rail. Nothing in this file — or anywhere in this app — can move real value,
//! because there is nothing here that reaches outside the process.
//!
//! ## What is genuinely wired
//!
//! ★★ All of it is the engine's own machinery, not a host imitation:
//!
//! - [`sustena_core::pawa::meter`] measures a committed run and produces a
//!   `PawaReading`, which has **no public constructor** — a reading exists only
//!   because work happened.
//! - `JuulLedger::charge` takes that reading, **not a number**, so a debit
//!   cannot be invented.
//! - `Affordability::Metered` puts `balance ≥ pawa` into the real gate, so an
//!   unaffordable run is refused *before it costs anything*.
//! - `serving` opts into PAWA-8's issuance seam, so the host earns
//!   `rate × pawa_served` for the work it did — at a **governed** rate.
//! - `Parameters` are read from a governance Sustain's state, and the only way
//!   to change one is `governance.set_parameter` through the gate.
//!
//! ★ **Genesis is declared, not calibrated.** The opening allocation below is a
//! number this host chose so a household can run; it is not derived from
//! anything, in exactly the way `κ` is not. Said here rather than discovered.

use serde_json::Value;
use sustena_core::{
    governance::{declared_parameters, opening_state, Parameters},
    issuance::{Issuance, Schedule},
    juul::{Genesis, JuulLedger},
};

/// The declared opening allocation for the local principal.
///
/// ★ Chosen so an ordinary household never trips the affordability clause by
/// accident. **Declared, not calibrated** — like `κ`, and named as such.
pub const GENESIS_JUUL: f64 = 1_000_000.0;

pub const GENESIS_ID: &str = "mycelium-local-genesis";
pub const ISSUANCE_ID: &str = "mycelium-local-issuance";

/// Everything the economy needs to run for one host.
pub struct Economy {
    pub genesis: Genesis,
    pub ledger: JuulLedger,
    pub issuance: Issuance,
    /// The governance Sustain's state. ★ A real state document — the parameters
    /// are read out of it, never from a constant.
    pub governance_state: Value,
}

impl Economy {
    pub fn open(principal: &str) -> Economy {
        let genesis = Genesis::declared(GENESIS_ID, &[(principal, GENESIS_JUUL)])
            .expect("a non-negative declared allocation");
        let ledger = JuulLedger::from_genesis(&genesis);
        Economy {
            genesis,
            ledger,
            issuance: Issuance::declared(ISSUANCE_ID, Schedule::Fixed)
                .expect("a named issuance"),
            governance_state: opening_state(&declared_parameters()),
        }
    }

    /// ★★ The governed parameters **in force**, read from the state.
    ///
    /// Never from a constant: `pawa::compute_pawa` was deleted precisely so
    /// this is the only way to price anything.
    pub fn parameters(&self) -> Parameters {
        Parameters::read(&self.governance_state)
    }
}
