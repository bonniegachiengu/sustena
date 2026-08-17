//! The money operators — the most-used, most-consequential surface.
//!
//! Declarations (guards, emitted events) match the reference engine exactly;
//! parity is proven by conformance vectors, not by reading.
//!
//! One structural difference worth naming: in the reference, each operator
//! body calls `_check_constraints(...)` itself, so pre-conditions are enforced
//! by convention — an operator that forgot the call would simply not be
//! guarded. Here the guard is run by [`crate::operator::execute`] from the
//! declared metadata, before the body is entered. An unguarded operator is not
//! something you can write by omission.

use serde_json::{json, Map, Value};

use super::meta::{OperatorMeta, OperatorResult, Protocol, Registry};
use super::EmittedEvent;
use crate::flow::Movement;
use crate::state::State;

fn num(params: &Map<String, Value>, key: &str) -> f64 {
    params.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn text(params: &Map<String, Value>, key: &str) -> String {
    params.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string()
}

/// Collapse runs of non-word characters to `_`, then trim them from the ends.
///
/// Mirrors the reference's `[^\w]+` normalisation, which exists because a
/// pocket name reaches state as a dot-path segment: "holiday fund" would be
/// unaddressable, and the reference raised an unhandled path error before this
/// was added.
fn normalize_pocket_name(raw: &str) -> String {
    let mut out = String::new();
    let mut pending_underscore = false;
    for ch in raw.trim().chars() {
        if ch.is_alphanumeric() || ch == '_' {
            if pending_underscore && !out.is_empty() {
                out.push('_');
            }
            pending_underscore = false;
            out.push(ch);
        } else {
            pending_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn money(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        json!(v)
    } else {
        json!(v)
    }
}

// ── budget.record_income ──────────────────────────────────────────────────────

fn record_income(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    movements: &mut Vec<Movement>,
) -> OperatorResult {
    let amount = num(params, "amount");
    let source = text(params, "source");
    let entry_id = text(params, "entry_id");

    if state.increment("finances.liquid.balance", &json!(amount)).is_err() {
        return OperatorResult::fail(
            "finances.liquid.balance is missing or not numeric.",
            "liquid_balance_present",
        );
    }

    // `id` and `received_at` are HOST-SUPPLIED, not minted here.
    //
    // The reference generates a UUID and a timestamp inside the operator body.
    // A core whose promise is reproducibility cannot do that: the same inputs
    // would produce different state on every run, and no conformance vector
    // could pin it. Identity and time belong to the host, which owns a random
    // source and a clock. Everything else about the entry is behaviour and is
    // matched exactly.
    let frequency = {
        let f = text(params, "frequency");
        if f.is_empty() { "once".to_string() } else { f }
    };
    let entry = json!({
        "id": if entry_id.is_empty() { Value::Null } else { json!(entry_id) },
        "label": source,
        "amount": money(amount),
        "frequency": frequency,
        "received_at": params.get("received_at").cloned().unwrap_or(Value::Null),
    });
    let id = if entry_id.is_empty() { "income" } else { &entry_id };
    let _ = state.append("finances.income.sources", entry, id);
    let _ = state.increment("finances.income.monthly_total", &json!(amount));

    // ★ The declared crossing: money arrived from `source`. Whether `source`
    // is outside B is μ's question, not this operator's — see `crate::flow`.
    movements.push(Movement::new("money", amount, &source, "finances.liquid"));

    events.push(EmittedEvent {
        name: "event.finances.income_received".into(),
        payload: json!({"amount": money(amount), "source": source}),
    });

    OperatorResult::ok(json!({
        "amount": money(amount),
        "new_balance": state.get("finances.liquid.balance").cloned().unwrap_or(Value::Null),
    }))
}

// ── budget.allocate ───────────────────────────────────────────────────────────

fn allocate(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    movements: &mut Vec<Movement>,
) -> OperatorResult {
    let pocket_name = text(params, "pocket_name");
    let amount = num(params, "amount");
    let pocket_path = format!("finances.pockets.{pocket_name}");

    if !state.exists(&format!("{pocket_path}.allocated")) {
        if state
            .set(&pocket_path, json!({"allocated": 0.0, "spent": 0.0, "limit": 0.0}))
            .is_err()
        {
            return OperatorResult::fail(
                format!("Pocket name '{pocket_name}' is not a usable path segment."),
                "pocket_name_valid",
            );
        }
    }

    if state.decrement("finances.liquid.balance", &json!(amount), false).is_err() {
        return OperatorResult::fail(
            "Allocation would take the liquid balance negative.",
            "finances.liquid.balance >= params.amount",
        );
    }
    let _ = state.increment(&format!("{pocket_path}.allocated"), &json!(amount));

    // ★ Liquid → pocket. Declared like any other movement — and because μ owns
    // both ends, it classifies as INTERNAL and produces no flow at all. This is
    // the pocket-to-pocket half of §I's irreducibility case.
    movements.push(Movement::new(
        "money",
        amount,
        "finances.liquid",
        &format!("finances.pockets.{pocket_name}"),
    ));

    events.push(EmittedEvent {
        name: "event.finances.pocket_allocated".into(),
        payload: json!({"pocket": pocket_name, "amount": money(amount)}),
    });

    OperatorResult::ok(json!({
        "pocket": pocket_name,
        "amount": money(amount),
        "liquid_remaining": state.get("finances.liquid.balance").cloned().unwrap_or(Value::Null),
    }))
}

// ── budget.add_pocket ─────────────────────────────────────────────────────────

fn add_pocket(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let raw = text(params, "pocket_name");
    let pocket_name = normalize_pocket_name(&raw);
    let limit = num(params, "limit");

    if pocket_name.is_empty() {
        return OperatorResult::fail("Pocket name can't be empty.", "pocket_name_valid");
    }

    let pocket_path = format!("finances.pockets.{pocket_name}");
    if state.exists(&pocket_path) {
        return OperatorResult::fail(
            format!("Pocket '{pocket_name}' already exists."),
            "pocket_not_already_present",
        );
    }

    if state
        .set(&pocket_path, json!({"allocated": 0.0, "spent": 0.0, "limit": money(limit)}))
        .is_err()
    {
        return OperatorResult::fail(
            format!("Pocket name '{pocket_name}' is not a usable path segment."),
            "pocket_name_valid",
        );
    }

    events.push(EmittedEvent {
        name: "event.finances.pocket_created".into(),
        payload: json!({"pocket": pocket_name, "limit": money(limit)}),
    });

    OperatorResult::ok(json!({"pocket": pocket_name, "limit": money(limit)}))
}

// ── budget.spend ──────────────────────────────────────────────────────────────

fn spend(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    movements: &mut Vec<Movement>,
) -> OperatorResult {
    let pocket_name = text(params, "pocket_name");
    let amount = num(params, "amount");
    let pocket_path = format!("finances.pockets.{pocket_name}");

    if !state.exists(&format!("{pocket_path}.allocated")) {
        return OperatorResult::fail(
            format!("Pocket '{pocket_name}' does not exist. Create it with budget.allocate first."),
            "pocket_exists",
        );
    }

    let allocated = state
        .get(&format!("{pocket_path}.allocated"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let spent = state
        .get(&format!("{pocket_path}.spent"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let remaining = allocated - spent;

    if amount > remaining {
        // Structured shortfall alongside the prose, so a caller can build a
        // real allocate-then-retry loop without parsing a sentence.
        let mut result = OperatorResult::fail(
            format!(
                "Spend of KES {amount:.0} exceeds remaining balance in '{pocket_name}' pocket (KES {remaining:.0} left)."
            ),
            "pocket_balance_sufficient",
        );
        result.data = json!({
            "pocket": pocket_name,
            "remaining": money(remaining),
            "requested": money(amount),
            "shortfall": money(((amount - remaining) * 100.0).round() / 100.0),
        });
        return result;
    }

    let _ = state.increment(&format!("{pocket_path}.spent"), &json!(amount));

    // ★ Money left the pocket toward `payee`. Spending with no declared payee
    // names it "unknown" rather than inventing one — a crossing whose far side
    // nobody recorded is exactly what a firewall should be able to refuse.
    let payee = {
        let p = text(params, "payee");
        if p.is_empty() { "unknown".to_string() } else { p }
    };
    movements.push(Movement::new(
        "money",
        amount,
        &format!("finances.pockets.{pocket_name}"),
        &payee,
    ));

    events.push(EmittedEvent {
        name: "event.finances.pocket_spent".into(),
        payload: json!({"pocket": pocket_name, "amount": money(amount)}),
    });

    OperatorResult::ok(json!({
        "pocket": pocket_name,
        "amount": money(amount),
        "remaining": money(remaining - amount),
    }))
}

// ── registration ──────────────────────────────────────────────────────────────

pub fn register(registry: &mut Registry) {
    registry.register(OperatorMeta {
        name: "budget.record_income",
        description: "Record income into the liquid balance.",
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.finances.income_received"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: record_income,
    });

    registry.register(OperatorMeta {
        name: "budget.allocate",
        description: "Move an amount from liquid balance into a named budget pocket.",
        constraints: vec![
            "params.amount > 0".into(),
            "finances.liquid.balance >= params.amount".into(),
        ],
        post_constraints: vec![],
        side_effects: vec!["event.finances.pocket_allocated"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: allocate,
    });

    registry.register(OperatorMeta {
        name: "budget.add_pocket",
        description: "Create an empty pocket without moving money.",
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.finances.pocket_created"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: add_pocket,
    });

    registry.register(OperatorMeta {
        name: "budget.spend",
        description: "Spend from a pocket's remaining balance.",
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.finances.pocket_spent"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: spend,
    });

    // Test-only: mutates state in a way no guard would catch, so the gate is
    // the only thing standing between it and the ledger. Registered rather
    // than hidden behind cfg(test) so the conformance vectors can exercise the
    // gate through the same path a real operator takes.
    registry.register(OperatorMeta {
        name: "test.force_negative",
        description: "Force a pocket negative. Exists to prove the gate refuses it.",
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.test.forced"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 0,
        effect: None,
        run: |state, _params, events, _movements| {
            let _ = state.set(
                "finances.pockets.food",
                json!({"allocated": -999.0, "spent": 0.0, "limit": 0.0}),
            );
            events.push(EmittedEvent {
                name: "event.test.forced".into(),
                payload: json!({}),
            });
            OperatorResult::ok(json!({"forced": true}))
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pocket_names_are_normalised_to_addressable_segments() {
        assert_eq!(normalize_pocket_name("holiday fund"), "holiday_fund");
        assert_eq!(normalize_pocket_name("  Car-Repair!!  "), "Car_Repair");
        assert_eq!(normalize_pocket_name("food"), "food");
        assert_eq!(normalize_pocket_name("!!!"), "");
    }
}
