//! **Reading a financial message nobody wrote a rule for** (Ingest · §II, §VII).
//!
//! ```text
//!   raw text ──► tokens ──► field roles ──► a candidate ParseRule
//!                            (evidence)        (human confirms)
//! ```
//!
//! ★★★ **This is NOT part of `τ`, and that is the whole architecture.** §II
//! requires the transducer to be **pure**: *"same input, same answer, every
//! time, no network call and no model in the loop"*, because idempotent intake
//! (§V) and replay both assume reprocessing reclassifies identically — *"a
//! boundary whose classification drifts turns a replayable log into a diary."*
//! So nothing here runs during classification. This module **induces candidate
//! rules**; `τ` keeps doing exactly what it did, over declared rules, ordered
//! and first-match.
//!
//! ★★★ **And it is deterministic anyway, which is why it can live in core.**
//! There is no model, no sampling and no clock: the same message produces the
//! same labels and the same candidate rule, always. That is what lets the
//! intelligence sit in the portable core (ADR-0001 Decision 1 puts the
//! transducer here) instead of being exiled to a host — a host-side extractor
//! would have been the easy answer and would have split the parser across two
//! runtimes for no gain.
//!
//! ## Why this generalises where a per-provider regex cannot
//!
//! ★★★ **Financial messages do not share a message grammar; they share a
//! FIELD grammar.** Safaricom, KCB, a co-op bank and a SACCO write completely
//! different sentences, and all of them contain the same five or six things: an
//! amount that is a currency token beside a number, a balance that is an amount
//! wearing a keyword, a reference that is a run of characters with far more
//! entropy than a word, a counterparty that is a capitalised run, a date, and a
//! verb that says which way the money went.
//!
//! So the unit of recognition here is the **field**, not the sentence. A shape
//! this module has never seen still contains an amount that looks like an
//! amount. That is the difference between a parser that needs a rule per
//! provider and one that needs a rule per *field kind*.
//!
//! ## What it refuses to do
//!
//! ★★★ **It never induces a rule that spends.** §4K's money-safety asymmetry
//! is not softened by better parsing: income may auto-apply because a credit
//! has one honest reading, and a debit does not — *which pocket* is a decision
//! with no derivable answer. So [`induce`] emits [`RuleStatus::Mapped`] only
//! for an inbound shape, and everything outbound comes back
//! `ParsedUnmapped`, exactly as a hand-written rule would.
//!
//! ★★★ **It learns SHAPES and never values.** An induced pattern keeps the
//! literal words around a field and replaces the field itself with a named
//! group, so a rule induced from a message mentioning a person carries the
//! sentence and not the person. A test asserts that no induced pattern
//! contains the amount, the reference or the counterparty it was induced from.
//! Learning from a backlog must not become remembering a backlog.
//!
//! ★★ **It proposes; the schema and a person dispose.** Output is a
//! [`ParseRule`] candidate, which still has to pass `typecheck_rule` and then a
//! human — the same propose/verify/confirm path `parse_rule_learn` already
//! uses. Nothing here writes anything.

use std::collections::BTreeMap;

use regex::Regex;

use crate::parse_rule::{FieldKind, FieldSpec, ParseRule, RuleStatus};

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

/// What a span of a financial message is *for*.
///
/// ★★ Deliberately about the **money**, not about the provider. Every one of
/// these appears in every provider's messages under different words, which is
/// exactly why the role rather than the wording is the unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// The sum that moved.
    Amount,
    /// What the account holds afterwards.
    Balance,
    /// What the movement cost on top of itself.
    Fee,
    /// The provider's own transaction code.
    Reference,
    /// Who the money went to or came from.
    Counterparty,
    /// The other party's number, or a masked account.
    ///
    /// ★★★ Added because a test caught a leak: with no role for it, a phone
    /// number sat in the literal context and an induced pattern carried
    /// somebody's real number. It is also a genuinely universal field —
    /// providers put a number or a masked PAN beside the party constantly.
    Account,
    /// When the provider says it happened.
    Date,
    /// The time of day, where it is given separately.
    Time,
}

