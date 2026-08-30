//! Double entry — every transaction is postings that sum to zero.
//!
//! ★★★ **The rule the money model settles on.** Money is never created or
//! destroyed, only moved between accounts. A posting is a `+` or `−` to one
//! account; the postings of one transaction always net to zero; and what moves
//! net worth is only *which kinds* of account moved.
//!
//! Until now each entry was single-sided — a spend simply lowered a pocket —
//! with a balance check that lived in the tests. That held right up until
//! Fuliza, where a borrow is two things at once: cash arriving AND a debt
//! opening. A single-sided model cannot say that, and a model that cannot say
//! it will quietly get it wrong.
//!
//! ## ★★★ What is actually checked, and why it is the stronger question
//!
//! An operator declares its money moves as [`Movement`]s, and a movement is
//! already two-sided: `from` loses, `to` gains. So "do the postings sum to
//! zero" is true by construction and checking it would prove nothing.
//!
//! The question worth asking is the other one:
//!
//! > **Does what actually changed match what was declared?**
//!
//! An operator that lowers a balance without declaring where the money went is
//! exactly the bug double entry exists to catch, and it is invisible to a
//! sum-to-zero check. So the gate reconciles the real mutations against the
//! declared movements, per account, and refuses on a mismatch.
//!
//! ## ★★ Money paths, and why the list is explicit
//!
//! Only some of a Sustain's state is money. `finances.pockets.food.limit` is a
//! declared intention, not a holding; `inventory.assets[i].value` is. The set
//! is named here rather than inferred from the shape of a number, because
//! guessing which figures are money is how a rename silently turns a real
//! check into a vacuous one.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::flow::Movement;
use crate::mutation::Mutation;
use crate::state::State;

/// What kind of thing an account is. Net worth depends only on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountKind {
    /// Where money actually sits: M-Pesa, KCB, cash in hand, M-Shwari.
    Cash,
    /// What money is *for*: food, rent, airtime. An envelope, not a holding.
    Pocket,
    /// A running tab with a person or vendor. Positive: they owe you.
    Person,
    /// Money you owe. Fuliza, a loan.
    Liability,
    /// Goods you hold.
    Inventory,
    /// Money arriving from outside. Raises net worth.
    Income,
    /// Money consumed. Lowers net worth.
    Expense,
    /// Unearmarked money inside the household — the balancing figure between
    /// what an account holds and what a pocket has claimed.
    Liquid,
}

impl AccountKind {
    /// Does a positive balance here add to what the household is worth?
    ///
    /// ★★ Pockets and liquid are deliberately NOT counted: they describe the
    /// same shillings a cash account already holds. Counting both would double
    /// every figure the household owns.
    pub fn counts_toward_net_worth(self) -> bool {
        matches!(self, Self::Cash | Self::Person | Self::Inventory)
    }

    /// Does a positive balance here subtract from it?
    pub fn is_owed(self) -> bool {
        matches!(self, Self::Liability)
    }
}

/// One side of a transaction: an account, and how much it moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    pub account: String,
    pub kind: AccountKind,
    pub delta: f64,
}

/// Which kind of account a state path names, or `None` if it is not money.
pub fn kind_of(path: &str) -> Option<AccountKind> {
    let p = path.trim();
    if p == "finances.liquid.balance" {
        return Some(AccountKind::Liquid);
    }
    if p.starts_with("finances.accounts.") && p.ends_with(".balance") {
        return Some(AccountKind::Cash);
    }
    if p.starts_with("finances.liabilities.") && p.ends_with(".balance") {
        return Some(AccountKind::Liability);
    }
    if p.starts_with("finances.people.") && p.ends_with(".balance") {
        return Some(AccountKind::Person);
    }
    // ★ `allocated` and `spent` are the two figures a pocket really has;
    //   `limit` is a declared intention and holds nothing.
    if p.starts_with("finances.pockets.")
        && (p.ends_with(".allocated") || p.ends_with(".spent"))
    {
        return Some(AccountKind::Pocket);
    }
    if p.starts_with("inventory.assets") && p.ends_with(".value") {
        return Some(AccountKind::Inventory);
    }
    // ★★★ `income.monthly_total` is a TALLY, not a balance. It records how
    //     much has arrived, and money does not sit in it — the shillings it
    //     counts are in a cash account. Treating it as a ledger account would
    //     make every income look like two arrivals.
    //
    //     The Income side of the entry is the outside party the money came
    //     from, which is external to the household by definition.
    None
}

