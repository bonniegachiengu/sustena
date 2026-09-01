//! **Correction-learning** — a human's confirmed classification becomes a rule.
//!
//! A person classifies a message the transducer did not understand. That
//! correction is the most valuable signal the system will ever get about a
//! message shape, and it should not have to be given twice.
//!
//! ```text
//! raw message + the (o, θ) a human confirmed  ──►  a candidate ParseRule
//! ```
//!
//! ## ★★★ Template synthesis, NOT generalisation
//!
//! This is not an LLM and not a learner. It is **conservative to the point of
//! being boring**: the confirmed amount's own text is located inside the raw
//! message and parameterised, a leading reference code is parameterised if
//! there is one, and **everything else is kept as a literal anchor**. The
//! candidate matches this shape and shapes that differ only in the amount.
//!
//! ★★ That is deliberately narrow. A rule that generalised further would start
//! matching messages nobody confirmed anything about — and a false match here
//! is a wrong entry in a ledger, which is exactly the failure the whole
//! transducer is arranged around. The cost of being too narrow is that a person
//! corrects a second variant once; the cost of being too broad is silent, and
//! financial.
//!
//! ★ If the confirmed amount cannot be found in the raw text at all, this
//! returns `None` rather than a rule anchored on nothing. A useless rule is
//! worse than no rule: it occupies precedence and matches by accident.
//!
//! ## ★★★ The money-safety asymmetry is inherited, not re-decided
//!
//! A learned **income** rule may be `Mapped` — money arriving is unambiguous,
//! and the same shape next month means the same thing. A learned **spend**
//! rule is `ParsedUnmapped` and carries no operator at all: *which pocket* is a
//! decision that belongs to a person every time, and a rule that auto-applied
//! it would be spending on their behalf because they once classified something
//! similar.
//!
//! ## ★★ Verified before a human ever sees it
//!
//! [`verify_candidate`] is `V(r)`, two conditions, both required:
//!
//! 1. **`Γ ⊢ r`** and the rule matches **its own inducing example**. A rule
//!    that does not recognise the message that produced it is broken by
//!    construction.
//! 2. **Regression**: it must not fire on a message an existing rule already
//!    handles. A correction that quietly captured its neighbours' messages
//!    would be a regression nobody asked for.

use std::collections::BTreeMap;

use regex::Regex;
use std::sync::OnceLock;

use crate::parse_rule::{
    apply_rule, typecheck_rule, FieldKind, FieldSpec, FigureRoute, OperatorUniverse, ParseRule,
    ParseRuleTrust, RouteRole, RuleStatus,
};

/// A leading transaction reference — `TGH4A2B9CD Confirmed…`.
fn leading_ref_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^([A-Z0-9]{8,12})\b").unwrap())
}

/// How a confirmed float might literally appear in the raw text.
///
/// ★ The raw message carries the ORIGINAL formatting — thousands separators, a
/// trailing `.00` — not a float's own rendering. Tried in order; the first one
/// actually present wins.
/// A money figure written with its currency, `Ksh 1,005.52`.
///
/// ★ Currency-prefixed on purpose. It is the one form that is unambiguously an
/// amount rather than an account fragment, a date part, or a phone number.
fn money_figure_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:ksh|kes)\.?\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)")
            .expect("a literal regex")
    })
}

fn amount_texts(amount: f64) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    if amount.fract() == 0.0 && amount.abs() < 9.0e15 {
        let i = amount as i64;
        push(format!("{}.00", with_commas(i)));
        push(with_commas(i));
        push(format!("{i}.00"));
        push(format!("{i}"));
    } else {
        push(with_commas_f(amount));
        push(format!("{amount:.2}"));
    }
    out
}