impl Role {
    /// The group name an induced pattern uses. ★ Fixed, so an induced rule and
    /// a hand-written one bind the same names and the operator layer cannot
    /// tell them apart.
    pub fn group(&self) -> &'static str {
        match self {
            Role::Amount => "amount",
            Role::Balance => "balance_after",
            Role::Fee => "fee",
            Role::Reference => "ref",
            Role::Counterparty => "counterparty",
            Role::Account => "account",
            Role::Date => "date",
            Role::Time => "time",
        }
    }
}

/// Which way the money went.
///
/// ★★★ `Unclear` is a real answer and not a failure. A message whose verb this
/// module does not recognise has an unknown direction, and guessing would be
/// the one error that matters — the difference between crediting and debiting a
/// household.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    Unclear,
}

/// One recognised span, with the reason it was recognised.
///
/// ★★ The evidence travels with the finding. A labelling a person cannot
/// interrogate is one they have to take on faith, and this whole layer exists
/// to *propose* to somebody who will check it.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub role: Role,
    /// Byte range in the original text.
    pub at: (usize, usize),
    /// The text itself. ★ Held for the caller's inspection and **never**
    /// written into an induced pattern — see the module note on shapes.
    pub text: String,
    /// Why this span got this role, in words.
    pub because: &'static str,
}

/// Everything read off one message.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub fields: Vec<Field>,
    pub direction: Direction,
    pub direction_because: &'static str,
}

impl Reading {
    pub fn get(&self, role: Role) -> Option<&Field> {
        self.fields.iter().find(|f| f.role == role)
    }

    /// ★★ How much of the money-shape was recovered. Not a confidence score —
    /// a count of what is present, which is a fact rather than an opinion.
    pub fn recovered(&self) -> usize {
        self.fields.len()
    }
}

// ---------------------------------------------------------------------------
// The recognisers
// ---------------------------------------------------------------------------

/// A currency token beside a number.
///
/// ★★ Currency-prefixed on purpose. A bare number matcher over a real SMS
/// finds the date, the phone fragment, the reference's digits and the time —
/// the amount is the number somebody wrote a currency in front of.
fn amount_re() -> &'static Regex {
    static_re(
        r"(?i)\b(?:ksh|kes|tsh|ugx|ngn|zar|usd|eur|gbp)\s*\.?\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)",
    )
}

/// Words that turn a nearby amount into a balance.
const BALANCE_WORDS: &[&str] = &[
    "balance", "bal", "avail", "available", "new balance", "current balance",
];

/// Words that turn a nearby amount into a cost.
const FEE_WORDS: &[&str] = &[
    "transaction cost", "cost", "fee", "charge", "charges", "excise", "levy",
];

/// Verbs that say money arrived.
const IN_WORDS: &[&str] = &[
    "received", "credited", "deposited", "you have received", "has been credited",
    "refund", "reversed to", "paid to you",
];

/// Verbs that say money left.
const OUT_WORDS: &[&str] = &[
    "sent to", "paid to", "withdraw", "withdrawn", "debited", "purchase",
    "bought", "transferred to", "spent",
];

fn static_re(pattern: &str) -> &'static Regex {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, &'static Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().expect("regex cache");
    if let Some(r) = guard.get(pattern) {
        return r;
    }
    let leaked: &'static Regex =
        Box::leak(Box::new(Regex::new(pattern).expect("static pattern compiles")));
    guard.insert(pattern.to_string(), leaked);
    leaked
}

/// Is this token shaped like a provider's transaction code?
///
/// ★★★ **Entropy, not a format.** Every provider mints codes differently —
/// `QGH7XJ2K9L`, `UH1B91GYNW`, `FT26081234` — and enumerating them is the
/// treadmill this module exists to get off. What they share is that they mix
/// letters and digits, run long, and are not words. That test recognises the
/// next provider's format too.
pub fn looks_like_a_reference(token: &str) -> bool {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    if t.len() < 8 || t.len() > 16 {
        return false;
    }
    if !t.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let digits = t.chars().filter(char::is_ascii_digit).count();
    let uppers = t.chars().filter(|c| c.is_ascii_uppercase()).count();
    // ★★ Both present, and the letters are shouted. A code is not a word, and
    //    a word is not a code — "Confirmed" is long and all letters; "254712"
    //    is long and all digits. Neither is a reference.
    digits >= 2 && uppers >= 2 && digits + uppers == t.len()
}