/// The account name a money path belongs to.
///
/// ★ A pocket's `allocated` and `spent` are two readings of ONE account, so
/// they share a name. Otherwise a reclassification would look like four
/// unrelated moves instead of two.
pub fn account_of(path: &str) -> String {
    let p = path.trim();
    for suffix in [".balance", ".allocated", ".spent", ".value"] {
        if let Some(stem) = p.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    p.to_string()
}

fn as_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
}

/// Is this path part of the LOCATION ledger — the one double entry balances?
///
/// ★★★ The distinction the model turns on. A household keeps two readings of
/// the same shillings:
///
/// - **where the money is** — cash accounts, what people owe, what is owed,
///   goods held. This is the ledger, and it must balance.
/// - **what the money is for** — liquid and pockets. An envelope view over
///   money a cash account already holds.
///
/// Balancing both together would count every shilling twice: allocating to a
/// pocket would look like acquiring money. The purpose view has its own law —
/// `Σ accounts + unaccounted == liquid + Σ(allocated − spent)` — and it is
/// checked where it belongs, not here.
fn is_ledger_account(kind: AccountKind) -> bool {
    !matches!(kind, AccountKind::Pocket | AccountKind::Liquid)
}

/// What actually changed, per money account, from the recorded mutations.
///
/// ★★★ Read from `old` and `new` on the record rather than from the operator's
/// intentions. The record is what the fold will replay, so it is the only
/// account of the transaction that will still be true tomorrow.
pub fn observed(mutations: &[Mutation]) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for m in mutations {
        let Mutation::Set { path, old, new } = m else {
            // ★ Append and remove move whole items, not balances. An appended
            //   inventory asset is handled by its own posting rather than by
            //   differencing a value that had no previous reading.
            continue;
        };
        match kind_of(path) {
            Some(k) if is_ledger_account(k) => {}
            // Not money, or the envelope view rather than the ledger.
            _ => continue,
        }
        let before = as_f64(old).unwrap_or(0.0);
        let after = as_f64(new).unwrap_or(0.0);
        let delta = after - before;
        if delta.abs() < 0.0000001 {
            continue;
        }
        *out.entry(account_of(path)).or_insert(0.0) += delta;
    }
    out
}

/// What the operator SAID it was doing, per account.
pub fn declared(movements: &[Movement]) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for m in movements {
        if m.kind != "money" {
            continue;
        }
        if is_ledger_endpoint(&m.from) {
            *out.entry(m.from.clone()).or_insert(0.0) -= m.qty;
        }
        if is_ledger_endpoint(&m.to) {
            *out.entry(m.to.clone()).or_insert(0.0) += m.qty;
        }
    }
    out
}

/// Is this endpoint an account whose balance this household keeps?
///
/// ★★★ Three kinds of endpoint, and only one of them balances here:
///
/// - **A ledger account** — a cash account, a liability, a person's tab, an
///   inventory asset. Checked.
/// - **The envelope view** — `finances.liquid`, a pocket. Real, but the other
///   reading of money a cash account already holds, so it has its own law.
/// - **Anything else** — a shop, an employer, "unknown". These are OUTSIDE the
///   household, and they are exactly what makes a spend two-sided. A boundary
///   crossing is the firewall's question, not the ledger's; flagging it here
///   would call every real payment an imbalance.
fn is_ledger_endpoint(name: &str) -> bool {
    for suffix in [".balance", ".allocated", ".spent", ".value"] {
        if let Some(k) = kind_of(&format!("{name}{suffix}")) {
            return is_ledger_account(k);
        }
    }
    false
}

