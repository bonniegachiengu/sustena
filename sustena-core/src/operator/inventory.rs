//! Supplies become assets — `inventory.itemize`.
//!
//! ★★★ **What a spend actually did.** "Food, KES 90" records that money left.
//! It does not record that beans, onions and carrots arrived, and those are
//! real things the household now holds. Until they are written down, the only
//! account of the week is money going out, which makes a household that shops
//! well look identical to one that loses money.
//!
//! ```text
//! spend      money out of an account, against a pocket   (already recorded)
//! itemize    what that money turned into                 (this)
//! consume    the real expense, when it is used up        (later)
//! ```
//!
//! ★★★ **Itemizing moves no money, and that is the whole design.** The spend
//! already took it out of the account. If this moved money too, the same
//! shilling would leave twice. What it records is the other side of a purchase
//! that has already happened: net worth is unchanged at the moment of buying,
//! because cash became beans. The expense comes later, when the beans are
//! eaten — which is what `consume` will be for.
//!
//! ★★ One call, all lines, all or nothing. A basket half-recorded is worse
//! than one not recorded at all: the total no longer matches the receipt and
//! nobody can tell which half is missing.

use serde_json::{json, Map, Value};

use super::meta::{OperatorMeta, OperatorResult, ParamDecl, Protocol, Registry};
use super::EmittedEvent;
use crate::flow::Movement;
use crate::state::State;