/// A date in any of the shapes providers actually write.
fn date_re() -> &'static Regex {
    static_re(r"\b(\d{1,2}[/-]\d{1,2}[/-]\d{2,4})\b")
}

/// A clock time, 12- or 24-hour.
fn time_re() -> &'static Regex {
    static_re(r"(?i)\b(\d{1,2}:\d{2}(?::\d{2})?\s*(?:[AP]\.?M\.?)?)")
}

/// A phone number or a masked account.
///
/// ★ Six digits or more, or anything carrying a mask. Short numbers are left
/// alone: a till number is part of the shape, and redacting it would make a
/// rule that could no longer tell two tills apart.
fn account_re() -> &'static Regex {
    static_re(r"\b([0-9]{3,}\*{2,}[0-9]{2,}|[0-9]{9,15})\b")
}

/// A capitalised run: a merchant, a person, a till name.
fn name_re() -> &'static Regex {
    static_re(r"\b([A-Z][A-Za-z&'.\-]*(?:\s+[A-Z][A-Za-z&'.\-]*){0,4})\b")
}

// ---------------------------------------------------------------------------
// Reading a message
// ---------------------------------------------------------------------------

/// How far before an amount a keyword still claims it.
const NEAR: usize = 28;

/// **Read the money-shape out of any financial message.**
///
/// Deterministic and total: an unrecognisable message returns a `Reading` with
/// no fields and `Direction::Unclear`, never an error — §II's totality applied
/// one layer down.
pub fn read(text: &str) -> Reading {
    let mut fields: Vec<Field> = Vec::new();

    // -- amounts, and what the words around them make them ------------------
    let mut amounts: Vec<(usize, usize, String)> = Vec::new();
    for m in amount_re().captures_iter(text) {
        // ★★★ The span is the NUMBER, not the currency-plus-number. The
        //     currency token is *shape* — it belongs in the pattern's literal
        //     context so an induced rule still requires a currency there.
        //     Swallowing it into the span dropped it from the pattern entirely,
        //     which a test caught by the rule failing to match its own sample.
        let num = m.get(1).expect("group 1");
        amounts.push((num.start(), num.end(), num.as_str().to_string()));
    }

    let lower = text.to_lowercase();
    for (start, end, value) in &amounts {
        // ★ Widened by the currency token the span no longer covers.
        let window_from = start.saturating_sub(NEAR + 6);
        let before = &lower[window_from..*start];
        // ★★★ The NEAREST keyword claims the figure, not the first list that
        //     happens to match. "Available balance NGN 88,300.50. Charge NGN
        //     26.88" puts both words in the window of the second figure, and
        //     checking balance-first labelled a fee as a balance. Proximity is
        //     the real rule: the word immediately before a figure is the one
        //     describing it.
        let nearest = |words: &[&str]| -> Option<usize> {
            words.iter().filter_map(|w| before.rfind(w)).max()
        };
        let role = match (nearest(BALANCE_WORDS), nearest(FEE_WORDS)) {
            (Some(b), Some(f)) if b >= f => {
                Some((Role::Balance, "a balance word sits nearest before it"))
            }
            (Some(_), Some(_)) => Some((Role::Fee, "a cost word sits nearest before it")),
            (Some(_), None) => Some((Role::Balance, "a balance word sits before it")),
            (None, Some(_)) => Some((Role::Fee, "a cost word sits before it")),
            (None, None) => None,
        };
        if let Some((role, because)) = role {
            fields.push(Field { role, at: (*start, *end), text: value.clone(), because });
        }
    }

    // ★★ The amount is the first currency figure no keyword claimed. Providers
    //    lead with the sum that moved and mention the balance afterwards, and
    //    where they do not, the keyword has already taken it.
    let claimed: Vec<(usize, usize)> = fields.iter().map(|f| f.at).collect();
    if let Some((start, end, value)) =
        amounts.iter().find(|(s, e, _)| !claimed.contains(&(*s, *e)))
    {
        fields.push(Field {
            role: Role::Amount,
            at: (*start, *end),
            text: value.clone(),
            because: "the first currency figure no keyword claimed",
        });
    }

    // -- the reference ------------------------------------------------------
    for token in text.split_whitespace() {
        if looks_like_a_reference(token) {
            let t = token.trim_matches(|c: char| !c.is_alphanumeric());
            if let Some(at) = text.find(t) {
                fields.push(Field {
                    role: Role::Reference,
                    at: (at, at + t.len()),
                    text: t.to_string(),
                    because: "letters and digits mixed, too long and too shouted to be a word",
                });
                break;
            }
        }
    }

    // -- when ---------------------------------------------------------------
    if let Some(m) = date_re().captures(text).and_then(|c| c.get(1)) {
        fields.push(Field {
            role: Role::Date,
            at: (m.start(), m.end()),
            text: m.as_str().to_string(),
            because: "a day/month/year triple",
        });
    }
    if let Some(m) = time_re().captures(text).and_then(|c| c.get(1)) {
        fields.push(Field {
            role: Role::Time,
            at: (m.start(), m.end()),
            text: m.as_str().trim().to_string(),
            because: "a clock reading",
        });
    }

    // -- direction, and who ------------------------------------------------
    let (direction, direction_because, verb_at) = direction_of(&lower);
    if let Some(v) = verb_at {
        if let Some(f) = counterparty_after(text, v) {
            fields.push(f);
        }
    }

    // -- the other party's number, or a masked account ---------------------
    //
    // ★★ Recognised so it is REDACTED, first and foremost. A long digit run or
    //    a masked PAN is somebody's identifier, and an induced pattern must
    //    carry its shape and never its value.
    if let Some(m) = account_re().captures(text).and_then(|c| c.get(1)) {
        let overlaps = fields.iter().any(|f| f.at.0 < m.end() && m.start() < f.at.1);
        if !overlaps {
            fields.push(Field {
                role: Role::Account,
                at: (m.start(), m.end()),
                text: m.as_str().to_string(),
                because: "a long digit run or a masked account number",
            });
        }
    }

    fields.sort_by_key(|f| (f.at.0, f.role));
    Reading { fields, direction, direction_because }
}