/// Where a reconciliation disagreed.
#[derive(Debug, Clone, PartialEq)]
pub struct Imbalance {
    pub account: String,
    /// What the state actually did.
    pub observed: f64,
    /// What the movements said it would do.
    pub declared: f64,
}

impl std::fmt::Display for Imbalance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "'{}' moved by {:.2} but the transaction declared {:.2}",
            self.account, self.observed, self.declared
        )
    }
}

/// How close two figures must be to count as the same money.
///
/// ★ Half a cent. Floating point addition of shillings drifts in the last
/// place, and refusing a real transaction over 1e-13 would be a worse bug than
/// the one this catches.
const TOLERANCE: f64 = 0.005;

/// Reconcile what happened against what was declared.
///
/// ★★★ Accounts the operator did not mention are the point. An operator that
/// lowers a balance and declares nothing produces an observed move with no
/// declared counterpart, which is exactly the single-sided entry double entry
/// exists to prevent.
pub fn reconcile(mutations: &[Mutation], movements: &[Movement]) -> Vec<Imbalance> {
    let obs = observed(mutations);
    let dec = declared(movements);
    let mut names: Vec<&String> = obs.keys().chain(dec.keys()).collect();
    names.sort();
    names.dedup();

    let mut out = Vec::new();
    for name in names {
        let o = *obs.get(name).unwrap_or(&0.0);
        let d = *dec.get(name).unwrap_or(&0.0);
        if (o - d).abs() > TOLERANCE {
            out.push(Imbalance {
                account: name.clone(),
                observed: (o * 100.0).round() / 100.0,
                declared: (d * 100.0).round() / 100.0,
            });
        }
    }
    out
}

