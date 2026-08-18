//! The treasury is itself a Sustain, `Σ_T` (PAWA-6; the Pawa paper §5).
//!
//! ★★★ **The point of this row is that there is NO NEW MACHINERY.** A treasury
//! is a [`Definition`] plus a state, its viable region `V_T` is an ordinary set
//! of invariants, its `allocate` / `grant` / `disburse` are ordinary
//! [`OperatorMeta`] Enzymes, and every one of them is admitted by the **same**
//! `execute_admitted` every household operator goes through. The recursion is
//! the substance: **a treasury is governed exactly as a household is**, and if
//! it needed its own engine that claim would be false.
//!
//! So this module registers three operators and builds one `Definition`. That
//! is the whole of it.
//!
//! ## ★★ `V_T` — the reserve floor is an ORDINARY INVARIANT
//!
//! The floor is `treasury.balance >= <floor>`, declared in the definition's own
//! invariant list. **No bespoke floor check exists**: a disbursement that would
//! breach it is refused by the enforcement gate with `enforcement_gate`, the
//! same reason and the same shape a household invariant breach produces.
//! Category caps are the same device — `treasury.allocations.x <= <cap>`.
//!
//! ## ★★★ One truth for the juul, and how the state relates to the ledger
//!
//! The **[`JuulLedger`] is the one truth**. The treasury Sustain's state carries
//! a `treasury.balance`, but it is **never independently mutated**: every
//! outflow operator *requires* a `ledger_balance` parameter and writes
//! `ledger_balance − amount`, so the state figure is a **same-call projection**
//! of the ledger and cannot drift from it across calls.
//!
//! ★★ And the two halves move together or not at all: [`pay_out`] runs the
//! operator through the gate **and** performs the ledger transfer, and it
//! performs the transfer **only if the gate admitted**. A refused disbursement
//! leaves both the state and the ledger untouched; there is no path that moves
//! one without the other, because there is only one function that moves either.
//!
//! ## ★★★ Outflows are CONSERVED TRANSFERS, not mints
//!
//! A grant or a disbursement is a [`JuulLedger::transfer`] from the treasury's
//! balance to a recipient's — `ΣΔ = 0`, total circulation unchanged, exactly as
//! PAWA-5's royalty settlement is. The treasury **cannot pay out more than it
//! holds** (the ledger refuses, and the operator's own guard refuses first), and
//! **it cannot mint**: nothing in this module raises the total.
//!
//! ★ **A boundary named rather than pre-empted:** MYC-6 draws a later, different
//! distinction in which a *grant* is a mint. That is not this row. Here a
//! treasury grant is a **transfer from a balance it already holds** — which is
//! the honest thing to build while genesis (PAWA-7) and issuance (PAWA-8) do not
//! exist, because a grant that minted would need an issuance policy nobody has
//! declared.
//!
//! ## ★★ Every disbursement is human-gated
//!
//! An outflow runs under [`EffectClass::Live`], so it needs an
//! [`ApprovalToken`] — **bound to that one act and single-use**. An agent or a
//! strategy cannot disburse from the treasury on its own, for exactly the reason
//! `Agent::act` cannot fire a live effect: a real outflow stays the human's
//! explicit act. A disbursement without a token is refused with
//! `approval_token`, and nothing moves.

use serde_json::{json, Map, Value};

use crate::approval::{ApprovalToken, EffectClass, NonceLedger};
use crate::editing::Definition;
use crate::flow::Movement;
use crate::juul::{Charge, JuulLedger};
use crate::operator::{
    execute_admitted, meta::ParamDecl, meta::Protocol, Authorization, EmittedEvent, Enforcement,
    Execution, OperatorMeta, OperatorResult, Registry,
};
use crate::schema::{DimType, Schema};
use crate::state::State;

