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

use super::meta::{OperatorMeta, OperatorResult, ParamDecl, Protocol, Registry};
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

/// The single seam every money value in this module passes through.
///
/// It is a passthrough, deliberately and honestly so. It once branched on
/// `v.fract() == 0.0` intending to emit an integral amount as `100` rather than
/// `100.0` — but both arms produced `json!(v)`, because `serde_json` keeps the
/// f64 representation either way and the branch could not have done what it
/// was named for. The dead condition is removed rather than the seam: keeping
/// one named place all amounts flow through is what makes a future
/// representation decision a one-line change instead of fourteen.
fn money(v: f64) -> Value {
    json!(v)
}

// ── accounts ─────────────────────────────────────────────────────────────────
//
// ★★★ Accounts are WHERE the money is; pockets are WHAT it is for. They are two
// readings of the same shilling, not two piles, and the law that ties them is:
//
//     Σ accounts[*].balance  ==  liquid  +  Σ (allocated − spent)
//
// Read it as: every shilling you hold is either unearmarked, or earmarked for
// something and not yet spent. Under that law each move touches exactly the
// terms it should — income raises an account and liquid, allocating moves
// liquid into a pocket without any money leaving an account, spending takes it
// out of an account against a pocket, and a transfer moves it between accounts
// and changes neither side of the earmarking.
//
// The predicate DSL has no arithmetic, so this cannot be a declared invariant
// the way `liquid >= 0` is. It holds by construction here, and the tests state
// it directly rather than leaving it as a claim in a comment.
//
// ★★ Where a message did not say which account, the money goes to `unassigned`
// rather than nowhere. A default that quietly skipped the account side would
// break the law above and leave the total unexplainable; a real balance sitting
// under a name he can see is a question he can answer.
const UNASSIGNED: &str = "unassigned";

/// The account a call names, or the honest stand-in.
fn account_of(params: &Map<String, Value>) -> String {
    let a = text(params, "account");
    if a.is_empty() { UNASSIGNED.to_string() } else { normalize_pocket_name(&a) }
}