/// The postings of a transaction, as the ledger sees them.
pub fn postings(mutations: &[Mutation]) -> Vec<Posting> {
    observed(mutations)
        .into_iter()
        .filter_map(|(account, delta)| {
            // Recover the kind from any path that named this account.
            let kind = mutations.iter().find_map(|m| {
                let Mutation::Set { path, .. } = m else { return None };
                if account_of(path) == account { kind_of(path) } else { None }
            })?;
            Some(Posting { account, kind, delta })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn set(path: &str, old: f64, new: f64) -> Mutation {
        Mutation::Set { path: path.into(), old: json!(old), new: json!(new) }
    }

    fn mv(qty: f64, from: &str, to: &str) -> Movement {
        Movement::new("money", qty, from, to)
    }

    #[test]
    fn a_path_is_money_only_if_it_names_a_holding() {
        assert_eq!(kind_of("finances.accounts.mpesa.balance"), Some(AccountKind::Cash));
        assert_eq!(kind_of("finances.liabilities.fuliza.balance"), Some(AccountKind::Liability));
        assert_eq!(kind_of("finances.people.aida.balance"), Some(AccountKind::Person));
        assert_eq!(kind_of("finances.pockets.food.spent"), Some(AccountKind::Pocket));
        assert_eq!(kind_of("inventory.assets[0].value"), Some(AccountKind::Inventory));
        // ★★ A limit is an intention. It holds nothing, and counting it would
        //    make declaring a budget look like acquiring money.
        assert_eq!(kind_of("finances.pockets.food.limit"), None);
        assert_eq!(kind_of("device.queue_depth"), None);
        assert_eq!(kind_of("identity.name"), None);
    }

    #[test]
    fn a_pocket_is_one_account_however_many_figures_it_keeps() {
        // ★★ Otherwise a reclassification reads as four unrelated moves.
        assert_eq!(account_of("finances.pockets.food.allocated"), "finances.pockets.food");
        assert_eq!(account_of("finances.pockets.food.spent"), "finances.pockets.food");
    }

    #[test]
    fn the_envelope_view_is_not_part_of_the_ledger() {
        // ★★★ Liquid and pockets describe the SAME shillings a cash account
        //     already holds. Balancing them here as well would count every
        //     transaction twice, and allocating to a pocket would read as
        //     acquiring money. Their own law lives with the accounts model.
        let obs = observed(&[
            set("finances.pockets.food.spent", 0.0, 90.0),
            set("finances.liquid.balance", 1000.0, 910.0),
        ]);
        assert!(obs.is_empty(), "the envelope view stayed out of it: {obs:?}");
    }

    #[test]
    fn allocating_to_a_pocket_is_not_a_transaction_at_all() {
        // ★★ Nothing left the household; money was earmarked. The ledger has
        //    nothing to say about it, and that is correct.
        let muts = vec![
            set("finances.liquid.balance", 1000.0, 700.0),
            set("finances.pockets.food.allocated", 0.0, 300.0),
        ];
        assert_eq!(reconcile(&muts, &[]), vec![]);
    }

    #[test]
    fn a_transaction_that_says_what_it_did_reconciles() {
        // A spend: the account really falls, and the movement says where it went.
        let muts = vec![set("finances.accounts.mpesa.balance", 1000.0, 900.0)];
        let movs = vec![mv(100.0, "finances.accounts.mpesa", "NAIVAS")];
        assert_eq!(
            reconcile(&muts, &movs),
            vec![],
            "the shop is outside the household; it is the counterpart, not an imbalance"
        );
    }

    #[test]
    fn an_outside_party_is_never_an_imbalance() {
        // ★★★ A spend IS two-sided: the household loses, the world gains. The
        //     outside end has no balance here and demanding one would call
        //     every real payment broken.
        let muts = vec![set("finances.accounts.mpesa.balance", 500.0, 400.0)];
        for payee in ["NAIVAS SUPERMARKET", "unknown", "an employer"] {
            assert_eq!(
                reconcile(&muts, &[mv(100.0, "finances.accounts.mpesa", payee)]),
                vec![],
                "{payee} was treated as an internal account"
            );
        }
    }

    #[test]
    fn income_is_the_arrival_not_the_tally_that_counts_it() {
        // ★★★ `income.monthly_total` records how much has arrived; the money
        //     itself is in a cash account. Counting the tally as an account
        //     would make one arrival look like two.
        let muts = vec![
            set("finances.accounts.mpesa.balance", 0.0, 500.0),
            set("finances.income.monthly_total", 0.0, 500.0),
        ];
        let movs = vec![mv(500.0, "an employer", "finances.accounts.mpesa")];
        assert_eq!(reconcile(&muts, &movs), vec![]);
    }

    #[test]
    fn a_transfer_between_two_of_his_own_accounts_reconciles() {
        let muts = vec![
            set("finances.accounts.kcb.balance", 5000.0, 3000.0),
            set("finances.accounts.mpesa.balance", 0.0, 2000.0),
        ];
        let movs = vec![mv(2000.0, "finances.accounts.kcb", "finances.accounts.mpesa")];
        assert_eq!(reconcile(&muts, &movs), vec![]);
    }

    #[test]
    fn money_that_moved_without_being_declared_is_caught() {
        // ★★★ The single-sided entry, which is the whole reason this exists.
        //     A sum-to-zero check cannot see it: there is only one side.
        let muts = vec![set("finances.accounts.mpesa.balance", 1000.0, 900.0)];
        let out = reconcile(&muts, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].account, "finances.accounts.mpesa");
        assert_eq!(out[0].observed, -100.0);
        assert_eq!(out[0].declared, 0.0);
    }

    #[test]
    fn money_declared_but_never_moved_is_caught_too() {
        // ★★ The mirror: an operator that promises a move and does not make it.
        let out = reconcile(
            &[],
            &[mv(50.0, "finances.accounts.kcb", "finances.accounts.mpesa")],
        );
        assert_eq!(out.len(), 2, "both ends are unaccounted for");
    }

    #[test]
    fn a_declared_move_of_the_wrong_size_is_caught() {
        let muts = vec![set("finances.accounts.mpesa.balance", 1000.0, 700.0)];
        // Says 250, actually moved 300.
        let out = reconcile(&muts, &[mv(250.0, "finances.accounts.mpesa", "SHOP")]);
        assert_eq!(out.len(), 1, "the account moved 300 and only 250 was declared");
        assert_eq!(out[0].account, "finances.accounts.mpesa");
    }

    #[test]
    fn a_transaction_touching_no_money_reconciles_trivially() {
        // ★ Renaming a pocket or recording a heartbeat is not a transaction,
        //   and must not be made to look like one.
        let muts = vec![
            Mutation::Set { path: "device.queue_depth".into(), old: json!(0), new: json!(3) },
            Mutation::Set { path: "finances.pockets.food.limit".into(), old: json!(0.0), new: json!(500.0) },
        ];
        assert_eq!(reconcile(&muts, &[]), vec![]);
    }

    #[test]
    fn rounding_in_the_last_place_is_not_an_imbalance() {
        let muts = vec![
            set("finances.accounts.kcb.balance", 1000.0, 1000.0 - 33.333333333),
            set("finances.accounts.mpesa.balance", 0.0, 33.333333333),
        ];
        let movs = vec![mv(33.333333333, "finances.accounts.kcb", "finances.accounts.mpesa")];
        assert_eq!(reconcile(&muts, &movs), vec![]);
    }

    #[test]
    fn only_the_kinds_that_hold_value_count_toward_net_worth() {
        // ★★★ Pockets and liquid describe the SAME shillings a cash account
        //     already holds. Counting them would double everything owned.
        assert!(AccountKind::Cash.counts_toward_net_worth());
        assert!(AccountKind::Person.counts_toward_net_worth());
        assert!(AccountKind::Inventory.counts_toward_net_worth());
        assert!(!AccountKind::Pocket.counts_toward_net_worth());
        assert!(!AccountKind::Liquid.counts_toward_net_worth());
        assert!(AccountKind::Liability.is_owed());
    }

    #[test]
    fn a_fuliza_borrow_is_two_postings_and_moves_no_net_worth() {
        // ★★★ The case that broke the single-sided model: cash arriving and a
        //     debt opening, in one transaction.
        let muts = vec![
            set("finances.accounts.mpesa.balance", 0.0, 500.0),
            set("finances.liabilities.fuliza.balance", 0.0, 500.0),
        ];
        let ps = postings(&muts);
        assert_eq!(ps.len(), 2);
        let cash = ps.iter().find(|p| p.kind == AccountKind::Cash).expect("cash side");
        let debt = ps.iter().find(|p| p.kind == AccountKind::Liability).expect("debt side");
        assert_eq!(cash.delta, 500.0);
        assert_eq!(debt.delta, 500.0);
        // Held goes up by 500, owed goes up by 500: net worth unchanged.
        let held: f64 = ps.iter().filter(|p| p.kind.counts_toward_net_worth()).map(|p| p.delta).sum();
        let owed: f64 = ps.iter().filter(|p| p.kind.is_owed()).map(|p| p.delta).sum();
        assert_eq!(held - owed, 0.0);
    }
}

// ── the household's position ────────────────────────────────────────────────

/// One line of the household's position, named so it can be shown.
#[derive(Debug, Clone, PartialEq)]
pub struct Holding {
    pub name: String,
    pub kind: AccountKind,
    pub amount: f64,
}

/// **What the household is actually worth**, not just what is in the bank.
///
/// ★★★ Money out is not money gone. A week's shopping leaves the account and
/// becomes food in the cupboard; a payment to somebody who will pay it back
/// leaves the account and becomes a claim. A position that counted only cash
/// would call both of those a loss, and a household that shops well would look
/// identical to one losing money — which is the exact failure `inventory` was
/// added to fix, finished here.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Position {
    /// Cash in accounts, plus anything unaccounted still sitting in liquid.
    pub cash: f64,
    /// Things the household holds, at the value it recorded for them.
    pub things: f64,
    /// Money other people owe it.
    pub owed_to_you: f64,
    /// Money it owes — debts, plus any person tab standing in their favour.
    pub owed_by_you: f64,
    /// Every line, so a total can always be taken apart.
    pub lines: Vec<Holding>,
}