fn with_commas(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn with_commas_f(v: f64) -> String {
    let whole = v.trunc() as i64;
    let cents = ((v.abs().fract() * 100.0).round()) as i64;
    format!("{}.{:02}", with_commas(whole), cents)
}

/// **Synthesise a candidate rule from one confirmed correction.**
///
/// `None` when the confirmed amount's own text cannot be located in the raw
/// message — see the module docs for why that is a refusal rather than a
/// looser rule.
pub fn synthesize_from_correction(
    source: &str,
    raw_text: &str,
    operator: &str,
    params: &BTreeMap<String, serde_json::Value>,
    id: &str,
) -> Option<ParseRule> {
    let amount = params.get("amount")?.as_f64()?;

    let mut pattern = regex::escape(raw_text);
    let mut extract: BTreeMap<String, FieldSpec> = BTreeMap::new();

    if let Some(m) = leading_ref_re().captures(raw_text) {
        let escaped_ref = regex::escape(m.get(1)?.as_str());
        if pattern.contains(&escaped_ref) {
            pattern = pattern.replacen(&escaped_ref, r"(?P<ref>[A-Z0-9]{8,12})", 1);
            extract.insert(
                "ref".into(),
                FieldSpec { kind: FieldKind::Text, group: Some("ref".into()), value: None },
            );
        }
    }

    let mut found = false;
    for candidate in amount_texts(amount) {
        let escaped = regex::escape(&candidate);
        if pattern.contains(&escaped) {
            pattern = pattern.replacen(&escaped, r"(?P<amount>[\d,]+\.?\d*)", 1);
            extract.insert(
                "amount".into(),
                FieldSpec { kind: FieldKind::Amount, group: Some("amount".into()), value: None },
            );
            found = true;
            break;
        }
    }
    if !found {
        // ★ Nothing to anchor a generalisable rule on. A rule that matched only
        //   this exact text would occupy precedence and teach nothing.
        return None;
    }

    // ★★★ The asymmetry, inherited rather than re-decided here.
    let is_income = operator == "budget.record_income";

    let mut rule_params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    if is_income {
        for (k, v) in params {
            // The confirmed amount becomes a reference; everything else the
            // person confirmed is carried verbatim.
            let is_the_amount = k == "amount" && v.as_f64() == Some(amount);
            rule_params.insert(
                k.clone(),
                if is_the_amount { serde_json::json!("$amount") } else { v.clone() },
            );
        }
    }

    Some(ParseRule {
        id: id.to_string(),
        source: source.to_string(),
        version: 1,
        pattern,
        extract,
        status: if is_income { RuleStatus::Mapped } else { RuleStatus::ParsedUnmapped },
        operator: if is_income { Some(operator.to_string()) } else { None },
        params: rule_params,
        flags: vec![],
        reason_template: if is_income {
            None
        } else {
            Some(
                "Recognised from a message you classified before — still needs a pocket decision."
                    .into(),
            )
        },
        trust: ParseRuleTrust::UserCorrected,
        provenance: "human_correction".into(),
        examples: vec![raw_text.to_string()],
        routes: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Correction + field shapes
// ---------------------------------------------------------------------------

/// **Synthesise a rule from a correction, using the field reader for the shape.**
///
/// ★★★ **Two sources of truth, and each is used for what it actually knows.**
/// The correction knows what the message *meant* — a person confirmed the
/// operator and the amount, and nothing infers that better than they did. The
/// field reader ([`crate::field_shape`]) knows what the message is *shaped
/// like* — where the balance, the reference, the date and the counterparty sit.
/// [`synthesize_from_correction`] had only the first, so it escaped the entire
/// message as a literal and parameterised the amount. That is conservative in
/// the right spirit and wrong in two ways this fixes:
///
/// ★★★ **It leaked.** Everything the old synthesiser did not parameterise
/// stayed in the pattern verbatim — so a rule learned from *"received Ksh5,000
/// from JOHN KAMAU 254712345678"* carried the name and the number. A person
/// correcting one message should not thereby write somebody's phone number into
/// a rule that outlives the message.
///
/// ★★★ **And it could not fire twice.** The date and the closing balance were
/// literal anchors, so the rule matched only a message sent on that day leaving
/// that exact balance — which is to say, never again. A learner whose output
/// cannot match a second message has not learned anything; it has memorised.
///
/// ★★ **Where the two disagree, the HUMAN wins.** If the reader thinks the
/// amount is a different figure from the one the person confirmed, the person's
/// is authoritative and the reader's is discarded for that field. A confirmed
/// classification is evidence; a heuristic is a guess.
///
/// ★★ **The asymmetry is inherited, not re-decided.** Income may map; a spend
/// comes back `ParsedUnmapped` with no operator, exactly as before.
///
/// Returns `None` for the same reason the original does: if the confirmed
/// amount cannot be located in the raw text there is nothing to anchor on, and
/// a rule anchored on nothing occupies precedence and matches by accident.
pub fn synthesize_with_shapes(
    source: &str,
    raw_text: &str,
    operator: &str,
    params: &BTreeMap<String, serde_json::Value>,
    id: &str,
) -> Option<ParseRule> {
    use crate::field_shape::{read, Role};

    let amount = params.get("amount")?.as_f64()?;

    // Which span in the raw text is the amount the person confirmed?
    let confirmed_at = amount_texts(amount)
        .into_iter()
        .find_map(|t| raw_text.find(&t).map(|i| (i, i + t.len())))?;

    let reading = read(raw_text);

    // ★★ Every field the reader found, minus any that overlaps the confirmed
    //    amount — the person's span wins that ground outright.
    let mut spans: Vec<(usize, usize, &'static str, FieldKind)> = vec![(
        confirmed_at.0,
        confirmed_at.1,
        "amount",
        FieldKind::Amount,
    )];
    for f in &reading.fields {
        if f.role == Role::Amount {
            continue; // the human already supplied this one
        }
        let overlaps = f.at.0 < confirmed_at.1 && confirmed_at.0 < f.at.1;
        if overlaps {
            continue;
        }
        spans.push((
            f.at.0,
            f.at.1,
            f.role.group(),
            match f.role {
                Role::Balance | Role::Fee => FieldKind::Amount,
                _ => FieldKind::Text,
            },
        ));
    }
    spans.sort_by_key(|s| s.0);

    // ★ One group per name, first occurrence.
    let mut seen: Vec<&str> = Vec::new();
    let mut pattern = String::new();
    let mut extract: BTreeMap<String, FieldSpec> = BTreeMap::new();
    let mut cursor = 0usize;
    for (start, end, group, kind) in spans {
        if start < cursor || seen.contains(&group) {
            continue;
        }
        pattern.push_str(&shape_literal(&raw_text[cursor..start]));
        pattern.push_str(&crate::field_shape::group_pattern(group));
        extract.insert(
            group.to_string(),
            FieldSpec { kind, group: Some(group.to_string()), value: None },
        );
        seen.push(group);
        cursor = end;
    }
    pattern.push_str(&shape_literal(&raw_text[cursor..]));

    let is_income = operator == "budget.record_income";

    let mut rule_params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    if is_income {
        for (k, v) in params {
            let is_the_amount = k == "amount" && v.as_f64() == Some(amount);
            rule_params.insert(
                k.clone(),
                if is_the_amount { serde_json::json!("$amount") } else { v.clone() },
            );
        }
    }

    Some(ParseRule {
        id: id.to_string(),
        source: source.to_string(),
        version: 1,
        pattern,
        extract,
        status: if is_income { RuleStatus::Mapped } else { RuleStatus::ParsedUnmapped },
        operator: if is_income { Some(operator.to_string()) } else { None },
        params: rule_params,
        flags: vec!["IGNORECASE".into()],
        reason_template: if is_income {
            None
        } else {
            Some(
                "Recognised from a message you classified before — still needs a pocket decision."
                    .into(),
            )
        },
        trust: ParseRuleTrust::UserCorrected,
        provenance: "human_correction + field shapes".into(),
        examples: vec![raw_text.to_string()],
        routes: Vec::new(),
    })
}

/// Literal context with elastic whitespace. ★ Same treatment the inducer uses,
/// and for the same reason: providers reflow, and a rule bound to one sample's
/// spacing breaks on the next message of the same shape.
fn shape_literal(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    if raw.starts_with(char::is_whitespace) {
        out.push_str(r"\s+");
    }
    out.push_str(&raw.split_whitespace().map(regex::escape).collect::<Vec<_>>().join(r"\s+"));
    if raw.len() > 1 && raw.ends_with(char::is_whitespace) {
        out.push_str(r"\s+");
    }
    out
}

/// Why a candidate was refused.
///
/// ★ Named the long way: `Refusal` is taken. Collision 44, same house rule —
/// the newcomer takes the longer name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearningRefusal {
    NotWellTyped(Vec<String>),
    /// It does not recognise the message that produced it.
    NoFidelity,
    /// It fires on a message an existing rule already handles.
    Regression { existing_rule: String, example: String },
    /// Nothing was pointed at, so there is nothing to bind a rule to.
    NothingToLearn,
    /// A figure was named that does not appear in the sample.
    ///
    /// ★★ A refusal rather than a silent drop. A rule missing a figure he
    /// thought he taught would read the wrong number out of every later
    /// message of that shape, and nothing on screen would say so.
    NotInTheText { figure: String },
    /// Two figures claimed the same ground.
    Overlapping { figure: String },
}

impl std::fmt::Display for LearningRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotWellTyped(errors) => write!(f, "Γ ⊢ r failed: {}", errors.join("; ")),
            Self::NoFidelity => {
                write!(f, "the candidate does not match its own inducing example")
            }
            Self::Regression { existing_rule, example } => write!(
                f,
                "the candidate would also match a message '{existing_rule}' already handles: {example}"
            ),
            Self::NothingToLearn => write!(f, "no figures were given, so there is nothing to learn"),
            Self::NotInTheText { figure } => {
                write!(f, "'{figure}' does not appear in this message, so there is nowhere to read it from")
            }
            Self::Overlapping { figure } => {
                write!(f, "'{figure}' overlaps a figure already claimed, so one of them would be lost")
            }
        }
    }
}

/// One figure a person pointed at, and what they said it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainedFigure {
    /// The figure exactly as it appears in the sample, e.g. `"5.52"`.
    pub text: String,
    pub role: RouteRole,
    /// Where he said it belongs.
    pub pocket: String,
}

