//! **Catching a confident mistake** — judging what was filed without being asked.
//!
//! ★★★ **Why this exists.** Automation earns its keep by not asking, and every
//! step of that earning raises the same stake: income files itself, a taught
//! shape applies to its whole cluster, a route pre-fills a pocket. Each is a
//! decision made on a household's books that nobody looked at. The failure mode
//! is not a crash — it is a wrong number landing quietly and staying, and being
//! discovered a month later as an inexplicable balance.
//!
//! So nothing here slows the automation down. It reads what was already done
//! and says which of it is worth a second look.
//!
//! ★★★ **It states doubts, never verdicts.** Not one function in this module
//! can tell whether a filing was right; only the person can. What it can do is
//! notice that a filing disagrees with something else already known — the
//! message's own words, the shape it was read by, the account's own stated
//! balance — and say so, in the words of the disagreement. A guard that
//! declared things "wrong" would be inventing an authority it does not have,
//! and its false positives would train him to ignore it.
//!
//! ★★ **Every doubt is a contradiction between two real readings**, never a
//! model's opinion and never a threshold picked to look busy. Each variant
//! below names both sides of its disagreement, so a person can see what
//! disagreed with what without trusting this module at all.
//!
//! ★ **Silence is a real answer.** Too little history to compare against
//! produces no doubt rather than a hedged one. A guard that flags everything
//! is the same as a guard that flags nothing, only louder.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What was filed, and what the message it came from actually said.
///
/// ★★ Deliberately a plain description rather than the host's message type:
/// this module is a judgement about data, and taking the store's struct would
/// tie a pure decision to how a particular node happens to keep its files.
#[derive(Debug, Clone, PartialEq)]
pub struct Filed<'a> {
    pub message_id: &'a str,
    /// The operator that ran, e.g. `budget.record_income`.
    pub operator: &'a str,
    /// The params it ran with.
    pub params: &'a BTreeMap<String, serde_json::Value>,
    /// What the transducer read out of the text.
    pub parsed: &'a BTreeMap<String, serde_json::Value>,
    /// Where the rule that read it says each figure belongs, if it was taught.
    pub routes: &'a [(String, String)],
    /// The account's previously known balance, if one is known.
    pub previous_balance: Option<f64>,
}

/// One disagreement worth a person's eye.
///
/// ★★ Each variant carries BOTH sides. "This looks odd" is not actionable; "the
/// text says money arrived and this was filed as a spend" is something he can
/// read once and settle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Doubt {
    /// The text says money went one way; the filing moved it the other.
    ///
    /// ★★★ The most consequential thing that can silently go wrong, because it
    /// is wrong by twice the amount: an arrival booked as a spend takes the sum
    /// off the books that should have been added to it.
    DirectionDisagrees { text_says: String, filed_as: String },
    /// The amount is far outside what this shape has ever filed before.
    ///
    /// ★★ Stated as a comparison, never as "suspicious". A hundred-fold jump on
    /// a shape that has been steady for months is worth a glance; the household
    /// decides whether it was a real large payment.
    AmountUnlikeItsShape { amount: f64, usual: f64, times: f64 },
    /// A taught shape says this figure belongs somewhere else.
    PocketUnlikeTheTaughtRoute { filed_to: String, taught: String },
    /// The account's own stated balance moved the opposite way to the filing.
    ///
    /// ★★★ The strongest signal available, and it costs nothing: the message
    /// states the balance AFTER the movement. If the books say money arrived
    /// and the bank says the account holds less than it did, one of them is
    /// wrong, and it is not the bank.
    BalanceContradicts { filed_as: String, was: f64, now: f64 },
}