fn num(params: &Map<String, Value>, key: &str) -> f64 {
    params.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(params: &Map<String, Value>, key: &str) -> String {
    params.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn money(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// One line of a receipt.
struct Line {
    item: String,
    value: f64,
    subpocket: String,
}

/// Read the lines, or say which one is wrong.
///
/// ★★ Named in the refusal. "A line is invalid" sends someone back to read
/// their own basket looking for it; "line 3 has no name" does not.
fn read_lines(params: &Map<String, Value>) -> Result<Vec<Line>, (String, &'static str)> {
    let raw = params
        .get("lines")
        .and_then(Value::as_array)
        .ok_or_else(|| ("Itemizing needs a list of what was bought.".to_string(), "lines_present"))?;
    if raw.is_empty() {
        return Err(("Nothing was listed.".to_string(), "lines_present"));
    }

    let mut out = Vec::new();
    for (i, entry) in raw.iter().enumerate() {
        let n = i + 1;
        let item = entry.get("item").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if item.is_empty() {
            return Err((format!("Line {n} has no name."), "line_has_a_name"));
        }
        let value = entry.get("value").and_then(Value::as_f64).unwrap_or(0.0);
        if !(value > 0.0) {
            return Err((format!("'{item}' has no amount."), "line_has_an_amount"));
        }
        let subpocket = entry
            .get("subpocket")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        out.push(Line { item, value, subpocket });
    }
    Ok(out)
}

/// Use up something the household holds — the real expense.
///
/// ★★★ **This is where the money is actually spent.** Buying rice moved cash
/// into a sack of rice and left the household no poorer; eating the rice is
/// what makes it poorer. Until now the ledger called the purchase the expense,
/// which is off by however long the thing lasts — a month's shopping looks
/// like a terrible week and the week it is eaten looks free.
///
/// ★★ Partial by design. Half a sack is the normal case; an asset consumed in
/// part keeps its identity and loses value, and only a fully used one is
/// closed. Splitting it into a new asset for the remainder would multiply the
/// list every time anyone cooked.
///
/// ★★★ It moves no money either, and for the same reason `itemize` does not:
/// the cash left when it was bought. What changes is what the household still
/// HAS. The pocket's `spent` already recorded the purchase; consumption is the
/// falling value of what that purchase bought.
fn consume(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let asset_id = text(params, "asset_id");
    if asset_id.is_empty() {
        return OperatorResult::fail("Which thing was used up?", "asset_named");
    }

    let assets = state
        .get("inventory.assets")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let Some(idx) = assets
        .iter()
        .position(|a| a.get("id").and_then(Value::as_str) == Some(asset_id.as_str()))
    else {
        return OperatorResult::fail(
            format!("Nothing called '{asset_id}' is held."),
            "asset_exists",
        );
    };

    let held = assets[idx].get("value").and_then(Value::as_f64).unwrap_or(0.0);
    if held <= 0.0 {
        return OperatorResult::fail(
            "That was already used up.".to_string(),
            "asset_not_already_used",
        );
    }

    // ★★ No amount named means all of it, which is what "used up" usually
    //    means when someone says it.
    let asked = num(params, "amount");
    let used = if asked > 0.0 { money(asked) } else { held };
    if used > held + 0.005 {
        let mut result = OperatorResult::fail(
            format!("Only KES {held:.0} of that is left to use."),
            "consume_within_holding",
        );
        result.data = json!({"held": money(held), "requested": used});
        return result;
    }

    let item = assets[idx].get("item").and_then(Value::as_str).unwrap_or("").to_string();
    let pocket = assets[idx].get("pocket").and_then(Value::as_str).unwrap_or("").to_string();
    let left = money(held - used);
    let path = format!("inventory.assets[{idx}].value");
    if state.set(&path, json!(left)).is_err() {
        return OperatorResult::fail(
            "Could not reach that item to change it.".to_string(),
            "asset_reachable",
        );
    }

    // ★★★ THE expense, declared. Goods held became goods used: the asset
    //     falls and the value goes to consumption. Without this the asset's
    //     fall was a single-sided entry — value leaving the household with
    //     nothing saying where it went.
    _movements.push(Movement::new(
        "money",
        used,
        &format!("inventory.assets[{idx}]"),
        "expense:consumption",
    ));

    events.push(EmittedEvent {
        name: "event.inventory.consumed".into(),
        payload: json!({"asset": asset_id, "item": item, "used": used, "pocket": pocket}),
    });

    OperatorResult::ok(json!({
        "asset": asset_id,
        "item": item,
        "used": used,
        "left": left,
        "finished": left <= 0.0,
    }))
}

/// Record what a spend brought into the household.
fn itemize(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let pocket_name = text(params, "pocket_name");
    let source_tx = text(params, "source_tx");
    if source_tx.is_empty() {
        // ★★ Without it an asset cannot say where it came from, and the pocket
        //    cannot show what its spending actually bought.
        return OperatorResult::fail(
            "Supplies have to say which purchase they came from.",
            "source_tx_present",
        );
    }

    let pocket_path = format!("finances.pockets.{pocket_name}");
    if !state.exists(&format!("{pocket_path}.allocated")) {
        return OperatorResult::fail(
            format!("Pocket '{pocket_name}' does not exist."),
            "pocket_exists",
        );
    }

    let lines = match read_lines(params) {
        Ok(l) => l,
        Err((why, rule)) => return OperatorResult::fail(why, rule),
    };

    // ★★★ The receipt has to add up. Listing more than was spent would put
    //     value into the household that no money paid for — inventory invented
    //     out of nothing, and every total downstream wrong by the difference.
    let limit = num(params, "limit");
    let listed: f64 = money(lines.iter().map(|l| l.value).sum());
    if limit > 0.0 && listed > money(limit) + 0.005 {
        let mut result = OperatorResult::fail(
            format!(
                "The list comes to KES {listed:.0}, more than the KES {limit:.0} that was spent."
            ),
            "lines_within_spend",
        );
        result.data = json!({
            "listed": listed,
            "limit": money(limit),
            "over": money(listed - limit),
        });
        return result;
    }

    // ★★ Already itemized is refused, not added to. Running it twice would
    //    double the shopping while the spend stayed where it was.
    let existing = state
        .get("inventory.assets")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    if existing
        .iter()
        .any(|a| a.get("source_tx").and_then(Value::as_str) == Some(source_tx.as_str()))
    {
        return OperatorResult::fail(
            "This purchase has already been itemized.".to_string(),
            "not_already_itemized",
        );
    }

    let acquired_at = params.get("acquired_at").cloned().unwrap_or(Value::Null);
    for (i, line) in lines.iter().enumerate() {
        // ★ Derived from the purchase and the line's position, so replaying the
        //   log produces the same ids. The core has no random source and must
        //   not pretend to.
        let id = format!("{source_tx}-{i}");
        let asset = json!({
            "id": id,
            "item": line.item,
            "value": money(line.value),
            "source_tx": source_tx,
            "pocket": pocket_name,
            "subpocket": if line.subpocket.is_empty() { Value::Null } else { json!(line.subpocket) },
            "acquired_at": acquired_at,
        });
        let _ = state.append("inventory.assets", asset, &id);
    }

    events.push(EmittedEvent {
        name: "event.inventory.acquired".into(),
        payload: json!({
            "pocket": pocket_name,
            "source_tx": source_tx,
            "count": lines.len(),
            "value": listed,
        }),
    });

    OperatorResult::ok(json!({
        "pocket": pocket_name,
        "count": lines.len(),
        "value": listed,
        // What the receipt does not account for. Honest rather than hidden:
        // it stays an ordinary spend, and saying so is how he knows the list
        // was partial.
        "unlisted": if limit > 0.0 { money(limit - listed) } else { 0.0 },
    }))
}

pub fn register(registry: &mut Registry) {
    registry.register(OperatorMeta {
        name: "inventory.itemize",
        description: "Record what a spend brought in, as things the household now holds.",
        params: vec![
            ParamDecl::naming("pocket_name", "finances.pockets"),
            ParamDecl::text("source_tx"),
            ParamDecl::number("limit").optional(),
            ParamDecl::text("acquired_at").optional(),
        ],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.inventory.acquired"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: itemize,
    });

    registry.register(OperatorMeta {
        name: "inventory.consume",
        description: "Use up something held. This is the real expense, not the purchase.",
        params: vec![ParamDecl::text("asset_id"), ParamDecl::number("amount").optional()],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.inventory.consumed"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: consume,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::{execute, Enforcement, Execution, Registry as Reg};

    fn household() -> Value {
        json!({"finances": {
            "liquid": {"balance": 1000.0},
            "pockets": {"food": {"allocated": 500.0, "spent": 90.0, "limit": 0.0}},
            "accounts": {},
            "income": {"monthly_total": 0.0, "sources": []}},
            "inventory": {"assets": []}})
    }

    fn lines(v: &[(&str, f64)]) -> Value {
        Value::Array(
            v.iter().map(|(i, a)| json!({"item": i, "value": a})).collect(),
        )
    }

    fn call(state: &Value, ps: &[(&str, Value)]) -> Execution {
        let reg = Reg::default();
        let params: Map<String, Value> =
            ps.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        execute(&reg, &reg.names(), &Enforcement::default(), state, "inventory.itemize", &params)
    }

    fn assets(state: &Value) -> Vec<Value> {
        state.pointer("/inventory/assets").and_then(Value::as_array).cloned().unwrap_or_default()
    }

    // ── using it up ──────────────────────────────────────────────────────

    fn consumed(state: &Value, ps: &[(&str, Value)]) -> Execution {
        let reg = Reg::default();
        let params: Map<String, Value> =
            ps.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        execute(&reg, &reg.names(), &Enforcement::default(), state, "inventory.consume", &params)
    }

    /// A household holding one 120-shilling sack of rice.
    fn holding() -> Value {
        let ex = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("lines", lines(&[("rice", 120.0)])),
            ],
        );
        assert!(ex.committed());
        ex.state
    }

    #[test]
    fn using_something_up_is_the_real_expense_and_moves_no_money() {
        // ★★★ Buying rice left the household no poorer — cash became rice.
        //     Eating it is what makes it poorer, and that is this. The cash
        //     already left when it was bought, so this must not take it again.
        let before = holding();
        let ex = consumed(&before, &[("asset_id", json!("msg-1-0"))]);
        assert!(ex.committed(), "{:?}", ex.result.reason);
        assert_eq!(assets(&ex.state)[0]["value"].as_f64(), Some(0.0), "the rice is gone");
        assert_eq!(
            ex.state.pointer("/finances").unwrap(),
            before.pointer("/finances").unwrap(),
            "and not one figure in finances moved"
        );
        assert_eq!(ex.result.data["finished"], json!(true));
    }

    #[test]
    fn half_a_sack_is_the_normal_case() {
        // ★★ An asset used in part keeps its identity and loses value.
        //    Splitting it into a new asset for the remainder would multiply
        //    the list every time anyone cooked.
        let ex = consumed(&holding(), &[("asset_id", json!("msg-1-0")), ("amount", json!(50.0))]);
        assert!(ex.committed());
        assert_eq!(assets(&ex.state)[0]["value"].as_f64(), Some(70.0));
        assert_eq!(assets(&ex.state).len(), 1, "still one thing, worth less");
        assert_eq!(ex.result.data["finished"], json!(false));
    }

    #[test]
    fn using_more_than_is_held_is_refused() {
        // ★★★ Otherwise value leaves the household that was never in it, and
        //     the running total of what is held goes negative.
        let ex = consumed(&holding(), &[("asset_id", json!("msg-1-0")), ("amount", json!(200.0))]);
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("consume_within_holding"));
        assert_eq!(ex.result.data["held"].as_f64(), Some(120.0));
    }

    #[test]
    fn using_something_up_twice_is_refused() {
        let once = consumed(&holding(), &[("asset_id", json!("msg-1-0"))]);
        let twice = consumed(&once.state, &[("asset_id", json!("msg-1-0"))]);
        assert!(!twice.committed());
        assert_eq!(twice.result.constraint_violated.as_deref(), Some("asset_not_already_used"));
    }

    #[test]
    fn something_not_held_cannot_be_used_up() {
        let ex = consumed(&holding(), &[("asset_id", json!("nothing"))]);
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("asset_exists"));
    }

    #[test]
    fn naming_nothing_to_use_is_refused_rather_than_guessed_at() {
        let ex = consumed(&holding(), &[]);
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("asset_named"));
    }

    #[test]
    fn the_purchase_stays_on_the_record_after_it_is_eaten() {
        // ★★ Consuming is not deleting. The asset keeps its name, its pocket
        //    and the purchase it came from, so "what did the food money buy"
        //    still answers after the food is gone.
        let ex = consumed(&holding(), &[("asset_id", json!("msg-1-0"))]);
        let a = &assets(&ex.state)[0];
        assert_eq!(a["item"], json!("rice"));
        assert_eq!(a["source_tx"], json!("msg-1"));
        assert_eq!(a["pocket"], json!("food"));
    }

    #[test]
    fn a_basket_becomes_things_the_household_holds() {
        let ex = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("limit", json!(90.0)),
                ("lines", lines(&[("beans", 30.0), ("onions", 50.0), ("carrots", 10.0)])),
            ],
        );
        assert!(ex.committed(), "{:?}", ex.result.reason);
        let a = assets(&ex.state);
        assert_eq!(a.len(), 3);
        assert_eq!(a[0]["item"], json!("beans"));
        assert_eq!(a[0]["pocket"], json!("food"));
        assert_eq!(a[0]["source_tx"], json!("msg-1"));
        assert_eq!(ex.result.data["value"].as_f64(), Some(90.0));
    }

    #[test]
    fn itemizing_moves_no_money() {
        // ★★★ The property everything rests on. The spend already took it out
        //     of the account; if this took it again the same shilling would
        //     leave twice.
        let before = household();
        let ex = call(
            &before,
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("limit", json!(90.0)),
                ("lines", lines(&[("beans", 90.0)])),
            ],
        );
        assert!(ex.committed());
        assert_eq!(
            ex.state.pointer("/finances").unwrap(),
            before.pointer("/finances").unwrap(),
            "not one figure in finances moved"
        );
    }

    #[test]
    fn a_list_that_comes_to_more_than_was_spent_is_refused() {
        // ★★★ Otherwise value enters the household that no money paid for.
        let ex = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("limit", json!(90.0)),
                ("lines", lines(&[("beans", 60.0), ("rice", 50.0)])),
            ],
        );
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("lines_within_spend"));
        assert_eq!(ex.result.data["over"].as_f64(), Some(20.0));
        assert!(assets(&ex.state).is_empty(), "and nothing was written");
    }

    #[test]
    fn listing_only_part_of_a_purchase_is_allowed_and_says_what_is_left() {
        // ★★ A receipt he only half remembers is still worth recording. The
        //    remainder stays an ordinary spend, and the result says how much,
        //    so a partial list is visible rather than silently complete.
        let ex = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("limit", json!(90.0)),
                ("lines", lines(&[("beans", 30.0)])),
            ],
        );
        assert!(ex.committed());
        assert_eq!(ex.result.data["unlisted"].as_f64(), Some(60.0));
    }

    #[test]
    fn the_same_purchase_cannot_be_itemized_twice() {
        // ★★★ Running it again would double the shopping while the spend
        //     stayed exactly where it was.
        let first = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("lines", lines(&[("beans", 30.0)])),
            ],
        );
        assert!(first.committed());
        let second = call(
            &first.state,
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("lines", lines(&[("beans", 30.0)])),
            ],
        );
        assert!(!second.committed());
        assert_eq!(second.result.constraint_violated.as_deref(), Some("not_already_itemized"));
        assert_eq!(assets(&second.state).len(), 1, "still one");
    }

    #[test]
    fn a_line_missing_its_name_or_amount_is_named_in_the_refusal() {
        let no_name = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("lines", json!([{"item": "beans", "value": 30.0}, {"item": "", "value": 5.0}])),
            ],
        );
        assert!(!no_name.committed());
        assert!(no_name.result.reason.as_deref().unwrap().contains("Line 2"));

        let no_amount = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("lines", json!([{"item": "onions", "value": 0.0}])),
            ],
        );
        assert!(!no_amount.committed());
        assert!(no_amount.result.reason.as_deref().unwrap().contains("onions"));
    }

    #[test]
    fn a_line_can_name_a_subpocket_to_group_it_under() {
        let ex = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("lines", json!([{"item": "onions", "value": 50.0, "subpocket": "vegetables"}])),
            ],
        );
        assert!(ex.committed());
        assert_eq!(assets(&ex.state)[0]["subpocket"], json!("vegetables"));
    }

    #[test]
    fn a_pocket_that_does_not_exist_is_refused() {
        let ex = call(
            &household(),
            &[
                ("pocket_name", json!("ghost")),
                ("source_tx", json!("msg-1")),
                ("lines", lines(&[("beans", 30.0)])),
            ],
        );
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("pocket_exists"));
    }

    #[test]
    fn supplies_with_no_purchase_behind_them_are_refused() {
        // ★★ An asset that cannot say where it came from breaks the link the
        //    pocket view is built on, and there is no way to get it back.
        let ex = call(
            &household(),
            &[("pocket_name", json!("food")), ("lines", lines(&[("beans", 30.0)]))],
        );
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("source_tx_present"));
    }

    #[test]
    fn a_household_that_predates_inventory_gains_it_on_first_use() {
        // ★★★ His real situation. Every household on the phone was opened
        //     before this dimension existed, so its state has no `inventory`
        //     key at all. If the first itemize refused, the feature would be
        //     unreachable for exactly the people who need it — and a migration
        //     to add an empty list everywhere would be a write to every
        //     household to record nothing.
        let mut older = household();
        older.as_object_mut().unwrap().remove("inventory");
        assert!(older.pointer("/inventory").is_none(), "as it really is on disk");

        let ex = call(
            &older,
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-1")),
                ("limit", json!(90.0)),
                ("lines", lines(&[("beans", 30.0), ("onions", 50.0)])),
            ],
        );
        assert!(ex.committed(), "{:?}", ex.result.reason);
        assert_eq!(assets(&ex.state).len(), 2, "the list is created on the way in");
    }

    #[test]
    fn ids_are_derived_so_replaying_the_log_produces_the_same_assets() {
        // ★★★ The core has no random source and must not pretend to: a fold
        //     has to land byte-identical every time it runs.
        let a = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-7")),
                ("lines", lines(&[("beans", 30.0), ("rice", 20.0)])),
            ],
        );
        let b = call(
            &household(),
            &[
                ("pocket_name", json!("food")),
                ("source_tx", json!("msg-7")),
                ("lines", lines(&[("beans", 30.0), ("rice", 20.0)])),
            ],
        );
        assert_eq!(assets(&a.state), assets(&b.state));
        assert_eq!(assets(&a.state)[1]["id"], json!("msg-7-1"));
    }
}