/// **Teach a shape by example, one figure at a time.**
///
/// ★★★ **The difference from `synthesize_with_shapes`.** That one learns from a
/// correction a person had already made — it takes the single confirmed amount
/// and works out what the rest of the message looks like around it. This one
/// takes what a person is *saying about the message in front of them*: these
/// numbers, here, mean these things, and belong in these pockets.
///
/// That distinction is the whole of Bonnie's Fuliza case. A borrow carries the
/// sum borrowed AND the access fee charged for it. One confirmed amount cannot
/// express that, so the fee was re-typed on every message of a shape he had
/// already explained. N figures can.
///
/// ★★★ **Positional, which is what makes one answer cover the cluster.** Each
/// figure's span in the sample becomes a named capture group, and the text
/// between spans becomes literal context with elastic whitespace. So the rule
/// does not remember `5.52`; it remembers *the number that sits where 5.52
/// sat*, and reads whatever is there in the next message of that shape.
///
/// ★★ **Everything not pointed at is still generalised, not frozen.** The field
/// reader's own findings — dates, references, the counterparty, account numbers
/// — are folded in around the person's figures exactly as the correction
/// learner folds them in. Without that, a rule learned from one sample would
/// carry that day's date and somebody's phone number and never match again.
/// This is not a new discipline; it is the same one, and a test holds it.
///
/// ★ **It refuses rather than guessing.** A figure the person names that does
/// not appear in the sample is not silently dropped, and two figures claiming
/// the same ground is not silently resolved. Both come back as refusals,
/// because a rule built on either would read the wrong number out of every
/// future message and nothing on screen would say so.
pub fn synthesize_from_training(
    source: &str,
    raw_text: &str,
    figures: &[TrainedFigure],
    id: &str,
) -> Result<ParseRule, LearningRefusal> {
    use crate::field_shape::{read, Role};

    if figures.is_empty() {
        return Err(LearningRefusal::NothingToLearn);
    }

    // ── Where each figure sits ────────────────────────────────────────────
    //
    // ★★ First occurrence, and claimed in the order given. A figure that
    //    appears twice is ambiguous, and taking the first is the reading a
    //    person doing the pointing would expect.
    let mut claimed: Vec<(usize, usize, &TrainedFigure)> = Vec::new();
    for f in figures {
        let t = f.text.trim();
        if t.is_empty() {
            return Err(LearningRefusal::NothingToLearn);
        }
        let Some(at) = raw_text.find(t) else {
            return Err(LearningRefusal::NotInTheText { figure: t.to_string() });
        };
        let span = (at, at + t.len());
        // ★ Two figures cannot own the same ground. Left unchecked, the second
        //   group would be dropped by the cursor walk below and the rule would
        //   quietly know one less thing than he taught it.
        if claimed.iter().any(|(s, e, _)| *s < span.1 && span.0 < *e) {
            return Err(LearningRefusal::Overlapping { figure: t.to_string() });
        }
        claimed.push((span.0, span.1, f));
    }

    // ── Names for the groups ──────────────────────────────────────────────
    //
    // ★★ The FIRST in/out figure is plain `amount` and the first fee is plain
    //    `fee`, because those are the names the rest of the system already
    //    binds — a taught rule and an induced one must be indistinguishable to
    //    the operator layer. Further figures of the same kind are numbered.
    let mut n_amount = 0usize;
    let mut n_fee = 0usize;
    let mut n_balance = 0usize;
    let mut n_date = 0usize;
    let mut named: Vec<(usize, usize, String, &TrainedFigure)> = Vec::new();
    for (s, e, f) in &claimed {
        let group = match f.role {
            RouteRole::Fee => {
                n_fee += 1;
                if n_fee == 1 { "fee".to_string() } else { format!("fee_{n_fee}") }
            }
            // ★★★ `balance_after` is not an arbitrary name. It is one of the
            //     names the balance reader already looks for, so a figure he
            //     tags as the balance on a shape nobody wrote a rule for
            //     becomes that account's authoritative figure with no further
            //     wiring at all -- the same path M-Pesa's own shapes take.
            RouteRole::Balance => {
                n_balance += 1;
                if n_balance == 1 {
                    "balance_after".to_string()
                } else {
                    format!("balance_after_{n_balance}")
                }
            }
            // ★★★ `datetime` when the span he tagged carries a clock, `date`
            //     when it is only a day. Both are names `event_time` already
            //     reads, so a tagged date becomes an ORDERING fact with no
            //     further wiring -- and ordering is the whole reason a balance
            //     means anything, since the account's figure is whichever of
            //     its messages is latest.
            RouteRole::Date => {
                n_date += 1;
                let base = if f.text.contains(" at ") { "datetime" } else { "date" };
                if n_date == 1 { base.to_string() } else { format!("{base}_{n_date}") }
            }
            _ => {
                n_amount += 1;
                if n_amount == 1 { "amount".to_string() } else { format!("amount_{n_amount}") }
            }
        };
        named.push((*s, *e, group, f));
    }

    // ── Everything he did NOT point at, generalised anyway ────────────────
    let reading = read(raw_text);
    let mut spans: Vec<(usize, usize, String, FieldKind)> = named
        .iter()
        .map(|(s, e, g, f)| {
            // ★ A date is read as TEXT. Typing it as an amount would send it
            //   through numeric parsing and lose it entirely.
            let kind = if f.role == RouteRole::Date { FieldKind::Text } else { FieldKind::Amount };
            (*s, *e, g.clone(), kind)
        })
        .collect();
    // ★★★ **Currency figures are claimed first, and that ORDER is the fix.**
    //
    //     A real bug lived here. The field reader can return a wide text span
    //     that swallows a figure whole — "…outstanding amount is Ksh 1,005.52
    //     due on…" read as one counterparty — and when that span was later
    //     dropped for having a group name already used, the figure inside it
    //     fell back into the literal context. The rule was then pinned to one
    //     day's outstanding balance: it matched the message it was taught on
    //     and no sibling, which is precisely the promise "teach one, all N
    //     follow" is made of. A test caught it; nothing else would have.
    //
    //     Claiming money first makes that unrepresentable rather than merely
    //     fixed: a number is a number, and the reader gets what is left.
    //
    //     Deliberately conservative — only figures written with an explicit
    //     `Ksh`/`KES`. Generalising every run of digits would swallow account
    //     fragments and dates and turn a precise rule into one that matches
    //     nearly anything, which is the more dangerous mistake by far.
    for m in money_figure_re().captures_iter(raw_text) {
        let g = m.get(1).expect("the figure group");
        if spans.iter().any(|(s, e, _, _)| g.start() < *e && *s < g.end()) {
            continue;
        }
        n_amount += 1;
        spans.push((g.start(), g.end(), format!("amount_{n_amount}"), FieldKind::Amount));
    }

    // ★★ Everything else the reader found — dates, references, the party,
    //    account numbers — folded in around what is already claimed. Without
    //    this a rule would carry that day's date and somebody's phone number.
    for fld in &reading.fields {
        // ★ Anything already claimed wins its ground outright: the person's
        //   figures first, then the money above.
        if spans.iter().any(|(s, e, _, _)| fld.at.0 < *e && *s < fld.at.1) {
            continue;
        }
        // ★ A figure the money pass did not take (written without a currency)
        //   still gets a group of its own rather than being frozen. It is read
        //   but never routed — he remains the authority on what means what.
        if matches!(fld.role, Role::Amount | Role::Fee) {
            n_amount += 1;
            spans.push((fld.at.0, fld.at.1, format!("amount_{n_amount}"), FieldKind::Amount));
            continue;
        }
        spans.push((
            fld.at.0,
            fld.at.1,
            fld.role.group().to_string(),
            match fld.role {
                Role::Balance => FieldKind::Amount,
                _ => FieldKind::Text,
            },
        ));
    }

    spans.sort_by_key(|s| s.0);

    // ── The pattern ───────────────────────────────────────────────────────
    let mut seen: Vec<String> = Vec::new();
    let mut pattern = String::new();
    let mut extract: BTreeMap<String, FieldSpec> = BTreeMap::new();
    let mut cursor = 0usize;
    for (start, end, group, kind) in spans {
        if start < cursor || seen.contains(&group) {
            continue;
        }
        pattern.push_str(&shape_literal(&raw_text[cursor..start]));
        pattern.push_str(&crate::field_shape::group_pattern(&group));
        extract.insert(
            group.clone(),
            FieldSpec { kind, group: Some(group.clone()), value: None },
        );
        seen.push(group);
        cursor = end;
    }
    pattern.push_str(&shape_literal(&raw_text[cursor..]));

    let routes: Vec<FigureRoute> = named
        .iter()
        .filter(|(_, _, g, _)| seen.contains(g))
        .map(|(_, _, g, f)| FigureRoute {
            group: g.clone(),
            role: f.role,
            pocket: f.pocket.trim().to_string(),
        })
        .collect();
    if routes.is_empty() {
        return Err(LearningRefusal::NothingToLearn);
    }

    // ★★★ **Money arriving may file itself; money leaving may not.** The
    //     asymmetry the whole ingest path is built on, and teaching does not
    //     get to weaken it. A taught rule that says "this is a fee of 5.52 for
    //     the Fuliza fees pocket" still comes to him to confirm — what it saves
    //     is the re-typing, not the deciding.
    //
    //     Income only auto-files when it is the ONLY thing taught. A message
    //     carrying an arrival and a fee together is two decisions, and one of
    //     them is a spend.
    // ★★★ A balance is not a decision. It states what the account holds, so a
    //     shape carrying an arrival AND its closing figure is still one thing
    //     to decide -- unlike an arrival carrying a fee, which is two
    //     movements into two places.
    let movements: Vec<&FigureRoute> = routes
        .iter()
        .filter(|r| !matches!(r.role, RouteRole::Balance | RouteRole::Date))
        .collect();
    let only_income = movements.len() == 1 && movements[0].role == RouteRole::In;

    Ok(ParseRule {
        id: id.to_string(),
        source: source.to_string(),
        version: 1,
        pattern,
        extract,
        status: if only_income { RuleStatus::Mapped } else { RuleStatus::ParsedUnmapped },
        operator: only_income.then(|| "budget.record_income".to_string()),
        params: if only_income {
            let mut p: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            p.insert("amount".into(), serde_json::json!("$amount"));
            p.insert("source".into(), serde_json::json!(movements[0].pocket.clone()));
            p
        } else {
            BTreeMap::new()
        },
        flags: vec!["IGNORECASE".into()],
        reason_template: (!only_income).then(|| {
            "Recognised from a shape you taught — the figures are read, and you confirm."
                .to_string()
        }),
        trust: ParseRuleTrust::UserCorrected,
        provenance: "human_training + field shapes".into(),
        examples: vec![raw_text.to_string()],
        routes,
    })
}