impl Doubt {
    /// What to say to a person, in the words of the disagreement itself.
    pub fn say(&self) -> String {
        match self {
            Self::DirectionDisagrees { text_says, filed_as } => format!(
                "the message says money {text_says}, but this was filed as {filed_as}"
            ),
            Self::AmountUnlikeItsShape { amount, usual, times } => format!(
                "Ksh {amount:.0} is about {times:.0}x the usual Ksh {usual:.0} for messages like this"
            ),
            Self::PocketUnlikeTheTaughtRoute { filed_to, taught } => {
                format!("this went to {filed_to}, but you taught this shape to use {taught}")
            }
            Self::BalanceContradicts { filed_as, was, now } => format!(
                "filed as {filed_as}, but the account says it went from Ksh {was:.0} to Ksh {now:.0}"
            ),
        }
    }
}

/// How many prior amounts a shape needs before an outlier means anything.
///
/// ★★★ Below this there is no "usual" to be unlike. Calling the second message
/// of a shape an outlier because it differs from the first is noise dressed as
/// vigilance, and noise is how a guard gets ignored.
pub const ENOUGH_TO_COMPARE: usize = 5;

/// How far from usual is far enough to mention.
///
/// ★★ Generous on purpose. Households have irregular months, and this fires on
/// a genuine order-of-magnitude departure rather than on ordinary variation.
pub const UNLIKE_FACTOR: f64 = 10.0;

fn num(v: Option<&serde_json::Value>) -> Option<f64> {
    v.and_then(|x| {
        x.as_f64().or_else(|| x.as_str().and_then(|s| s.replace(',', "").parse().ok()))
    })
}

fn text<'a>(m: &'a BTreeMap<String, serde_json::Value>, k: &str) -> Option<&'a str> {
    m.get(k).and_then(serde_json::Value::as_str)
}

/// Which way a filing moves money, in a word a person would use.
fn direction_of(operator: &str) -> Option<&'static str> {
    match operator {
        "budget.record_income" => Some("in"),
        "budget.spend" => Some("out"),
        _ => None,
    }
}