/// Move an account's balance, creating the account the first time it is named.
///
/// ★★ No guard on going negative, and that is deliberate for now. An account
/// balance only becomes true once it has been reconciled against what the bank
/// itself reports; guarding on a figure that has not been established yet would
/// refuse honest history for failing a test it was never given the data to
/// pass. The guard belongs with reconciliation, not before it.
fn move_account(state: &mut State, account: &str, delta: f64) {
    let path = format!("finances.accounts.{account}");
    if !state.exists(&format!("{path}.balance")) {
        let _ = state.set(&path, json!({"label": account, "balance": 0.0}));
    }
    let _ = state.increment(&format!("{path}.balance"), &json!(delta));
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
    // Money arrived somewhere real. The source of the text says where, so this
    // costs the person no extra question.
    move_account(state, &account_of(params), amount);

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

    // `&&` short-circuits, so the `set` still runs only when the pocket is
    // absent — collapsing the nesting changes the shape, not the behaviour.
    if !state.exists(&format!("{pocket_path}.allocated"))
        && state
            .set(&pocket_path, json!({"allocated": 0.0, "spent": 0.0, "limit": 0.0}))
            .is_err()
    {
        return OperatorResult::fail(
            format!("Pocket name '{pocket_name}' is not a usable path segment."),
            "pocket_name_valid",
        );
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
    // ★★ A spend leaves an ACCOUNT and lands against a POCKET. Liquid is
    //    untouched, because the money stopped being unearmarked when it was
    //    allocated, not when it was spent.
    move_account(state, &account_of(params), -amount);

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

// ── budget.unspend ───────────────────────────────────────────────────────────

/// The declared inverse of `budget.spend`.
///
/// ★★★ A refund is not a negative spend, it is the undo of one. The money the
/// pocket recorded as gone is back, so `spent` falls and the pocket's room
/// grows by the same amount. Liquid is untouched, because a spend never took
/// anything out of liquid in the first place.
///
/// ★★ Why a named operator rather than `inverse::invert` over the original
/// event's mutations. The generic patch-level inverse in `inverse.rs` is the
/// stronger general mechanism, but applying it needs an operator that accepts a
/// raw mutation list, and that is a far wider door than this needs -- one that
/// could rewrite any path in the state. §V of that module calls a declared
/// semantic inverse a move in `T` subject to the same gate as any other move,
/// which is exactly what this is: narrow, guarded, and named after what it
/// means rather than after the bytes it touches.
fn unspend(
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
            format!("Pocket '{pocket_name}' does not exist, so there is no spend to undo."),
            "pocket_exists",
        );
    }

    let spent = state
        .get(&format!("{pocket_path}.spent"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // ★★★ The second clause of §V: an inverse that refuses at the states the
    // forward move produces is not an inverse. This refuses only where there
    // was never that much spending to undo, which is a different thing.
    if amount > spent {
        let mut result = OperatorResult::fail(
            format!(
                "Refund of KES {amount:.0} is more than '{pocket_name}' has recorded as spent (KES {spent:.0})."
            ),
            "pocket_spent_sufficient",
        );
        result.data = json!({
            "pocket": pocket_name,
            "spent": money(spent),
            "requested": money(amount),
            "excess": money(((amount - spent) * 100.0).round() / 100.0),
        });
        return result;
    }

    let _ = state.decrement(&format!("{pocket_path}.spent"), &json!(amount), false);
    // The mirror of the spend: the money is back in the account it left.
    move_account(state, &account_of(params), amount);

    // The mirror of the spend's own movement: back from the payee into the
    // pocket. A refund really is money crossing the boundary inward.
    let payer = {
        let p = text(params, "payer");
        if p.is_empty() { "unknown".to_string() } else { p }
    };
    movements.push(Movement::new(
        "money",
        amount,
        &payer,
        &format!("finances.pockets.{pocket_name}"),
    ));

    events.push(EmittedEvent {
        name: "event.finances.pocket_refunded".into(),
        payload: json!({"pocket": pocket_name, "amount": money(amount)}),
    });

    OperatorResult::ok(json!({
        "pocket": pocket_name,
        "amount": money(amount),
        "spent": money(spent - amount),
    }))
}

// ── budget.unallocate ────────────────────────────────────────────────────────

/// The declared inverse of `budget.allocate`: money leaves a pocket for liquid.
///
/// ★★★ Only unspent money can come back. Allocated money that has already been
/// spent is gone from the pocket's point of view, so the guard is the pocket's
/// remaining room rather than its whole allocation -- otherwise this would let
/// a pocket claim back money it no longer has and leave `spent > allocated`.
fn unallocate(
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
            format!("Pocket '{pocket_name}' does not exist, so there is nothing to take back."),
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
    let free = allocated - spent;

    if amount > free {
        let mut result = OperatorResult::fail(
            format!(
                "'{pocket_name}' has only KES {free:.0} unspent, so KES {amount:.0} cannot come back out."
            ),
            "pocket_unspent_sufficient",
        );
        result.data = json!({
            "pocket": pocket_name,
            "unspent": money(free),
            "requested": money(amount),
            "excess": money(((amount - free) * 100.0).round() / 100.0),
        });
        return result;
    }

    let _ = state.decrement(&format!("{pocket_path}.allocated"), &json!(amount), false);
    let _ = state.increment("finances.liquid.balance", &json!(amount));

    // Pocket to liquid. Both ends are inside the household, so like the
    // allocation it mirrors, this classifies as internal and is not a flow.
    movements.push(Movement::new(
        "money",
        amount,
        &format!("finances.pockets.{pocket_name}"),
        "finances.liquid",
    ));

    events.push(EmittedEvent {
        name: "event.finances.pocket_unallocated".into(),
        payload: json!({"pocket": pocket_name, "amount": money(amount)}),
    });

    OperatorResult::ok(json!({
        "pocket": pocket_name,
        "amount": money(amount),
        "liquid_remaining": state.get("finances.liquid.balance").cloned().unwrap_or(Value::Null),
    }))
}

// ── budget.open_account ──────────────────────────────────────────────────────

/// Declare an account by name, so it can be seen before anything lands in it.
///
/// ★ Accounts also appear on first use, the way pockets do. This exists for the
/// other order: naming the two you actually hold before reading any texts, so
/// the list on screen is yours rather than whatever the parser happened to see.
fn open_account(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let id = normalize_pocket_name(&text(params, "account"));
    if id.is_empty() {
        return OperatorResult::fail("An account needs a name.", "account_name_valid");
    }
    let label = {
        let l = text(params, "label");
        if l.is_empty() { id.clone() } else { l }
    };
    let path = format!("finances.accounts.{id}");
    if state.exists(&path) {
        return OperatorResult::fail(
            format!("Account '{id}' already exists."),
            "account_not_already_present",
        );
    }
    if state.set(&path, json!({"label": label, "balance": 0.0})).is_err() {
        return OperatorResult::fail(
            format!("Account name '{id}' is not a usable path segment."),
            "account_name_valid",
        );
    }
    events.push(EmittedEvent {
        name: "event.finances.account_opened".into(),
        payload: json!({"account": id, "label": label}),
    });
    OperatorResult::ok(json!({"account": id, "label": label}))
}

// ── budget.transfer ──────────────────────────────────────────────────────────

/// Move money between two of your own accounts.
///
/// ★★★ Net zero, and that is the whole point. Sending from KCB to M-Pesa
/// produces two texts that each look like a real transaction, and taking them
/// at face value books an expense and an income that never happened. One move
/// between two accounts leaves liquid alone, touches no pocket, and adds
/// nothing to income.
fn transfer(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    movements: &mut Vec<Movement>,
) -> OperatorResult {
    let from = normalize_pocket_name(&text(params, "from_account"));
    let to = normalize_pocket_name(&text(params, "to_account"));
    let amount = num(params, "amount");

    if from.is_empty() || to.is_empty() {
        return OperatorResult::fail(
            "A transfer needs an account at both ends.",
            "transfer_endpoints_named",
        );
    }
    if from == to {
        return OperatorResult::fail(
            format!("'{from}' is both ends of this transfer, so nothing would move."),
            "transfer_endpoints_differ",
        );
    }

    move_account(state, &from, -amount);
    move_account(state, &to, amount);

    // Both ends are the household's own, so this is internal and is not a flow.
    movements.push(Movement::new(
        "money",
        amount,
        &format!("finances.accounts.{from}"),
        &format!("finances.accounts.{to}"),
    ));

    events.push(EmittedEvent {
        name: "event.finances.transferred".into(),
        payload: json!({"from": from, "to": to, "amount": money(amount)}),
    });

    OperatorResult::ok(json!({"from": from, "to": to, "amount": money(amount)}))
}

// ── budget.place_unaccounted ─────────────────────────────────────────────────

/// How much the household holds, read off the pockets and liquid.
///
/// ★ The right-hand side of the conservation law: every shilling is either
/// unearmarked, or earmarked for something and not yet spent.
pub fn held(state: &State) -> f64 {
    let liquid = state.get("finances.liquid.balance").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let earmarked: f64 = state
        .get("finances.pockets")
        .and_then(|v| v.as_object().cloned())
        .map(|m| {
            m.values()
                .map(|p| {
                    let a = p.get("allocated").and_then(Value::as_f64).unwrap_or(0.0);
                    let sp = p.get("spent").and_then(Value::as_f64).unwrap_or(0.0);
                    a - sp
                })
                .sum()
        })
        .unwrap_or(0.0);
    liquid + earmarked
}

/// How much the accounts say the household holds.
pub fn in_accounts(state: &State) -> f64 {
    state
        .get("finances.accounts")
        .and_then(|v| v.as_object().cloned())
        .map(|m| m.values().filter_map(|a| a.get("balance")?.as_f64()).sum())
        .unwrap_or(0.0)
}

/// Money the household holds but has not said where it is.
///
/// ★★★ This is the migration from a single pooled balance to real accounts,
/// and it is deliberately the only shape that migration takes. It does not set
/// a balance to a figure someone typed, and it cannot create or destroy money:
/// it moves the gap between what the pockets say is held and what the accounts
/// account for, which is exactly zero once every shilling has a place.
///
/// ★★ It refuses when there is no gap. A migration that quietly does nothing
/// reads the same as one that worked, and the difference matters when the
/// question is where someone's money is.
fn place_unaccounted(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let account = account_of(params);
    let gap = ((held(state) - in_accounts(state)) * 100.0).round() / 100.0;

    if gap.abs() < 0.005 {
        return OperatorResult::fail(
            "Every shilling is already in an account, so there is nothing to place.",
            "unaccounted_money_exists",
        );
    }

    move_account(state, &account, gap);

    events.push(EmittedEvent {
        name: "event.finances.unaccounted_placed".into(),
        payload: json!({"account": account, "amount": money(gap)}),
    });

    OperatorResult::ok(json!({
        "account": account,
        "amount": money(gap),
        "held": money(held(state)),
        "in_accounts": money(in_accounts(state)),
    }))
}

// ── registration ─────────────────────────────────────────────────────────────

pub fn register(registry: &mut Registry) {
    registry.register(OperatorMeta {
        name: "budget.record_income",
        description: "Record income into the liquid balance.",
        params: vec![
            ParamDecl::number("amount"),
            ParamDecl::text("source"),
            ParamDecl::text("entry_id").optional(),
            ParamDecl::text("frequency").optional(),
            ParamDecl::text("received_at").optional(),
            ParamDecl::text("account").optional(),
        ],
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
        params: vec![
            ParamDecl::naming("pocket_name", "finances.pockets"),
            ParamDecl::number("amount"),
        ],
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
        params: vec![
            ParamDecl::text("pocket_name"),
            ParamDecl::number("limit").optional(),
        ],
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
        params: vec![
            ParamDecl::naming("pocket_name", "finances.pockets"),
            ParamDecl::number("amount"),
            ParamDecl::text("payee").optional(),
            ParamDecl::text("account").optional(),
        ],
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.finances.pocket_spent"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: spend,
    });

    registry.register(OperatorMeta {
        name: "budget.unspend",
        description: "Undo a spend: a refund returns room to the pocket it left.",
        params: vec![
            ParamDecl::naming("pocket_name", "finances.pockets"),
            ParamDecl::number("amount"),
            ParamDecl::text("payer").optional(),
            ParamDecl::text("account").optional(),
        ],
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.finances.pocket_refunded"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: unspend,
    });

    registry.register(OperatorMeta {
        name: "budget.unallocate",
        description: "Undo an allocation: unspent money in a pocket returns to liquid.",
        params: vec![
            ParamDecl::naming("pocket_name", "finances.pockets"),
            ParamDecl::number("amount"),
        ],
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.finances.pocket_unallocated"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: unallocate,
    });

    registry.register(OperatorMeta {
        name: "budget.open_account",
        description: "Declare an account you hold money in.",
        params: vec![ParamDecl::text("account"), ParamDecl::text("label").optional()],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.finances.account_opened"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: open_account,
    });

    registry.register(OperatorMeta {
        name: "budget.transfer",
        description: "Move money between two of your own accounts. Net zero.",
        params: vec![
            ParamDecl::text("from_account"),
            ParamDecl::text("to_account"),
            ParamDecl::number("amount"),
        ],
        constraints: vec!["params.amount > 0".into()],
        post_constraints: vec![],
        side_effects: vec!["event.finances.transferred"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: transfer,
    });

    registry.register(OperatorMeta {
        name: "budget.place_unaccounted",
        description: "Say which account already-counted money is actually sitting in.",
        params: vec![ParamDecl::text("account").optional()],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.finances.unaccounted_placed"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: place_unaccounted,
    });

    // Test-only: mutates state in a way no guard would catch, so the gate is
    // the only thing standing between it and the ledger. Registered rather
    // than hidden behind cfg(test) so the conformance vectors can exercise the
    // gate through the same path a real operator takes.
    registry.register(OperatorMeta {
        name: "test.force_negative",
        description: "Force a pocket negative. Exists to prove the gate refuses it.",
        params: vec![],
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