/// **`V(r)`** — both conditions, both required. See the module docs.
pub fn verify_candidate<U: OperatorUniverse>(
    candidate: &ParseRule,
    inducing_example: &str,
    existing: &[ParseRule],
    universe: &U,
) -> Result<(), LearningRefusal> {
    let errors = typecheck_rule(candidate, universe);
    if !errors.is_empty() {
        // ★ Short-circuit: an ill-typed rule cannot safely be RUN, so the
        //   fidelity check below must not be attempted on it.
        return Err(LearningRefusal::NotWellTyped(errors));
    }

    if apply_rule(candidate, inducing_example).is_none() {
        return Err(LearningRefusal::NoFidelity);
    }

    for rule in existing {
        for example in &rule.examples {
            if example == inducing_example {
                continue;
            }
            if apply_rule(candidate, example).is_some() {
                return Err(LearningRefusal::Regression {
                    existing_rule: rule.id.clone(),
                    example: example.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_rule::NoOperators;
    use crate::transducer::seed_rules;
    use serde_json::json;

    struct Universe;
    impl OperatorUniverse for Universe {
        fn has(&self, operator: &str) -> bool {
            matches!(operator, "budget.record_income" | "budget.spend")
        }
        fn params_of(&self, _operator: &str) -> Option<Vec<String>> {
            Some(vec![
                "amount".into(),
                "source".into(),
                "frequency".into(),
                "pocket_name".into(),
                "description".into(),
            ])
        }
    }

    fn params(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    const UNSEEN: &str =
        "ZZ99XYZ123 Confirmed. Ksh2,500.00 has been moved to your wallet by CHAMA GROUP on 1/8/26.";

    #[test]
    fn a_correction_becomes_a_rule_that_matches_the_message_that_taught_it() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0)), ("source", json!("Chama"))]),
            "learned_1",
        )
        .expect("synthesised");
        assert!(verify_candidate(&r, UNSEEN, &[], &Universe).is_ok());
        let t = apply_rule(&r, UNSEEN).expect("matches");
        assert_eq!(t.operator(), Some("budget.record_income"));
        assert_eq!(t.operator_params()["amount"], json!(2500.0));
        assert_eq!(t.operator_params()["source"], json!("Chama"));
    }

    #[test]
    fn the_learned_rule_generalises_over_the_amount_and_nothing_else() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_2",
        )
        .expect("synthesised");

        // ★ The same shape with a DIFFERENT amount matches, and extracts it.
        let next_month = UNSEEN.replace("2,500.00", "4,100.00");
        let t = apply_rule(&r, &next_month).expect("matches the next one");
        assert_eq!(t.operator_params()["amount"], json!(4100.0));

        // ★★ A different shape does NOT. Being narrow is the design.
        assert!(apply_rule(&r, "Ksh2,500.00 paid to SOMEWHERE ELSE entirely").is_none());
    }

    #[test]
    fn a_leading_reference_is_parameterised_so_the_next_one_still_matches() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_3",
        )
        .expect("synthesised");
        let different_ref = UNSEEN.replace("ZZ99XYZ123", "AB12CD34EF");
        assert!(apply_rule(&r, &different_ref).is_some(), "a new ref must not break it");
    }

    #[test]
    fn a_learned_spend_never_auto_applies() {
        // ★★★ The asymmetry. Money out is a person's decision every time.
        let r = synthesize_from_correction(
            "mpesa",
            "QQ11WW22EE Confirmed. Ksh680.00 paid to NAIVAS on 1/8/26.",
            "budget.spend",
            &params(&[("amount", json!(680.0)), ("pocket_name", json!("food"))]),
            "learned_4",
        )
        .expect("synthesised");
        assert_eq!(r.status, RuleStatus::ParsedUnmapped);
        assert!(r.operator.is_none(), "a learned spend carries no operator at all");
        assert!(r.params.is_empty(), "and no pocket");
        assert!(r.reason_template.is_some(), "it says why it still needs a person");
    }

    #[test]
    fn a_learned_rule_is_marked_as_the_person_s_own() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_5",
        )
        .unwrap();
        assert_eq!(r.trust, ParseRuleTrust::UserCorrected);
        assert_eq!(r.provenance, "human_correction");
        assert_eq!(r.examples, vec![UNSEEN.to_string()]);
    }

    #[test]
    fn an_amount_that_is_not_in_the_text_synthesises_nothing() {
        // ★ A rule anchored on nothing would occupy precedence and match by
        //   accident. None is the honest answer.
        assert!(synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(999.0))]),
            "learned_6",
        )
        .is_none());
        // And so is a correction with no amount at all.
        assert!(synthesize_from_correction("kcb", UNSEEN, "budget.spend", &params(&[]), "x")
            .is_none());
    }

    #[test]
    fn a_candidate_that_would_capture_an_existing_rules_message_is_refused() {
        // ★★ The regression condition. A hand-made over-broad candidate that
        //    matches a shipped rule's own example must not be accepted.
        let shipped = seed_rules("mpesa");
        let example = shipped
            .iter()
            .find(|r| !r.examples.is_empty())
            .and_then(|r| r.examples.first().cloned())
            .expect("a shipped example");

        let mut greedy = shipped[0].clone();
        greedy.id = "greedy".into();
        greedy.pattern = "Confirmed".into();
        greedy.status = RuleStatus::ParsedUnmapped;
        greedy.operator = None;
        greedy.params.clear();
        greedy.extract.clear();
        greedy.examples = vec![];

        let err = verify_candidate(&greedy, "something else entirely Confirmed", shipped, &Universe)
            .unwrap_err();
        match err {
            LearningRefusal::Regression { example: e, .. } => assert_eq!(e, example),
            other => panic!("expected a regression refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_candidate_that_does_not_match_its_own_example_is_refused() {
        let mut r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_7",
        )
        .unwrap();
        r.pattern = "something else".into();
        assert_eq!(
            verify_candidate(&r, UNSEEN, &[], &Universe).unwrap_err(),
            LearningRefusal::NoFidelity
        );
    }

    #[test]
    fn an_ill_typed_candidate_is_refused_before_it_is_ever_run() {
        // ★ Short-circuit: a broken pattern must not reach the fidelity check.
        let mut r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_8",
        )
        .unwrap();
        r.pattern = "(unclosed".into();
        assert!(matches!(
            verify_candidate(&r, UNSEEN, &[], &Universe).unwrap_err(),
            LearningRefusal::NotWellTyped(_)
        ));
    }

    #[test]
    fn a_learned_income_rule_is_refused_where_the_operator_does_not_exist() {
        // ★ The typecheck is the caller's universe, not a global assumption.
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_9",
        )
        .unwrap();
        assert!(matches!(
            verify_candidate(&r, UNSEEN, &[], &NoOperators).unwrap_err(),
            LearningRefusal::NotWellTyped(_)
        ));
    }

    #[test]
    fn amount_text_candidates_cover_how_a_real_message_writes_a_figure() {
        assert_eq!(amount_texts(2500.0), vec!["2,500.00", "2,500", "2500.00", "2500"]);
        assert_eq!(amount_texts(680.0), vec!["680.00", "680"]);
        assert_eq!(amount_texts(1234.56), vec!["1,234.56", "1234.56"]);
    }

    #[test]
    fn synthesis_is_reproducible() {
        let p = params(&[("amount", json!(2500.0))]);
        let a = synthesize_from_correction("kcb", UNSEEN, "budget.record_income", &p, "id");
        let b = synthesize_from_correction("kcb", UNSEEN, "budget.record_income", &p, "id");
        assert_eq!(a, b, "no clock, no randomness — the id is the caller's");
    }
}