fn direction_of(lower: &str) -> (Direction, &'static str, Option<usize>) {
    // ★★ OUT is checked first. "paid to" and "received" can both appear in one
    //    message ("...paid to X ... you have received a statement"), and the
    //    movement this message is about is the one its verb names first.
    let out = OUT_WORDS.iter().filter_map(|w| lower.find(w).map(|i| (i, *w))).min();
    let inn = IN_WORDS.iter().filter_map(|w| lower.find(w).map(|i| (i, *w))).min();
    match (out, inn) {
        (Some((oi, w)), Some((ii, _))) if oi <= ii => {
            (Direction::Out, "an outbound verb comes first", Some(oi + w.len()))
        }
        (Some(_), Some((ii, w))) => {
            (Direction::In, "an inbound verb comes first", Some(ii + w.len()))
        }
        (Some((oi, w)), None) => (Direction::Out, "an outbound verb", Some(oi + w.len())),
        (None, Some((ii, w))) => (Direction::In, "an inbound verb", Some(ii + w.len())),
        (None, None) => (Direction::Unclear, "no verb this reader knows", None),
    }
}

/// Words that introduce the other party.
///
/// ★★★ **The counterparty is anchored on a PREPOSITION, not on the verb.**
/// The first version looked for a capitalised run immediately after the verb
/// and a test caught it: *"received Ksh5,000.00 from JOHN KAMAU"* puts the
/// currency token there, so the reader labelled `Ksh` and left the name in the
/// literal text — which then leaked into an induced pattern. The grammar every
/// provider actually uses is *verb … preposition … name*: received **from**,
/// paid **to**, spent **at**, transferred **to**. Anchoring on the preposition
/// is both correct and more universal than anchoring on any one verb.
const PARTY_WORDS: &[&str] = &[" from ", " to ", " at ", " for "];