impl Position {
    /// `cash + things + owed_to_you − owed_by_you`.
    pub fn net(&self) -> f64 {
        self.cash + self.things + self.owed_to_you - self.owed_by_you
    }
}

fn number_at(v: &Value, path: &[&str]) -> Option<f64> {
    let mut cur = v;
    for seg in path {
        cur = cur.get(seg)?;
    }
    cur.as_f64()
}

/// Read the household's position out of its own state.
///
/// ★★★ **A person pocket is a claim, and which way it points decides which
/// side it lands on.** Sending somebody money turns cash into a receivable, so
/// the position does not move — correctly, because nothing was lost. Their
/// paying it back turns the receivable into cash, and again nothing moves.
/// A tab standing in THEIR favour is money he is holding that is not his, and
/// it lands on the other side.
///
/// ★★ ORDINARY pockets are not counted at all. An envelope is a view of money
/// that is already in an account; adding it would count the same shilling
/// twice. Only pockets tied to a person — which are claims, not envelopes —
/// contribute, and `is_person_pocket` is what tells them apart.
pub fn position(state: &Value) -> Position {
    let mut p = Position::default();
    let st = State::new(state.clone());

    // Cash the household holds.
    if let Some(accounts) = state.pointer("/finances/accounts").and_then(Value::as_object) {
        for (name, account) in accounts {
            let Some(amount) = account.get("balance").and_then(Value::as_f64) else { continue };
            p.cash += amount;
            p.lines.push(Holding { name: name.clone(), kind: AccountKind::Cash, amount });
        }
    }
    // ★★ Money in the pooled balance that no account has claimed yet is still
    //    money. Leaving it out would understate the position by exactly the
    //    amount nobody has got round to attributing.
    if let Some(liquid) = number_at(state, &["finances", "liquid", "balance"]) {
        let unaccounted = liquid - p.cash;
        if unaccounted.abs() > TOLERANCE {
            p.cash += unaccounted;
            p.lines.push(Holding {
                name: "unaccounted".into(),
                kind: AccountKind::Liquid,
                amount: unaccounted,
            });
        }
    }

    // Things it holds.
    if let Some(assets) = state.pointer("/inventory/assets").and_then(Value::as_array) {
        for asset in assets {
            let Some(amount) = asset.get("value").and_then(Value::as_f64) else { continue };
            let name = asset
                .get("item")
                .and_then(Value::as_str)
                .unwrap_or("something")
                .to_string();
            p.things += amount;
            p.lines.push(Holding { name, kind: AccountKind::Inventory, amount });
        }
    }

    // Claims, either way round.
    if let Some(pockets) = state.pointer("/finances/pockets").and_then(Value::as_object) {
        for (name, pocket) in pockets {
            if !crate::operator::vendor::is_person_pocket(&st, name) {
                continue;
            }
            let outstanding = pocket.get("spent").and_then(Value::as_f64).unwrap_or(0.0);
            if outstanding.abs() <= TOLERANCE {
                continue;
            }
            if outstanding > 0.0 {
                p.owed_to_you += outstanding;
            } else {
                p.owed_by_you += -outstanding;
            }
            p.lines.push(Holding {
                name: name.clone(),
                kind: AccountKind::Person,
                amount: outstanding,
            });
        }
    }

    // Debts.
    if let Some(debts) = state.pointer("/finances/liabilities").and_then(Value::as_object) {
        for (name, debt) in debts {
            let Some(amount) = debt.get("balance").and_then(Value::as_f64) else { continue };
            p.owed_by_you += amount;
            p.lines.push(Holding { name: name.clone(), kind: AccountKind::Liability, amount });
        }
    }

    p
}