#[cfg(test)]
mod shape_learning_tests {
    use super::*;
    use crate::parse_rule::RuleStatus;

    const RECEIVED: &str = "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU \
                            254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00";
    const PAID: &str = "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 \
                        at 4:30 PM. New M-PESA balance is Ksh12,050.00";

    fn params(amount: f64) -> BTreeMap<String, serde_json::Value> {
        let mut p = BTreeMap::new();
        p.insert("amount".to_string(), serde_json::json!(amount));
        p
    }

    fn compiled(rule: &ParseRule) -> regex::Regex {
        regex::RegexBuilder::new(&rule.pattern)
            .case_insensitive(true)
            .build()
            .expect("a learned pattern must compile")
    }

    #[test]
    fn a_learned_rule_carries_none_of_the_message_it_was_taught_with() {
        // ★★★ The defect this replaces. The original escaped the whole message,
        //     so a rule learned from one correction carried the sender's name
        //     and phone number for as long as the rule lived.
        let rule = synthesize_with_shapes(
            "mpesa", RECEIVED, "budget.record_income", &params(5000.0), "learned-1",
        )
        .expect("synthesised");
        for pii in ["JOHN KAMAU", "254712345678", "5,000.00", "15,000.00", "QGH7XJ2K9L"] {
            assert!(!rule.pattern.contains(pii), "the learned pattern leaked {pii:?}");
        }
    }

    #[test]
    fn the_old_synthesiser_really_did_leak_and_this_is_the_difference() {
        // ★★ Asserted rather than claimed, so the improvement is a fact on the
        //    record and a regression would show up here.
        let old = synthesize_from_correction(
            "mpesa", RECEIVED, "budget.record_income", &params(5000.0), "old",
        )
        .expect("synthesised");
        assert!(old.pattern.contains("JOHN KAMAU"), "the original kept the name");

        let new = synthesize_with_shapes(
            "mpesa", RECEIVED, "budget.record_income", &params(5000.0), "new",
        )
        .expect("synthesised");
        assert!(!new.pattern.contains("JOHN KAMAU"));
    }

    #[test]
    fn a_learned_rule_matches_the_next_message_of_the_same_shape() {
        // ★★★ The second defect. Anchoring on the date and the closing balance
        //     meant the rule matched only a message sent that day leaving that
        //     exact balance — never again. A learner whose output cannot fire
        //     twice has memorised rather than learned.
        let rule = synthesize_with_shapes(
            "mpesa", RECEIVED, "budget.record_income", &params(5000.0), "learned",
        )
        .expect("synthesised");
        let re = compiled(&rule);
        assert!(re.is_match(RECEIVED), "it matches what taught it");

        // Same shape, everything variable different.
        let next = "AB99ZZ1234 Confirmed. You have received Ksh250.00 from AMINA HASSAN \
                    254799887766 on 03/9/26 at 8:05 AM. New M-PESA balance is Ksh9,410.55";
        assert!(re.is_match(next), "a learned rule must fire on the NEXT message like it");

        // And the original could not.
        let old = synthesize_from_correction(
            "mpesa", RECEIVED, "budget.record_income", &params(5000.0), "old",
        )
        .expect("synthesised");
        assert!(!compiled(&old).is_match(next), "the original could only ever match its own text");
    }

    #[test]
    fn it_does_not_match_a_different_shape() {
        // ★★ The literal sentence is still an anchor. Generalising the FIELDS
        //    is not the same as generalising the message, and a rule that
        //    matched anything would be worse than none.
        let rule = synthesize_with_shapes(
            "mpesa", RECEIVED, "budget.record_income", &params(5000.0), "learned",
        )
        .expect("synthesised");
        assert!(!compiled(&rule).is_match(PAID), "an income rule must not match a spend");
    }