/// The capitalised run introduced by a preposition after the verb.
fn counterparty_after(text: &str, from: usize) -> Option<Field> {
    if from >= text.len() {
        return None;
    }
    let tail = &text[from..];
    let lower = tail.to_lowercase();
    // The nearest preposition after the verb.
    let (prep_at, prep) = PARTY_WORDS
        .iter()
        .filter_map(|w| lower.find(w).map(|i| (i, *w)))
        .min_by_key(|(i, _)| *i)?;
    let after = prep_at + prep.len();
    if after >= tail.len() {
        return None;
    }

    let m = name_re().captures(&tail[after..])?.get(1)?;
    // ★ It has to follow the preposition directly. A capitalised word further
    //   down the sentence belongs to a different clause.
    if m.start() > 1 {
        return None;
    }
    let who = m.as_str().trim();
    // ★★ A currency token is not a name. Without this the reader labels the
    //    `KES` in "to KES 400" as the party it was paid to.
    if who.is_empty() || is_currency(who) {
        return None;
    }
    Some(Field {
        role: Role::Counterparty,
        at: (from + after + m.start(), from + after + m.start() + who.len()),
        text: who.to_string(),
        because: "a capitalised run introduced by a preposition",
    })
}

fn is_currency(token: &str) -> bool {
    matches!(
        token.split_whitespace().next().unwrap_or("").to_ascii_uppercase().as_str(),
        "KSH" | "KES" | "TSH" | "UGX" | "NGN" | "ZAR" | "USD" | "EUR" | "GBP"
    )
}

// ---------------------------------------------------------------------------
// Inducing a rule
// ---------------------------------------------------------------------------

/// What a candidate rule could not be given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotInducible {
    /// No amount was recovered. Without one there is no transaction to describe.
    NoAmount,
    /// The direction is unknown, so neither tier can be chosen honestly.
    DirectionUnclear,
}

impl NotInducible {
    pub fn describe(&self) -> &'static str {
        match self {
            Self::NoAmount => {
                "no amount could be read, and a rule that matches a transaction without \
                 finding its sum would file nothing"
            }
            Self::DirectionUnclear => {
                "the direction is unknown — guessing it is the one error that matters, \
                 because it is the difference between crediting and debiting"
            }
        }
    }
}

/// **Turn one read message into a candidate `ParseRule`.**
///
/// ★★★ The pattern is built from the message's own literal context with every
/// recognised field replaced by a named group. So it carries the *sentence
/// shape* and none of the *content* — the induced rule from a text mentioning a
/// person contains no person.
///
/// ★★★ **Inbound may map; outbound never does.** §4K's asymmetry, applied to
/// induction rather than argued about again.
///
/// The result is a **candidate**: `typecheck_rule` still has to accept it and a
/// person still has to confirm it. Nothing here writes.
pub fn induce(text: &str, source: &str, id: &str) -> Result<ParseRule, NotInducible> {
    let reading = read(text);
    if reading.get(Role::Amount).is_none() {
        return Err(NotInducible::NoAmount);
    }
    if reading.direction == Direction::Unclear {
        return Err(NotInducible::DirectionUnclear);
    }

    // -- pattern: literal context, named groups where the fields were --------
    let mut pattern = String::new();
    let mut cursor = 0usize;
    let mut used: Vec<Role> = Vec::new();
    for f in &reading.fields {
        if f.at.0 < cursor {
            continue; // overlapping finding; the earlier one wins
        }
        if used.contains(&f.role) {
            continue; // one group per role
        }
        pattern.push_str(&literal(&text[cursor..f.at.0]));
        pattern.push_str(&group_for(f.role));
        used.push(f.role);
        cursor = f.at.1;
    }
    pattern.push_str(&literal(&text[cursor..]));

    let mut extract: BTreeMap<String, FieldSpec> = BTreeMap::new();
    for role in &used {
        extract.insert(
            role.group().to_string(),
            FieldSpec {
                kind: match role {
                    Role::Amount | Role::Balance | Role::Fee => FieldKind::Amount,
                    _ => FieldKind::Text,
                },
                group: Some(role.group().to_string()),
                value: None,
            },
        );
    }
    extract.insert(
        "direction".to_string(),
        FieldSpec {
            kind: FieldKind::Literal,
            group: None,
            value: Some(serde_json::json!(match reading.direction {
                Direction::In => "received",
                _ => "sent",
            })),
        },
    );

    let inbound = reading.direction == Direction::In;
    let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    if inbound {
        params.insert("amount".into(), serde_json::json!("$amount"));
        params.insert("source".into(), serde_json::json!(source));
    }

    Ok(ParseRule {
        id: id.to_string(),
        source: source.to_string(),
        version: 1,
        pattern,
        extract,
        // ★★★ The asymmetry, in one expression.
        status: if inbound { RuleStatus::Mapped } else { RuleStatus::ParsedUnmapped },
        operator: inbound.then(|| "budget.record_income".to_string()),
        params,
        flags: vec!["IGNORECASE".into()],
        reason_template: (!inbound).then(|| {
            "money left the account; which pocket it came from is yours to say".to_string()
        }),
        // ★★★ `ProposedConfirmed` is the trust a rule EARNS by being confirmed,
        //     and this one has not been yet — but there is no fourth variant
        //     for *proposed and not yet confirmed*, because a rule in that
        //     state is not in the rule set at all. It is a candidate the caller
        //     is holding, and the confirming step is what puts it anywhere.
        trust: crate::parse_rule::ParseRuleTrust::ProposedConfirmed,
        provenance: format!("induced from a {source} message by field shape"),
        // ★★ The inducing message is its own regression corpus, exactly as
        //    §VII asks: a rule carries its own evidence. This is the one place
        //    the raw text is kept, and it is kept BY the caller who already had
        //    it — nothing new is disclosed by a rule remembering what made it.
        examples: vec![text.to_string()],
    })
}