#[cfg(test)]
mod position_tests {
    use super::*;
    use serde_json::json;

    fn household() -> Value {
        json!({
            "finances": {
                "liquid": {"balance": 5000.0},
                "accounts": {"mpesa": {"balance": 5000.0}},
                "pockets": {"food": {"allocated": 2000.0, "spent": 500.0},
                            "Aida": {"allocated": 3000.0, "spent": 0.0}},
                "links": {"726123961": "Aida"}
            },
            "inventory": {"assets": []}
        })
    }

    #[test]
    fn an_envelope_is_never_counted_beside_the_money_it_holds() {
        // ★★★ A pocket is a VIEW of money already in an account. Counting both
        //     would count the same shilling twice, and the total would grow
        //     every time he budgeted — which is the opposite of what budgeting
        //     does.
        let p = position(&household());
        assert_eq!(p.cash, 5000.0);
        assert_eq!(p.net(), 5000.0);
        assert!(p.lines.iter().all(|l| l.name != "food"));
    }

    #[test]
    fn money_spent_on_things_the_household_still_has_is_not_a_loss() {
        // ★★★ The whole reason inventory exists, finished. Without this a
        //     household that shops well looks identical to one losing money.
        let mut s = household();
        s["finances"]["accounts"]["mpesa"]["balance"] = json!(4_000.0);
        s["finances"]["liquid"]["balance"] = json!(4_000.0);
        s["inventory"]["assets"] = json!([{"item": "rice", "value": 1_000.0}]);
        let p = position(&s);
        assert_eq!(p.things, 1_000.0);
        assert_eq!(p.net(), 5_000.0, "the shopping moved, it did not vanish");
    }