    #[test]
    fn a_corrected_spend_never_learns_to_spend_on_its_own() {
        // ★★★ The asymmetry, inherited and re-asserted. Correcting a spend once
        //     must not authorise the app to file the next one unasked.
        let rule = synthesize_with_shapes(
            "mpesa", PAID, "budget.spend", &params(450.0), "learned-spend",
        )
        .expect("synthesised");
        assert_eq!(rule.status, RuleStatus::ParsedUnmapped);
        assert!(rule.operator.is_none());
        assert!(rule.params.is_empty(), "no params to apply, because it will not apply");
        assert!(rule.reason_template.as_deref().unwrap().contains("pocket decision"));
    }

    #[test]
    fn the_human_wins_where_the_reader_disagrees() {
        // ★★★ A confirmed classification is evidence; a heuristic is a guess.
        //     Here the person confirms the BALANCE figure as the amount — an
        //     odd correction, and theirs to make. The rule must anchor on what
        //     they said, not on what the reader would have picked.
        let rule = synthesize_with_shapes(
            "mpesa", RECEIVED, "budget.record_income", &params(15000.0), "learned",
        )
        .expect("synthesised");
        let re = compiled(&rule);
        let caps = re.captures(RECEIVED).expect("matches");
        assert_eq!(caps.name("amount").map(|m| m.as_str()), Some("15,000.00"));
    }

    #[test]
    fn an_unlocatable_amount_still_refuses() {
        // ★ Unchanged: a rule anchored on nothing occupies precedence and
        //   matches by accident.
        assert!(synthesize_with_shapes(
            "mpesa", RECEIVED, "budget.record_income", &params(77.0), "x"
        )
        .is_none());
    }

    #[test]
    fn it_is_marked_as_a_human_correction() {
        let rule = synthesize_with_shapes(
            "mpesa", RECEIVED, "budget.record_income", &params(5000.0), "learned",
        )
        .expect("synthesised");
        assert_eq!(rule.trust, ParseRuleTrust::UserCorrected);
        assert!(rule.provenance.contains("human_correction"));
        assert_eq!(rule.examples, vec![RECEIVED.to_string()]);
    }

    #[test]
    fn learning_is_deterministic() {
        let a = synthesize_with_shapes("m", RECEIVED, "budget.record_income", &params(5000.0), "i");
        let b = synthesize_with_shapes("m", RECEIVED, "budget.record_income", &params(5000.0), "i");
        assert_eq!(a, b);
    }
}

#[cfg(test)]
mod training_tests {
    use super::*;
    use crate::parse_rule::{run_rules, RuleStatus};

    /// Bonnie's own case, and the reason this exists: two numbers, two
    /// meanings, two pockets, one message.
    const FULIZA: &str = "TFF9J0ABCD Confirmed. Fuliza M-PESA amount is Ksh 1,000.00. \
                          Access Fee charged Ksh 5.52. Total Fuliza M-PESA outstanding \
                          amount is Ksh 1,005.52 due on 15/8/26.";

    /// A second message of the SAME shape, different figures. This is what the
    /// taught rule has to read, and the whole claim of "teach one, all N follow".
    const FULIZA_2: &str = "QKL2M8XYZW Confirmed. Fuliza M-PESA amount is Ksh 250.00. \
                            Access Fee charged Ksh 1.38. Total Fuliza M-PESA outstanding \
                            amount is Ksh 251.38 due on 3/9/26.";

    fn fuliza_figures() -> Vec<TrainedFigure> {
        vec![
            TrainedFigure {
                text: "1,000.00".into(),
                role: RouteRole::In,
                pocket: "Fuliza".into(),
            },
            TrainedFigure {
                text: "5.52".into(),
                role: RouteRole::Fee,
                pocket: "Fuliza fees".into(),
            },
        ]
    }

    #[test]
    fn two_figures_in_one_message_get_two_routes() {
        // ★★★ The whole point. One amount could never express "the sum
        //     borrowed goes here and the fee goes there", so the fee was
        //     re-typed on every message of a shape already explained once.
        let r = synthesize_from_training("mpesa", FULIZA, &fuliza_figures(), "t1")
            .expect("it learns");
        assert_eq!(r.routes.len(), 2);
        assert_eq!(r.routes[0].group, "amount");
        assert_eq!(r.routes[0].role, RouteRole::In);
        assert_eq!(r.routes[0].pocket, "Fuliza");
        assert_eq!(r.routes[1].group, "fee");
        assert_eq!(r.routes[1].role, RouteRole::Fee);
        assert_eq!(r.routes[1].pocket, "Fuliza fees");
    }

    #[test]
    fn what_it_learned_reads_the_next_message_of_that_shape() {
        // ★★★ Positional, not remembered. The rule must read 250.00 and 1.38
        //     out of a message it has never seen, from where 1,000.00 and 5.52
        //     sat in the one it was taught on.
        let r = synthesize_from_training("mpesa", FULIZA, &fuliza_figures(), "t1")
            .expect("it learns");
        let out = run_rules(&[r], FULIZA_2).expect("it recognises the sibling");
        assert_eq!(out.parsed_fields().get("amount").and_then(|v| v.as_f64()), Some(250.0));
        assert_eq!(out.parsed_fields().get("fee").and_then(|v| v.as_f64()), Some(1.38));
    }


    #[test]
    fn it_reads_the_message_it_was_taught_on() {
        let r = synthesize_from_training("mpesa", FULIZA, &fuliza_figures(), "t1")
            .expect("it learns");
        let out = run_rules(&[r], FULIZA).expect("fidelity to its own example");
        assert_eq!(out.parsed_fields().get("amount").and_then(|v| v.as_f64()), Some(1000.0));
        assert_eq!(out.parsed_fields().get("fee").and_then(|v| v.as_f64()), Some(5.52));
    }

    #[test]
    fn a_taught_shape_with_a_fee_still_asks_him() {
        // ★★★ The money-safety asymmetry, and teaching does not weaken it.
        //     What a taught rule saves is the re-typing, never the deciding.
        let r = synthesize_from_training("mpesa", FULIZA, &fuliza_figures(), "t1")
            .expect("it learns");
        assert_eq!(r.status, RuleStatus::ParsedUnmapped);
        assert!(r.operator.is_none(), "nothing files itself here");
        assert!(r.reason_template.is_some(), "and it says why it is waiting");
    }

    #[test]
    fn money_arriving_on_its_own_may_file_itself() {
        // ★★ The one case that auto-files, unchanged from every other learner
        //    in this codebase: an arrival, alone, with nothing else to decide.
        let figures = vec![TrainedFigure {
            text: "1,000.00".into(),
            role: RouteRole::In,
            pocket: "salary".into(),
        }];
        let r = synthesize_from_training("mpesa", FULIZA, &figures, "t2").expect("it learns");
        assert_eq!(r.status, RuleStatus::Mapped);
        assert_eq!(r.operator.as_deref(), Some("budget.record_income"));
        assert_eq!(r.params.get("amount").and_then(|v| v.as_str()), Some("$amount"));
    }

    #[test]
    fn an_arrival_that_also_carries_a_fee_does_not_file_itself() {
        // ★★★ Two decisions, and one of them is a spend. Auto-filing the
        //     arrival would file half a message and leave the other half
        //     unaccounted, which is worse than asking.
        let r = synthesize_from_training("mpesa", FULIZA, &fuliza_figures(), "t1")
            .expect("it learns");
        assert_eq!(r.status, RuleStatus::ParsedUnmapped);
    }