/// **Everything that disagrees about one automatic filing.**
///
/// `prior_amounts` are the amounts this same shape has filed before, so an
/// outlier is measured against the shape's own history rather than against a
/// number chosen in advance.
pub fn doubts(f: &Filed, prior_amounts: &[f64]) -> Vec<Doubt> {
    let mut out = Vec::new();
    let filed_as = direction_of(f.operator);

    // ── the text's own word against the filing ───────────────────────────
    if let (Some(said), Some(did)) = (text(f.parsed, "direction"), filed_as) {
        let said_word = match said {
            "received" => Some("in"),
            "sent" => Some("out"),
            // ★ "none" is a real answer: a balance statement or a move between
            //   his own accounts has no direction to disagree with.
            _ => None,
        };
        if let Some(said_word) = said_word {
            if said_word != did {
                out.push(Doubt::DirectionDisagrees {
                    text_says: said_word.to_string(),
                    filed_as: did.to_string(),
                });
            }
        }
    }

    let amount = num(f.params.get("amount"));

    // ── the amount against what this shape usually files ─────────────────
    if let Some(amount) = amount {
        if prior_amounts.len() >= ENOUGH_TO_COMPARE && amount > 0.0 {
            let mut sorted: Vec<f64> = prior_amounts.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            // ★★ The median, not the mean. One genuine large payment in the
            //    history would drag a mean up far enough to hide the next one.
            let usual = sorted[sorted.len() / 2];
            if usual > 0.0 {
                let times = amount / usual;
                if times >= UNLIKE_FACTOR {
                    out.push(Doubt::AmountUnlikeItsShape { amount, usual, times });
                }
            }
        }
    }

    // ── the pocket against what he taught ────────────────────────────────
    if let Some(filed_to) = text(f.params, "pocket_name") {
        // The taught route for the figure that moved, when the shape has one.
        if let Some((_, taught)) = f.routes.iter().find(|(group, _)| group == "amount") {
            if !taught.eq_ignore_ascii_case(filed_to) {
                out.push(Doubt::PocketUnlikeTheTaughtRoute {
                    filed_to: filed_to.to_string(),
                    taught: taught.clone(),
                });
            }
        }
    }

    // ── the account's own stated balance against the filing ──────────────
    //
    // ★★★ Free, and the hardest to argue with. The message states what the
    //     account holds AFTER the movement.
    if let (Some(was), Some(did)) = (f.previous_balance, filed_as) {
        let now = num(f.parsed.get("balance_after"))
            .or_else(|| num(f.parsed.get("actual_balance")))
            .or_else(|| num(f.parsed.get("pochi_balance")))
            .or_else(|| num(f.parsed.get("mshwari_balance")));
        if let Some(now) = now {
            // ★ Only a movement large enough to be unambiguous. Fees and
            //   rounding move a balance the "wrong" way by pennies constantly,
            //   and flagging that would bury the real thing.
            let moved = now - was;
            let significant = amount.map(|a| a * 0.5).unwrap_or(1.0).max(1.0);
            let contradicts = match did {
                "in" => moved <= -significant,
                "out" => moved >= significant,
                _ => false,
            };
            if contradicts {
                out.push(Doubt::BalanceContradicts {
                    filed_as: did.to_string(),
                    was,
                    now,
                });
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn filed<'a>(
        operator: &'a str,
        params: &'a BTreeMap<String, serde_json::Value>,
        parsed: &'a BTreeMap<String, serde_json::Value>,
        routes: &'a [(String, String)],
        previous_balance: Option<f64>,
    ) -> Filed<'a> {
        Filed { message_id: "m1", operator, params, parsed, routes, previous_balance }
    }

    #[test]
    fn an_arrival_filed_as_a_spend_is_caught() {
        // ★★★ Wrong by twice the amount, and completely silent without this.
        let p = map(&[("amount", json!(500.0))]);
        let parsed = map(&[("direction", json!("received"))]);
        let d = doubts(&filed("budget.spend", &p, &parsed, &[], None), &[]);
        assert_eq!(
            d,
            vec![Doubt::DirectionDisagrees {
                text_says: "in".into(),
                filed_as: "out".into()
            }]
        );
        assert!(d[0].say().contains("filed as out"), "and it says both sides: {}", d[0].say());
    }

    #[test]
    fn a_filing_that_agrees_with_its_text_raises_nothing() {
        let p = map(&[("amount", json!(500.0))]);
        let parsed = map(&[("direction", json!("sent"))]);
        assert!(doubts(&filed("budget.spend", &p, &parsed, &[], None), &[]).is_empty());
    }

    #[test]
    fn a_message_with_no_direction_is_not_argued_with() {
        // ★★ "none" is what a balance statement and a move between his own
        //    accounts both say. There is nothing to disagree with.
        let p = map(&[("amount", json!(500.0))]);
        let parsed = map(&[("direction", json!("none"))]);
        assert!(doubts(&filed("budget.spend", &p, &parsed, &[], None), &[]).is_empty());
    }

    #[test]
    fn an_amount_far_outside_its_shape_is_mentioned() {
        let p = map(&[("amount", json!(50_000.0))]);
        let parsed = map(&[("direction", json!("sent"))]);
        let prior = [400.0, 500.0, 450.0, 600.0, 550.0];
        let d = doubts(&filed("budget.spend", &p, &parsed, &[], None), &prior);
        match &d[..] {
            [Doubt::AmountUnlikeItsShape { amount, usual, .. }] => {
                assert_eq!(*amount, 50_000.0);
                assert_eq!(*usual, 500.0, "the median, so one big month cannot hide the next");
            }
            other => panic!("expected one outlier doubt, got {other:?}"),
        }
    }

    #[test]
    fn too_little_history_makes_no_claim() {
        // ★★★ Silence is a real answer. Calling the second message of a shape
        //     an outlier is noise dressed as vigilance.
        let p = map(&[("amount", json!(50_000.0))]);
        let parsed = map(&[("direction", json!("sent"))]);
        assert!(doubts(&filed("budget.spend", &p, &parsed, &[], None), &[400.0, 500.0]).is_empty());
    }

    #[test]
    fn ordinary_variation_is_left_alone() {
        // ★★ A guard that flags every busy week is a guard he stops reading.
        let p = map(&[("amount", json!(1_200.0))]);
        let parsed = map(&[("direction", json!("sent"))]);
        let prior = [400.0, 500.0, 450.0, 600.0, 550.0];
        assert!(doubts(&filed("budget.spend", &p, &parsed, &[], None), &prior).is_empty());
    }

    #[test]
    fn a_pocket_that_is_not_the_one_he_taught_is_mentioned() {
        let p = map(&[("amount", json!(500.0)), ("pocket_name", json!("food"))]);
        let parsed = map(&[("direction", json!("sent"))]);
        let routes = [("amount".to_string(), "Fuliza".to_string())];
        let d = doubts(&filed("budget.spend", &p, &parsed, &routes, None), &[]);
        assert_eq!(
            d,
            vec![Doubt::PocketUnlikeTheTaughtRoute {
                filed_to: "food".into(),
                taught: "Fuliza".into()
            }]
        );
    }

    #[test]
    fn the_pocket_he_taught_raises_nothing_whatever_its_case() {
        let p = map(&[("amount", json!(500.0)), ("pocket_name", json!("fuliza"))]);
        let parsed = map(&[("direction", json!("sent"))]);
        let routes = [("amount".to_string(), "Fuliza".to_string())];
        assert!(doubts(&filed("budget.spend", &p, &parsed, &routes, None), &[]).is_empty());
    }

    #[test]
    fn a_balance_that_fell_after_an_income_is_caught() {
        // ★★★ The bank states what the account holds afterwards. If the books
        //     say money arrived and the account holds less, one of them is
        //     wrong, and it is not the bank.
        let p = map(&[("amount", json!(1_000.0))]);
        let parsed = map(&[("direction", json!("received")), ("balance_after", json!(200.0))]);
        let d = doubts(&filed("budget.record_income", &p, &parsed, &[], Some(5_000.0)), &[]);
        assert_eq!(
            d,
            vec![Doubt::BalanceContradicts { filed_as: "in".into(), was: 5_000.0, now: 200.0 }]
        );
    }

    #[test]
    fn a_balance_moving_the_way_it_should_raises_nothing() {
        let p = map(&[("amount", json!(1_000.0))]);
        let parsed = map(&[("direction", json!("received")), ("balance_after", json!(6_000.0))]);
        assert!(doubts(&filed("budget.record_income", &p, &parsed, &[], Some(5_000.0)), &[])
            .is_empty());
    }

    #[test]
    fn a_fee_sized_wobble_is_not_a_contradiction() {
        // ★★ Balances move by pennies against the direction of a movement all
        //    the time -- charges, rounding, a fee debited in the same breath.
        //    Flagging that would bury the real thing under noise.
        let p = map(&[("amount", json!(1_000.0))]);
        let parsed = map(&[("direction", json!("received")), ("balance_after", json!(5_998.0))]);
        assert!(doubts(&filed("budget.record_income", &p, &parsed, &[], Some(6_000.0)), &[])
            .is_empty());
    }

    #[test]
    fn nothing_known_means_nothing_claimed() {
        // ★ No previous balance, no history, no routes, no direction: there is
        //   nothing to disagree with, and the honest output is empty.
        let p = map(&[("amount", json!(500.0))]);
        let parsed = map(&[]);
        assert!(doubts(&filed("budget.spend", &p, &parsed, &[], None), &[]).is_empty());
    }

    #[test]
    fn several_disagreements_are_all_reported() {
        // ★★ They are independent readings. Showing one and hiding the others
        //    would let him settle the small one and never see the large one.
        let p = map(&[("amount", json!(50_000.0)), ("pocket_name", json!("food"))]);
        let parsed = map(&[("direction", json!("received")), ("balance_after", json!(10.0))]);
        let routes = [("amount".to_string(), "Fuliza".to_string())];
        let prior = [400.0, 500.0, 450.0, 600.0, 550.0];
        let d = doubts(&filed("budget.spend", &p, &parsed, &routes, Some(9_000.0)), &prior);
        assert_eq!(d.len(), 3, "direction, amount and pocket all disagree: {d:?}");
    }
}