/// Literal context, with every run of whitespace made elastic.
///
/// ★★ Built this way rather than escaped-then-substituted: how `regex::escape`
/// treats a space is a detail of that crate's version, and a rule whose
/// correctness depends on it would break quietly on an upgrade. Providers
/// reflow their messages, so spacing must never bind a rule.
fn literal(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    // ★★ Leading and trailing whitespace matter as much as the middle: they
    //    are the separator between a group and the word beside it. Dropping
    //    them glued `(?P<ref>…)` straight onto the next word and the rule
    //    stopped matching the very message it came from.
    if raw.starts_with(char::is_whitespace) {
        out.push_str(r"\s+");
    }
    out.push_str(
        &raw.split_whitespace().map(regex::escape).collect::<Vec<_>>().join(r"\s+"),
    );
    if raw.len() > 1 && raw.ends_with(char::is_whitespace) {
        out.push_str(r"\s+");
    }
    out
}

/// The regex group for a field name, so the correction learner emits the same
/// groups the inducer does. ★ One vocabulary, two producers — otherwise a
/// learned rule and an induced one would bind different names for the same
/// field and the operator layer would need to know which made it.
pub fn group_pattern(group: &str) -> String {
    let inner = match group {
        "amount" | "balance_after" | "fee" => r"[0-9][0-9,]*(?:\.[0-9]{1,2})?",
        "ref" => r"[A-Z0-9]{8,16}",
        "date" => r"\d{1,2}[/-]\d{1,2}[/-]\d{2,4}",
        "time" => r"\d{1,2}:\d{2}(?::\d{2})?\s*(?:[AP]\.?M\.?)?",
        "account" => r"[0-9*]{6,20}",
        _ => r"[A-Za-z0-9&'.\- ]{2,40}?",
    };
    format!("(?P<{group}>{inner})")
}