    #[test]
    fn the_particulars_do_not_get_frozen_into_the_rule() {
        // ★★★ The discipline the correction learner already holds. A rule that
        //     kept this day's date and this reference could never match a
        //     second message, and would carry the sample around forever.
        let r = synthesize_from_training("mpesa", FULIZA, &fuliza_figures(), "t1")
            .expect("it learns");
        assert!(!r.pattern.contains("TFF9J0ABCD"), "the reference is generalised: {}", r.pattern);
        assert!(!r.pattern.contains("15/8/26"), "the date is generalised: {}", r.pattern);
        assert!(!r.pattern.contains("1,000.00"), "nor the figure he pointed at: {}", r.pattern);
        assert!(!r.pattern.contains("5.52"), "nor the fee: {}", r.pattern);
    }

    #[test]
    fn a_figure_that_is_not_in_the_message_is_refused_by_name() {
        // ★★ Refused, not dropped. A rule quietly missing a figure he thought
        //    he taught would read the wrong number out of every later message,
        //    and nothing on screen would say so.
        let figures = vec![TrainedFigure {
            text: "999.99".into(),
            role: RouteRole::Out,
            pocket: "food".into(),
        }];
        let err = synthesize_from_training("mpesa", FULIZA, &figures, "t3").unwrap_err();
        match err {
            LearningRefusal::NotInTheText { ref figure } => assert_eq!(figure, "999.99"),
            other => panic!("expected NotInTheText, got {other:?}"),
        }
        assert!(err.to_string().contains("999.99"), "and it names it: {err}");
    }

    #[test]
    fn two_figures_claiming_the_same_ground_are_refused() {
        let figures = vec![
            TrainedFigure { text: "1,000.00".into(), role: RouteRole::In, pocket: "a".into() },
            TrainedFigure { text: "1,000.00".into(), role: RouteRole::Fee, pocket: "b".into() },
        ];
        let err = synthesize_from_training("mpesa", FULIZA, &figures, "t4").unwrap_err();
        assert!(matches!(err, LearningRefusal::Overlapping { .. }), "got {err:?}");
    }

    #[test]
    fn pointing_at_nothing_teaches_nothing() {
        let err = synthesize_from_training("mpesa", FULIZA, &[], "t5").unwrap_err();
        assert!(matches!(err, LearningRefusal::NothingToLearn), "got {err:?}");
    }

    #[test]
    fn a_third_figure_of_the_same_kind_gets_its_own_group() {
        // ★★ Numbered rather than colliding. Two fees in one message is not
        //    hypothetical -- providers itemise -- and the second must not
        //    silently overwrite the first.
        let figures = vec![
            TrainedFigure { text: "1,000.00".into(), role: RouteRole::In, pocket: "a".into() },
            TrainedFigure { text: "5.52".into(), role: RouteRole::Fee, pocket: "b".into() },
            TrainedFigure { text: "1,005.52".into(), role: RouteRole::Out, pocket: "c".into() },
        ];
        let r = synthesize_from_training("mpesa", FULIZA, &figures, "t6").expect("it learns");
        let groups: Vec<&str> = r.routes.iter().map(|x| x.group.as_str()).collect();
        assert_eq!(groups, vec!["amount", "fee", "amount_2"]);
        // ★ And the numbered group must read a NUMBER, not words.
        let out = run_rules(&[r], FULIZA_2).expect("sibling");
        assert_eq!(out.parsed_fields().get("amount_2").and_then(|v| v.as_f64()), Some(251.38));
    }

    #[test]
    fn a_rule_written_before_routes_existed_still_loads() {
        // ★★★ The seed library is JSON on disk and predates all of this. If
        //     `routes` were required, every shipped rule would fail to load and
        //     the household would lose every format it already knew.
        let json = r#"{
            "id": "old", "source": "mpesa", "pattern": "Ksh(?P<amount>[0-9.]+)",
            "status": "parsed_unmapped", "extract": {}
        }"#;
        let r: ParseRule = serde_json::from_str(json).expect("it loads");
        assert!(r.routes.is_empty(), "and means the honest 'nobody has said yet'");
    }
}

#[cfg(test)]
mod balance_role_tests {
    use super::*;
    use crate::parse_rule::run_rules;

    /// A shape from a bank nobody has written a rule for. It states a movement
    /// and the account's closing figure, the way every provider does.
    const NEW_BANK: &str = "EQ8842001 Confirmed. You have received KES 2,500.00 from JANE DOE on 01/09/26 at 10:15 AM. Your account balance is KES 47,310.55";
    const NEW_BANK_2: &str = "EQ8842002 Confirmed. You have received KES 900.00 from JOHN ROE on 02/09/26 at 11:20 AM. Your account balance is KES 48,210.55";

    fn taught() -> Vec<TrainedFigure> {
        vec![
            TrainedFigure {
                text: "2,500.00".into(),
                role: RouteRole::In,
                pocket: "salary".into(),
            },
            TrainedFigure {
                text: "47,310.55".into(),
                role: RouteRole::Balance,
                pocket: String::new(),
            },
        ]
    }

    #[test]
    fn a_taught_balance_lands_in_the_field_the_reader_looks_for() {
        // ★★★ The whole mechanism. `balance_after` is one of the names the
        //     balance reader already checks, so tagging a figure teaches this
        //     node to read a new bank's authoritative number with no further
        //     wiring -- the same path M-Pesa's own shapes take.
        let r = synthesize_from_training("equity", NEW_BANK, &taught(), "t1").expect("learns");
        assert!(r.extract.contains_key("balance_after"), "extracted: {:?}", r.extract.keys());
        let out = run_rules(&[r], NEW_BANK).expect("reads its own example");
        assert_eq!(
            out.parsed_fields().get("balance_after").and_then(|v| v.as_f64()),
            Some(47_310.55)
        );
    }

    #[test]
    fn it_reads_the_balance_out_of_the_next_message_of_that_shape() {
        // ★★★ Positional, so the figure is read from where it SAT, not
        //     remembered. This is what makes the newest message authoritative.
        let r = synthesize_from_training("equity", NEW_BANK, &taught(), "t1").expect("learns");
        let out = run_rules(&[r], NEW_BANK_2).expect("reads a sibling");
        assert_eq!(
            out.parsed_fields().get("balance_after").and_then(|v| v.as_f64()),
            Some(48_210.55),
            "the newer message's own closing figure"
        );
    }

    #[test]
    fn a_balance_is_never_routed_to_a_pocket() {
        // ★★★ A closing figure is not a movement. Routing one into a pocket
        //     would book money that never moved.
        let r = synthesize_from_training("equity", NEW_BANK, &taught(), "t1").expect("learns");
        let balance = r.routes.iter().find(|x| x.role == RouteRole::Balance).expect("routed");
        assert!(balance.pocket.is_empty(), "it belongs to the account, not a pocket");
        assert!(
            r.routes.iter().filter(|x| x.role != RouteRole::Balance).all(|x| !x.pocket.is_empty()),
            "while every real movement still names one"
        );
    }

    #[test]
    fn an_arrival_that_also_states_a_balance_still_files_itself() {
        // ★★★ A balance is not a second decision. A shape carrying an arrival
        //     AND its closing figure is one thing to decide -- unlike an
        //     arrival carrying a FEE, which is two movements into two places.
        let r = synthesize_from_training("equity", NEW_BANK, &taught(), "t1").expect("learns");
        assert_eq!(r.status, RuleStatus::Mapped);
        assert_eq!(r.operator.as_deref(), Some("budget.record_income"));
        assert_eq!(
            r.params.get("source").and_then(|v| v.as_str()),
            Some("salary"),
            "and it reads the MOVEMENT's pocket, not whichever figure came first"
        );
    }