fn num(params: &Map<String, Value>, key: &str) -> f64 {
    params.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(params: &Map<String, Value>, key: &str) -> String {
    params.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

// ── the three Enzymes ────────────────────────────────────────────────────────

/// Earmark juul within the treasury. ★ **No juul moves** — this is a
/// categorisation, and the balance is untouched by it.
#[allow(clippy::ptr_arg)] // the `OperatorFn` signature is fixed; this one moves nothing
fn allocate(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let category = text(params, "category");
    let amount = num(params, "amount");
    if category.is_empty() {
        return OperatorResult::fail("An allocation needs a category.", "category_named");
    }
    let path = format!("treasury.allocations.{category}");
    if !state.exists(&path) && state.set(&path, json!(0.0)).is_err() {
        return OperatorResult::fail(
            format!("'{category}' is not a usable path segment."),
            "category_valid",
        );
    }
    if state.increment(&path, &json!(amount)).is_err() {
        return OperatorResult::fail("The allocation is not numeric.", "allocation_numeric");
    }
    events.push(EmittedEvent {
        name: "event.treasury.allocated".into(),
        payload: json!({"category": category, "amount": amount}),
    });
    OperatorResult::ok(json!({"category": category, "amount": amount}))
}

/// Pay juul out of the treasury.
///
/// ★★ Requires `ledger_balance` — the treasury's **real** balance, read from
/// the [`JuulLedger`] by [`pay_out`] at call time. The state figure is written
/// as `ledger_balance − amount`, so it is a same-call projection of the one
/// truth rather than an independently-tracked second number.
fn outflow(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    movements: &mut Vec<Movement>,
    kind: &'static str,
) -> OperatorResult {
    let recipient = text(params, "recipient");
    let amount = num(params, "amount");
    let ledger_balance = num(params, "ledger_balance");

    if recipient.is_empty() {
        return OperatorResult::fail("An outflow needs a recipient.", "recipient_named");
    }
    // ★ The treasury cannot pay out more than it holds — refused here, and
    // refused again by the ledger. Two independent guards, neither redundant:
    // this one keeps the *state* honest, the ledger's keeps the *balance* so.
    if ledger_balance < amount {
        return OperatorResult::fail(
            format!("The treasury holds {ledger_balance} and cannot pay {amount}."),
            "treasury_solvent",
        );
    }

    // The post-outflow figure the gate will judge `V_T` against.
    if state.set("treasury.balance", json!(ledger_balance - amount)).is_err() {
        return OperatorResult::fail("treasury.balance is not settable.", "balance_present");
    }

    // ★ A draw-down reduces its own earmark, so an allocation is not spendable
    // twice. A `grant` has no earmark to reduce.
    if kind == "disburse" {
        let category = text(params, "category");
        let path = format!("treasury.allocations.{category}");
        if !state.exists(&path) {
            return OperatorResult::fail(
                format!("Nothing is allocated to '{category}'."),
                "allocation_exists",
            );
        }
        if state.decrement(&path, &json!(amount), false).is_err() {
            return OperatorResult::fail(
                format!("'{category}' has less than {amount} allocated."),
                "allocation_sufficient",
            );
        }
    }

    movements.push(Movement::new("juul", amount, "treasury", &recipient));
    events.push(EmittedEvent {
        name: format!("event.treasury.{kind}d"),
        payload: json!({"recipient": recipient, "amount": amount}),
    });
    OperatorResult::ok(json!({"recipient": recipient, "amount": amount, "kind": kind}))
}

fn grant(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    movements: &mut Vec<Movement>,
) -> OperatorResult {
    outflow(state, params, events, movements, "grant")
}

fn disburse(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    movements: &mut Vec<Movement>,
) -> OperatorResult {
    outflow(state, params, events, movements, "disburse")
}

/// Register `treasury.*` on a registry.
///
/// ★ Ordinary [`OperatorMeta`] values, registered the ordinary way. Nothing
/// here marks them as special, and nothing in `execute_admitted` knows they are
/// a treasury's.
pub fn register(registry: &mut Registry) {
    registry.register(OperatorMeta {
        name: "treasury.allocate",
        description: "Earmark juul within the treasury. Moves nothing.",
        params: vec![ParamDecl::text("category"), ParamDecl::number("amount")],
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.treasury.allocated"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: allocate,
    });

    registry.register(OperatorMeta {
        name: "treasury.grant",
        description: "Pay juul out of the treasury without an earmark.",
        params: vec![
            ParamDecl::text("recipient"),
            ParamDecl::number("amount"),
            ParamDecl::number("ledger_balance"),
        ],
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.treasury.grantd"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 0,
        effect: None,
        run: grant,
    });

    registry.register(OperatorMeta {
        name: "treasury.disburse",
        description: "Pay juul out of the treasury, drawing down an allocation.",
        params: vec![
            ParamDecl::text("recipient"),
            ParamDecl::text("category"),
            ParamDecl::number("amount"),
            ParamDecl::number("ledger_balance"),
        ],
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.treasury.disbursed"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 0,
        effect: None,
        run: disburse,
    });
}

// ── `Σ_T` — the definition ───────────────────────────────────────────────────

/// Build the treasury's [`Definition`] — `⟨schema, V_T, allowed operators⟩`.
///
/// ★★ `reserve_floor` and each category cap become **ordinary invariants**.
/// There is no bespoke floor mechanism: `V_T` is a set of predicates and the
/// existing gate is what refuses a breach.
///
/// ★ `protocol_fee_per_mille` (`φ_p`) and `period_budget` are **declared
/// parameters** carried in the state. Changing them is governance (PAWA-11) and
/// is not an operator here — this row declares them, it does not decide who may
/// move them.
pub fn definition(reserve_floor: f64, caps: &[(&str, f64)]) -> Definition {
    let mut d = Definition::new(Schema::new().declare("treasury", DimType::Any))
        .with_operator("treasury.allocate")
        .with_operator("treasury.grant")
        .with_operator("treasury.disburse")
        // ★★ The reserve floor. An ordinary invariant, checked by the ordinary
        // gate, refusing with the ordinary reason.
        .with_invariant("reserve_floor", &format!("treasury.balance >= {reserve_floor}"));

    for (category, cap) in caps {
        d = d.with_invariant(
            &format!("cap_{category}"),
            &format!("treasury.allocations.{category} <= {cap}"),
        );
    }
    d
}

/// The treasury's opening state.
///
/// ★ `balance` starts as whatever the ledger says — see [`pay_out`] for why it
/// is a projection rather than a second record.
///
/// ★★★ **`caps` must be the same list the definition declared, and a test
/// caught why.** A cap is the invariant `treasury.allocations.x <= n`, and if
/// `x` is absent from the state the predicate compares `None` to a number —
/// which is a **type error**, and the gate then refuses **every** operation on
/// the treasury, including ones that have nothing to do with that category.
///
/// So a declared cap **implies the category exists**, and each one is seeded at
/// zero here. That is the same class of problem UI-2's load-time typecheck
/// exists for — a rule referencing a dimension the state does not hold — and
/// the honest fix at this level is to make the definition and the opening state
/// agree rather than to make the gate lenient. ★ Naming it as a residual too:
/// nothing yet *checks* that they agree.
pub fn opening_state(
    balance: f64,
    protocol_fee_per_mille: u32,
    period_budget: f64,
    caps: &[(&str, f64)],
) -> Value {
    let allocations: Map<String, Value> =
        caps.iter().map(|(c, _)| ((*c).to_string(), json!(0.0))).collect();
    json!({
        "treasury": {
            "balance": balance,
            "allocations": allocations,
            "protocol_fee_per_mille": protocol_fee_per_mille,
            "period_budget": period_budget,
        }
    })
}

/// The `Enforcement` a definition implies — `V_T`, armed.
///
/// ★ Reused from [`crate::semantic::enforcement_of`] rather than rebuilt, which
/// is the same *no new machinery* point in miniature.
pub fn enforcement(d: &Definition) -> Enforcement {
    crate::semantic::enforcement_of(d)
}

/// What a payout attempt did.
#[derive(Debug, Clone)]
pub struct PayOut {
    /// The gate's own verdict, including its refusal reason when it refused.
    pub execution: Execution,
    /// The ledger movement. `None` whenever the gate did not admit — **the
    /// transfer is not attempted at all**, rather than attempted and rolled
    /// back.
    pub moved: Option<Charge>,
}

impl PayOut {
    pub fn paid(&self) -> bool {
        self.execution.committed() && matches!(self.moved, Some(Charge::Charged { .. }))
    }
}

/// ★★★ Pay juul out of the treasury: gate, then ledger, in one call.
///
/// The **only** function in this module that moves juul, and it does both halves
/// or neither — so the Sustain's state and the ledger balance cannot diverge by
/// one succeeding without the other.
///
/// 1. Reads the treasury's **real** balance from the ledger and passes it as
///    `ledger_balance`, so the candidate state's figure is a same-call
///    projection of the one truth.
/// 2. Runs the operator through **`execute_admitted`** — the same gate a
///    household operator goes through, with `V_T`'s reserve floor and caps as
///    ordinary invariants, and under [`EffectClass::Live`] so the
///    **[`ApprovalToken`] is required**.
/// 3. **Only if it committed**, performs the [`JuulLedger::transfer`] — a
///    conserved move, never a mint.
#[allow(clippy::too_many_arguments)]
pub fn pay_out(
    registry: &Registry,
    definition: &Definition,
    ledger: &mut JuulLedger,
    nonces: &mut NonceLedger,
    treasury: &str,
    state: &Value,
    operator: &str,
    params: &Map<String, Value>,
    token: &ApprovalToken,
    now: u64,
) -> PayOut {
    let mut with_balance = params.clone();
    with_balance.insert("ledger_balance".into(), json!(ledger.balance_of(treasury)));

    let execution = execute_admitted(
        registry,
        &definition.operators,
        &enforcement(definition),
        state,
        operator,
        &with_balance,
        &Authorization::Unchecked,
        &EffectClass::Live { token, now },
        nonces,
    );

    if !execution.committed() {
        // ★ Nothing is attempted on the ledger. A refusal leaves both halves
        // untouched, which is why there is no rollback path to get wrong.
        return PayOut { execution, moved: None };
    }

    let recipient = text(&with_balance, "recipient");
    let amount = num(&with_balance, "amount");
    let moved = ledger.transfer(treasury, &recipient, amount, &format!("treasury:{operator}"));
    PayOut { execution, moved: Some(moved) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::juul::Genesis;
    use crate::approval::{Binding, Simulated};
    use crate::council::ProposalStatus;

    const FLOOR: f64 = 100.0;
    const CAPS: &[(&str, f64)] = &[("grants", 500.0)];

    fn registry() -> Registry {
        let mut r = Registry::default();
        register(&mut r);
        r
    }

    fn sigma_t() -> Definition {
        definition(FLOOR, CAPS)
    }

    fn funded(juul: f64) -> JuulLedger {
        JuulLedger::from_genesis(&Genesis::declared("g0", &[("treasury", juul)]).unwrap())
    }

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    /// A real approval token for one act — simulated, voted, approved.
    fn token_for(operator: &str, p: &Map<String, Value>, nonce: u64) -> ApprovalToken {
        Simulated::from_sandbox("prop-1", Binding::new(operator, p), Ok(()))
            .expect("the sandbox committed")
            .voted(ProposalStatus::Passed)
            .expect("passed")
            .approve("bonnie", nonce, 9_999)
    }

    /// A grant, with the token bound to what `pay_out` will actually call.
    fn grant_of(l: &mut JuulLedger, state: &Value, amount: f64, nonce: u64) -> PayOut {
        let d = sigma_t();
        let reg = registry();
        let mut nonces = NonceLedger::new();
        let asked = params(&[("recipient", json!("ada")), ("amount", json!(amount))]);
        let mut bound = asked.clone();
        bound.insert("ledger_balance".into(), json!(l.balance_of("treasury")));
        let token = token_for("treasury.grant", &bound, nonce);
        pay_out(
            &reg,
            &d,
            l,
            &mut nonces,
            "treasury",
            state,
            "treasury.grant",
            &asked,
            &token,
            1_000,
        )
    }

    // ── ★★★ no new machinery ────────────────────────────────────────────────

    #[test]
    fn the_treasury_is_an_ordinary_definition_with_ordinary_operators() {
        let d = sigma_t();
        let reg = registry();
        // Its operators are in the ordinary registry, with ordinary metadata.
        for op in &d.operators {
            let meta = reg.get(op).unwrap_or_else(|| panic!("{op} not registered"));
            assert!(!meta.params.is_empty(), "{op} declares its shape like any other");
        }
        // `V_T` is an ordinary invariant list — the floor is one of them.
        assert!(d
            .invariants
            .iter()
            .any(|(id, expr)| id == "reserve_floor" && expr.contains("treasury.balance >=")));
    }

    #[test]
    fn an_allocation_moves_no_juul_at_all() {
        let l = funded(1_000.0);
        let d = sigma_t();
        let before = l.total_in_circulation();
        let x = execute_admitted(
            &registry(),
            &d.operators,
            &enforcement(&d),
            &opening_state(1_000.0, 20, 300.0, CAPS),
            "treasury.allocate",
            &params(&[("category", json!("grants")), ("amount", json!(250.0))]),
            &Authorization::Unchecked,
            &EffectClass::Unchecked,
            &mut NonceLedger::new(),
        );
        assert!(x.committed(), "{:?}", x.result.reason);
        assert_eq!(x.state["treasury"]["allocations"]["grants"], json!(250.0));
        assert_eq!(x.state["treasury"]["balance"], json!(1_000.0), "an earmark is not a payment");
        assert_eq!(l.total_in_circulation(), before);
    }

    // ── ★★ V_T: the reserve floor is an ordinary invariant ──────────────────

    #[test]
    fn a_payout_that_would_breach_the_reserve_floor_is_refused_by_the_ordinary_gate() {
        // ★★ Balance 150, floor 100: paying 80 would leave 70. Refused —
        // and by `enforcement_gate`, the SAME reason a household breach gives.
        let mut l = funded(150.0);
        let before = l.clone();
        let out = grant_of(&mut l, &opening_state(150.0, 20, 300.0, CAPS), 80.0, 1);

        assert!(!out.paid());
        assert_eq!(out.execution.result.constraint_violated.as_deref(), Some("enforcement_gate"));
        assert!(out.moved.is_none(), "the transfer was not even attempted");
        assert_eq!(l, before, "the ledger is byte-identical");
    }

    #[test]
    fn a_payout_that_stays_above_the_floor_commits() {
        let mut l = funded(150.0);
        let out = grant_of(&mut l, &opening_state(150.0, 20, 300.0, CAPS), 40.0, 2);
        assert!(out.paid(), "{:?}", out.execution.result.reason);
        assert_eq!(l.balance_of("treasury"), 110.0);
        assert_eq!(l.balance_of("ada"), 40.0);
    }

    #[test]
    fn a_category_cap_is_the_same_device_as_the_floor() {
        // The cap on `grants` is 500; allocating 600 breaches `V_T`.
        let d = sigma_t();
        let x = execute_admitted(
            &registry(),
            &d.operators,
            &enforcement(&d),
            &opening_state(1_000.0, 20, 300.0, CAPS),
            "treasury.allocate",
            &params(&[("category", json!("grants")), ("amount", json!(600.0))]),
            &Authorization::Unchecked,
            &EffectClass::Unchecked,
            &mut NonceLedger::new(),
        );
        assert!(!x.committed());
        assert_eq!(x.result.constraint_violated.as_deref(), Some("enforcement_gate"));
    }

    // ── ★★★ conserved transfer, never a mint ────────────────────────────────

    #[test]
    fn a_treasury_outflow_leaves_total_circulation_unchanged() {
        let mut l = funded(1_000.0);
        let before = l.total_in_circulation();
        assert!(grant_of(&mut l, &opening_state(1_000.0, 20, 300.0, CAPS), 200.0, 3).paid());
        assert_eq!(l.total_in_circulation(), before, "moved, not minted");
        assert_eq!(l.balance_of("treasury") + l.balance_of("ada"), before);
    }

    #[test]
    fn the_treasury_cannot_pay_out_more_than_it_holds() {
        let mut l = funded(150.0);
        let before = l.clone();
        let out = grant_of(&mut l, &opening_state(150.0, 20, 300.0, CAPS), 500.0, 4);
        assert!(!out.paid());
        assert_eq!(out.execution.result.constraint_violated.as_deref(), Some("treasury_solvent"));
        assert_eq!(l, before);
    }

    // ── ★★ every disbursement is human-gated ────────────────────────────────

    #[test]
    fn a_payout_without_a_valid_token_is_refused() {
        // ★★ A token bound to a DIFFERENT act cannot admit this one — which is
        // the same thing as having no token for it.
        let mut l = funded(1_000.0);
        let before = l.clone();
        let d = sigma_t();
        let asked = params(&[("recipient", json!("ada")), ("amount", json!(50.0))]);
        let wrong = token_for("treasury.grant", &params(&[("recipient", json!("mallory"))]), 9);
        let out = pay_out(
            &registry(),
            &d,
            &mut l,
            &mut NonceLedger::new(),
            "treasury",
            &opening_state(1_000.0, 20, 300.0, CAPS),
            "treasury.grant",
            &asked,
            &wrong,
            1_000,
        );
        assert!(!out.paid());
        assert_eq!(out.execution.result.constraint_violated.as_deref(), Some("approval_token"));
        assert!(out.moved.is_none());
        assert_eq!(l, before);
    }

    #[test]
    fn a_token_is_single_use_so_a_payout_cannot_be_replayed() {
        let mut l = funded(1_000.0);
        let d = sigma_t();
        let reg = registry();
        let mut nonces = NonceLedger::new();
        let state = opening_state(1_000.0, 20, 300.0, CAPS);
        let asked = params(&[("recipient", json!("ada")), ("amount", json!(50.0))]);
        let mut bound = asked.clone();
        bound.insert("ledger_balance".into(), json!(1_000.0));

        let first = pay_out(
            &reg, &d, &mut l, &mut nonces, "treasury", &state, "treasury.grant", &asked,
            &token_for("treasury.grant", &bound, 7), 1_000,
        );
        assert!(first.paid());

        let after = l.clone();
        let replay = pay_out(
            &reg, &d, &mut l, &mut nonces, "treasury", &state, "treasury.grant", &asked,
            &token_for("treasury.grant", &bound, 7), 1_000,
        );
        assert!(!replay.paid(), "the nonce was already spent");
        assert_eq!(l, after, "and nothing moved twice");
    }

    // ── the state and the ledger are one truth ──────────────────────────────

    #[test]
    fn the_states_balance_is_a_projection_of_the_ledger_not_a_second_record() {
        let mut l = funded(1_000.0);
        let out = grant_of(&mut l, &opening_state(1_000.0, 20, 300.0, CAPS), 200.0, 5);
        assert!(out.paid());
        assert_eq!(
            out.execution.state["treasury"]["balance"].as_f64().unwrap(),
            l.balance_of("treasury"),
            "the candidate state agrees with the ledger, same call"
        );
    }

    #[test]
    fn a_disbursement_draws_down_its_own_earmark() {
        // ★ So an allocation cannot be spent twice.
        let mut l = funded(1_000.0);
        let d = sigma_t();
        let reg = registry();
        let allocated = execute_admitted(
            &reg,
            &d.operators,
            &enforcement(&d),
            &opening_state(1_000.0, 20, 300.0, CAPS),
            "treasury.allocate",
            &params(&[("category", json!("grants")), ("amount", json!(300.0))]),
            &Authorization::Unchecked,
            &EffectClass::Unchecked,
            &mut NonceLedger::new(),
        );
        assert!(allocated.committed());

        let asked = params(&[
            ("recipient", json!("ada")),
            ("category", json!("grants")),
            ("amount", json!(120.0)),
        ]);
        let mut bound = asked.clone();
        bound.insert("ledger_balance".into(), json!(1_000.0));
        let out = pay_out(
            &reg, &d, &mut l, &mut NonceLedger::new(), "treasury", &allocated.state,
            "treasury.disburse", &asked, &token_for("treasury.disburse", &bound, 11), 1_000,
        );
        assert!(out.paid(), "{:?}", out.execution.result.reason);
        assert_eq!(out.execution.state["treasury"]["allocations"]["grants"], json!(180.0));
    }

    // ── the recursion: a treasury simulates like any Sustain ────────────────

    #[test]
    fn the_treasury_replays_under_its_own_definition_like_any_sustain() {
        // ★★★ The recursion made concrete: EVT-15's `replay_under` takes a
        // `Definition` and knows nothing about treasuries.
        use crate::semantic::{replay_under, CallOutcome, ReplayMode};
        let d = sigma_t();
        let call = crate::semantic::EnzymeCall::new("t1", "treasury.allocate")
            .with("category", json!("grants"))
            .with("amount", json!(250.0));
        let out = replay_under(
            &[call],
            &registry(),
            &d,
            &opening_state(1_000.0, 20, 300.0, CAPS),
            ReplayMode::Replay,
        );
        assert!(
            matches!(out.steps.first(), Some(CallOutcome::Admitted { .. })),
            "{:?}",
            out.steps
        );
    }
}