    #[test]
    fn lending_to_somebody_moves_the_position_nowhere() {
        // ★★★ Cash became a claim. If this moved, every loan would read as a
        //     loss and every repayment as income.
        let mut s = household();
        s["finances"]["accounts"]["mpesa"]["balance"] = json!(3_000.0);
        s["finances"]["liquid"]["balance"] = json!(3_000.0);
        s["finances"]["pockets"]["Aida"]["spent"] = json!(2_000.0);
        let p = position(&s);
        assert_eq!(p.owed_to_you, 2_000.0);
        assert_eq!(p.net(), 5_000.0);
    }

    #[test]
    fn a_tab_standing_in_their_favour_is_money_he_owes() {
        // ★★ He is holding it, and it is not his.
        let mut s = household();
        s["finances"]["accounts"]["mpesa"]["balance"] = json!(6_000.0);
        s["finances"]["liquid"]["balance"] = json!(6_000.0);
        s["finances"]["pockets"]["Aida"]["spent"] = json!(-1_000.0);
        let p = position(&s);
        assert_eq!(p.owed_by_you, 1_000.0);
        assert_eq!(p.net(), 5_000.0);
    }

    #[test]
    fn a_debt_lowers_the_position_by_exactly_what_is_owed() {
        let mut s = household();
        s["finances"]["liabilities"] = json!({"fuliza": {"balance": 1_500.0}});
        assert_eq!(position(&s).net(), 3_500.0);
    }

    #[test]
    fn money_no_account_has_claimed_is_still_money() {
        // ★★ Otherwise the position understates by exactly the amount nobody
        //    has got round to attributing.
        let mut s = household();
        s["finances"]["accounts"] = json!({});
        let p = position(&s);
        assert_eq!(p.cash, 5_000.0);
        assert!(p.lines.iter().any(|l| l.name == "unaccounted"));
    }

    #[test]
    fn every_total_can_be_taken_apart() {
        // ★★★ A single number nobody can explain is a number nobody should
        //     act on. Each side is the sum of its own named lines.
        let mut s = household();
        s["inventory"]["assets"] = json!([{"item": "rice", "value": 200.0}]);
        s["finances"]["pockets"]["Aida"]["spent"] = json!(300.0);
        let p = position(&s);
        let summed = |k: AccountKind| -> f64 {
            p.lines.iter().filter(|l| l.kind == k).map(|l| l.amount).sum()
        };
        assert_eq!(summed(AccountKind::Inventory), p.things);
        assert_eq!(summed(AccountKind::Person), p.owed_to_you - p.owed_by_you);
    }
}