    #[test]
    fn an_arrival_with_a_fee_still_asks_even_when_a_balance_is_taught() {
        let figures = vec![
            TrainedFigure { text: "2,500.00".into(), role: RouteRole::In, pocket: "salary".into() },
            TrainedFigure { text: "47,310.55".into(), role: RouteRole::Balance, pocket: String::new() },
        ];
        let mut with_fee = figures;
        with_fee.insert(
            1,
            TrainedFigure { text: "10.15".into(), role: RouteRole::Fee, pocket: "charges".into() },
        );
        // The fee text has to be present for the figure to be found.
        let raw = "EQ8842003 Confirmed. You have received KES 2,500.00 from JANE DOE, charge KES 10.15, on 01/09/26. Your account balance is KES 47,310.55";
        let r = synthesize_from_training("equity", raw, &with_fee, "t2").expect("learns");
        assert_eq!(r.status, RuleStatus::ParsedUnmapped, "two movements is two decisions");
    }

    #[test]
    fn a_balance_on_its_own_is_a_real_thing_to_teach() {
        // ★★ A pure balance statement -- no movement at all -- is exactly the
        //    shape a person most wants to tag, and it must not auto-file.
        let raw = "EQ8842004 Your account balance is KES 47,310.55 as at 01/09/26";
        let figures = vec![TrainedFigure {
            text: "47,310.55".into(),
            role: RouteRole::Balance,
            pocket: String::new(),
        }];
        let r = synthesize_from_training("equity", raw, &figures, "t3").expect("learns");
        assert_eq!(r.status, RuleStatus::ParsedUnmapped, "nothing moved, so nothing files");
        assert!(r.operator.is_none());
        let out = run_rules(&[r], raw).expect("reads it");
        assert_eq!(
            out.parsed_fields().get("balance_after").and_then(|v| v.as_f64()),
            Some(47_310.55)
        );
    }
}

#[cfg(test)]
mod date_role_tests {
    use super::*;
    use crate::parse_rule::run_rules;

    /// His own message, and the one the whole feature is about: the account's
    /// figure is whichever message is LATEST, so this date decides which
    /// balance is current.
    const REAL: &str = "UHOB946Y0H Confirmed. Ksh600.00 paid to PETER MAINA NDIGIRIGI. on 24/8/26 at 8:38 PM.New M-PESA balance is Ksh235.19. Transaction cost, Ksh0.00.";
    /// A sibling, a day later and with a different balance.
    const LATER: &str = "UHOB947Z1K Confirmed. Ksh80.00 paid to SOME OTHER PAYEE. on 25/8/26 at 9:10 AM.New M-PESA balance is Ksh155.19. Transaction cost, Ksh0.00.";

    fn taught() -> Vec<TrainedFigure> {
        vec![
            TrainedFigure { text: "600.00".into(), role: RouteRole::Out, pocket: "Leisure".into() },
            TrainedFigure {
                text: "24/8/26 at 8:38 PM".into(),
                role: RouteRole::Date,
                pocket: String::new(),
            },
            TrainedFigure {
                text: "235.19".into(),
                role: RouteRole::Balance,
                pocket: String::new(),
            },
        ]
    }


    #[test]
    fn a_tagged_date_becomes_a_real_reading() {
        let r = synthesize_from_training("mpesa", REAL, &taught(), "d1").expect("learns");
        let out = run_rules(&[r], REAL).expect("reads its own example");
        let f = out.parsed_fields();
        assert_eq!(
            f.get("datetime").and_then(|v| v.as_str()),
            Some("24/8/26 at 8:38 PM"),
            "the day and the clock, in one tag: {f:?}"
        );
    }

    #[test]
    fn the_tagged_date_orders_the_cluster() {
        // ★★★ The whole point, and why he asked for it. An account shows the
        //     balance of its LATEST message, so a date that does not read is a
        //     balance that may be the wrong one.
        let r = synthesize_from_training("mpesa", REAL, &taught(), "d1").expect("learns");
        let first = run_rules(std::slice::from_ref(&r), REAL).expect("first");
        let second = run_rules(&[r], LATER).expect("second");
        let at = |t: &crate::transducer::Transduction| {
            crate::event_time::event_time(&t.parsed_fields())
                .expect("it says when")
                .to_epoch_ms(180)
        };
        assert!(at(&second) > at(&first), "25/8 9:10 AM is after 24/8 8:38 PM");
    }

    #[test]
    fn the_date_is_read_out_of_a_sibling_not_remembered() {
        let r = synthesize_from_training("mpesa", REAL, &taught(), "d1").expect("learns");
        let out = run_rules(&[r], LATER).expect("reads a sibling");
        assert_eq!(
            out.parsed_fields().get("datetime").and_then(|v| v.as_str()),
            Some("25/8/26 at 9:10 AM"),
            "the sibling's OWN date, from where the taught one sat"
        );
    }

    #[test]
    fn a_date_is_never_routed_to_a_pocket_and_never_files() {
        let r = synthesize_from_training("mpesa", REAL, &taught(), "d1").expect("learns");
        let date = r.routes.iter().find(|x| x.role == RouteRole::Date).expect("routed");
        assert!(date.pocket.is_empty(), "nothing moved, so there is no pocket");
        // A spend is present, so this asks either way -- but the date must not
        // be what makes it a movement.
        assert_eq!(r.status, RuleStatus::ParsedUnmapped);
    }

    #[test]
    fn a_date_alongside_a_lone_arrival_still_files_itself() {
        // ★★ Neither a date nor a balance is a decision. An arrival carrying
        //    both is still one thing to decide, unlike an arrival with a fee.
        let raw = "EQ1 Confirmed. You have received KES 2,500.00 from JANE DOE on 01/09/26 at 10:15 AM. Your account balance is KES 47,310.55";
        let figures = vec![
            TrainedFigure { text: "2,500.00".into(), role: RouteRole::In, pocket: "salary".into() },
            TrainedFigure {
                text: "01/09/26 at 10:15 AM".into(),
                role: RouteRole::Date,
                pocket: String::new(),
            },
            TrainedFigure {
                text: "47,310.55".into(),
                role: RouteRole::Balance,
                pocket: String::new(),
            },
        ];
        let r = synthesize_from_training("equity", raw, &figures, "d2").expect("learns");
        assert_eq!(r.status, RuleStatus::Mapped);
        assert_eq!(r.operator.as_deref(), Some("budget.record_income"));
    }

    #[test]
    fn a_bare_day_with_no_clock_is_still_a_date() {
        let raw = "KCB Your loan is due on 29/11/23. Outstanding KES 597.42";
        let figures = vec![TrainedFigure {
            text: "29/11/23".into(),
            role: RouteRole::Date,
            pocket: String::new(),
        }];
        let r = synthesize_from_training("kcb", raw, &figures, "d3").expect("learns");
        let out = run_rules(&[r], raw).expect("reads it");
        assert_eq!(
            out.parsed_fields().get("date").and_then(|v| v.as_str()),
            Some("29/11/23"),
            "named `date`, not `datetime`, because there is no clock in it"
        );
    }
}