fn group_for(role: Role) -> String {
    let inner = match role {
        Role::Amount | Role::Balance | Role::Fee => r"[0-9][0-9,]*(?:\.[0-9]{1,2})?",
        Role::Reference => r"[A-Z0-9]{8,16}",
        Role::Date => r"\d{1,2}[/-]\d{1,2}[/-]\d{2,4}",
        Role::Time => r"\d{1,2}:\d{2}(?::\d{2})?\s*(?:[AP]\.?M\.?)?",
        Role::Counterparty => r"[A-Za-z0-9&'.\- ]{2,40}?",
        Role::Account => r"[0-9*]{6,20}",
    };
    format!("(?P<{}>{})", role.group(), inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real shapes, redacted the way the rest of this repo redacts them:
    // names substituted, figures and code formats kept.
    const MPESA_IN: &str = "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU \
                            254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00";
    const MPESA_OUT: &str = "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 \
                             at 4:30 PM. New M-PESA balance is Ksh12,050.00. Transaction cost, Ksh0.00.";
    const KCB_CARD: &str = "KES 429.00 spent on KCB card 4243***0319 at GOOGLE Spotify Music on \
                            01/08/2026 12:25pm. Avail balance KES 59,055.00";

    /// A provider this build has never seen. The point of the whole module.
    const UNSEEN: &str = "TXN AB12CD34EF: NGN 12,500.00 transferred to ADAEZE OKAFOR on 03-09-2026 \
                          at 09:14. Available balance NGN 88,300.50. Charge NGN 26.88";

    fn role_text(r: &Reading, role: Role) -> Option<&str> {
        r.get(role).map(|f| f.text.as_str())
    }

    #[test]
    fn it_reads_a_shape_it_has_never_been_taught() {
        // ★★★ The claim the module exists to make. No rule, no provider entry,
        //     no keyword list naming this bank — and every field comes back.
        let r = read(UNSEEN);
        assert_eq!(role_text(&r, Role::Amount), Some("12,500.00"));
        assert_eq!(role_text(&r, Role::Balance), Some("88,300.50"));
        assert_eq!(role_text(&r, Role::Fee), Some("26.88"));
        assert_eq!(role_text(&r, Role::Reference), Some("AB12CD34EF"));
        assert_eq!(role_text(&r, Role::Date), Some("03-09-2026"));
        assert_eq!(r.direction, Direction::Out);
    }

    #[test]
    fn the_amount_is_not_the_balance_and_the_keyword_is_why() {
        // ★★ Both are currency figures in the same sentence. What separates
        //    them is the word in front, which is what the reader keys on.
        let r = read(MPESA_IN);
        assert_eq!(role_text(&r, Role::Amount), Some("5,000.00"));
        assert_eq!(role_text(&r, Role::Balance), Some("15,000.00"));
        assert!(r.get(Role::Balance).unwrap().because.contains("balance word"));
    }

    #[test]
    fn a_cost_is_told_apart_from_both() {
        let r = read(MPESA_OUT);
        assert_eq!(role_text(&r, Role::Amount), Some("450.00"));
        assert_eq!(role_text(&r, Role::Balance), Some("12,050.00"));
        assert_eq!(role_text(&r, Role::Fee), Some("0.00"));
    }

    #[test]
    fn a_reference_is_recognised_by_entropy_not_by_format() {
        // ★★★ Three providers, three formats, no list of formats anywhere.
        assert!(looks_like_a_reference("QGH7XJ2K9L"));
        assert!(looks_like_a_reference("UH1B91GYNW"));
        assert!(looks_like_a_reference("AB12CD34EF"));
        // And the things that are long but are not codes:
        assert!(!looks_like_a_reference("Confirmed"), "a word");
        assert!(!looks_like_a_reference("254712345678"), "a phone number");
        assert!(!looks_like_a_reference("SUPERMARKET"), "all letters");
        assert!(!looks_like_a_reference("KSH"), "too short");
    }

    #[test]
    fn direction_reads_the_verb_and_says_which_one() {
        assert_eq!(read(MPESA_IN).direction, Direction::In);
        assert_eq!(read(MPESA_OUT).direction, Direction::Out);
        assert_eq!(read(KCB_CARD).direction, Direction::Out);
    }

    #[test]
    fn an_unknown_verb_is_unclear_and_not_a_guess() {
        // ★★★ The one error that matters is inventing a direction, because it
        //     is the difference between crediting and debiting a household.
        let r = read("Ksh100.00 something happened on 01/01/2026");
        assert_eq!(r.direction, Direction::Unclear);
        assert!(r.direction_because.contains("no verb"));
    }

    #[test]
    fn reading_is_total_and_never_errors() {
        // §II's totality, one layer down: an unreadable message is a Reading
        // with nothing in it, not an absence the caller has to handle.
        for junk in ["", "hey are we still on for lunch?", "....", "Ksh"] {
            let r = read(junk);
            assert_eq!(r.direction, Direction::Unclear);
            assert!(r.get(Role::Amount).is_none());
        }
    }

    #[test]
    fn reading_is_deterministic() {
        assert_eq!(read(UNSEEN), read(UNSEEN));
        assert_eq!(read(MPESA_OUT), read(MPESA_OUT));
    }

    // -- induction -----------------------------------------------------------

    #[test]
    fn an_induced_pattern_contains_none_of_the_message_it_came_from() {
        // ★★★ The property that makes learning from 2,500 real messages
        //     legitimate. The rule keeps the SENTENCE and drops the CONTENT.
        let rule = induce(MPESA_IN, "mpesa", "induced-1").expect("inducible");
        for secret in ["JOHN KAMAU", "5,000.00", "15,000.00", "QGH7XJ2K9L", "254712345678"] {
            assert!(
                !rule.pattern.contains(secret),
                "the induced pattern leaked {secret:?}"
            );
        }
        // And it kept the shape it needs to match again.
        assert!(rule.pattern.contains("(?P<amount>"));
        assert!(rule.pattern.contains("Confirmed"));
    }

    #[test]
    fn an_induced_rule_matches_the_message_that_induced_it() {
        // ★★ Otherwise induction is a story about a message rather than a rule
        //    for it. This is `verify_proposed_rule`'s first condition, checked
        //    here so a candidate cannot leave this module already broken.
        for sample in [MPESA_IN, MPESA_OUT, KCB_CARD, UNSEEN] {
            let rule = induce(sample, "any", "induced").expect("inducible");
            let re = regex::RegexBuilder::new(&rule.pattern)
                .case_insensitive(true)
                .build()
                .expect("the induced pattern compiles");
            assert!(re.is_match(sample), "did not match its own inducing sample");
        }
    }

    #[test]
    fn an_outbound_shape_never_induces_a_rule_that_spends() {
        // ★★★ §4K's money-safety asymmetry, and better parsing does not soften
        //     it. Which pocket a debit came from has no derivable answer, so a
        //     person is asked however well the message was read.
        for outbound in [MPESA_OUT, KCB_CARD, UNSEEN] {
            let rule = induce(outbound, "any", "induced").expect("inducible");
            assert_eq!(rule.status, RuleStatus::ParsedUnmapped);
            assert!(rule.operator.is_none(), "an induced spend rule must name no operator");
        }
    }

    #[test]
    fn an_inbound_shape_may_map_because_a_credit_has_one_reading() {
        let rule = induce(MPESA_IN, "mpesa", "induced").expect("inducible");
        assert_eq!(rule.status, RuleStatus::Mapped);
        assert_eq!(rule.operator.as_deref(), Some("budget.record_income"));
        assert_eq!(rule.params.get("amount"), Some(&serde_json::json!("$amount")));
    }

    #[test]
    fn an_unclear_direction_induces_nothing() {
        let out = induce("Ksh100.00 happened on 01/01/2026", "any", "x");
        assert_eq!(out, Err(NotInducible::DirectionUnclear));
        assert!(out.unwrap_err().describe().contains("guessing"));
    }

    #[test]
    fn no_amount_induces_nothing() {
        assert_eq!(
            induce("You have received a statement from us", "any", "x"),
            Err(NotInducible::NoAmount)
        );
    }

    #[test]
    fn the_rule_carries_its_own_evidence_and_its_provenance() {
        // §VII: `examples` is a regression corpus and `provenance` records
        // where a rule came from.
        let rule = induce(UNSEEN, "newbank", "induced-9").expect("inducible");
        assert_eq!(rule.examples, vec![UNSEEN.to_string()]);
        assert!(rule.provenance.contains("induced"));
        assert_eq!(rule.source, "newbank");
    }

    #[test]
    fn induction_is_deterministic() {
        assert_eq!(
            induce(UNSEEN, "b", "id").expect("a"),
            induce(UNSEEN, "b", "id").expect("b")
        );
    }

    #[test]
    fn reflowed_whitespace_still_matches() {
        // ★★ Providers reflow. A rule bound to the exact spacing of one sample
        //    is a rule that breaks on the next message of the same shape.
        let rule = induce(MPESA_IN, "mpesa", "induced").expect("inducible");
        let re = regex::RegexBuilder::new(&rule.pattern)
            .case_insensitive(true)
            .build()
            .expect("compiles");
        let reflowed = MPESA_IN.replace(" on ", "  on  ");
        assert!(re.is_match(&reflowed), "spacing must not bind the rule");
    }
}
