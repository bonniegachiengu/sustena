//! `ℐ : ε → (o, θ)` — the deterministic Enzyme-call proposer (UI-11's core
//! slice; the Curated-UI paper §VIII).
//!
//! Turn a described effect into a **candidate** [`EnzymeCall`] over the real
//! [`Definition`] and [`Registry`]. Pattern-match, declared-parameter lookup
//! and live-state matching only — **no LLM, no `eval`, no `compile`**, the same
//! discipline as `predicate`, `parse_rule` and every prior slice.
//!
//! ## ★★★ Why this is one slice and not two
//!
//! Two residuals asked for the same missing piece, and each said so in its own
//! row:
//!
//! - **UI-11** was deferred with a named core slice: *"queue it as a real core
//!   slice **when a Rust-side caller exists** — the natural one is an
//!   Enzyme-call proposer over `Definition` + `Registry`, which is also what
//!   EVT-15's `EnzymeCall` residual would consume."* The inference was already
//!   judged portable; what it lacked was a caller.
//! - **EVT-15** shipped [`EnzymeCall`] with **no producer at all**: every one in
//!   the crate was hand-built in a test.
//!
//! This module is that producer. ★ **And the discharge is scoped honestly**:
//! EVT-15's residual is worded *"not wired to `execute` (nothing yet **records**
//! an `EnzymeCall` when an operator runs)"*, which is the **other direction** —
//! a recorder over the host's log. A proposer gives `EnzymeCall` a real
//! producer and gives `replay_under` something it did not have to be handed by
//! hand; it does **not** make `execute` write a call log, because that log is
//! host state. Both halves are named in the tracker rather than one being
//! claimed for the other.
//!
//! ## ★★ `θ` is DECLARED here, where the reference introspects
//!
//! Python's `ℐ` reads `inspect.signature(meta.fn)`. A Rust operator is an
//! `OperatorFn` reading a `Map<String, Value>` by name — **there is no
//! signature to introspect** — so the honest analogue is
//! [`ParamDecl`](crate::operator::ParamDecl), declared on `OperatorMeta`. See
//! its own docs for what that trade costs in each direction.
//!
//! The property UI-11 actually cares about survives either way: **a proposal
//! can never carry a field the operator does not declare**, because
//! [`propose_call`] only ever writes names it read off the declaration.
//!
//! ## ★★ Matched against the sustain's own live state, generically
//!
//! The reference hardcodes pocket matching. Here a parameter declares
//! `names_within: Some("finances.pockets")`, and the proposer matches candidate
//! values against **the keys actually present at that path in this state**. So
//! the core never learns what a pocket is, and any sustain that names a
//! collection gets the same behaviour for free.
//!
//! ## ★★★ Never guesses, never skips the confirm
//!
//! Three of UI-11's rules are load-bearing and each is a distinct outcome here:
//!
//! - **A hint that eliminates every candidate is treated as no hint.** If
//!   narrowing by the description leaves nothing, the proposer falls back to
//!   the full candidate set and *asks* — it does not report *nothing matches*.
//! - **History may pre-fill but never silently.** A value taken from `known`
//!   is listed in [`Proposed::from_history`], so a caller can show *where this
//!   came from* rather than presenting it as inferred.
//! - **The output is a CANDIDATE, never an execution.** [`propose_call`] runs no
//!   operator and touches no state. Committing stays the human-gated path —
//!   the approval token, one act at a time, which follow-on wiring 2 made
//!   structural.
//!
//! ## The app-layer boundary, named
//!
//! What *feeds* `ℐ` stays app-layer and this does not change that: the
//! transducer (`parse_message` and the SMS shapes) by the ING rows' own
//! decision, and capture history because it is durable per-household memory.
//! They arrive here as an [`Effect`]'s `text` and `known` — two plain values —
//! so the port is the **inference**, not the plumbing around it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Number, Value};

use crate::editing::Definition;
use crate::operator::{ParamDecl, ParamKind, Registry};
use crate::semantic::EnzymeCall;
use crate::state::State;

/// `ε` — a described effect, plus whatever is already known about it.
///
/// ★ `known` is where history pre-fill and a human's prior answers arrive, and
/// they arrive by the **same** door: a fact is a fact however it was obtained,
/// and the difference that matters — *was this inferred or remembered* — is
/// reported on the way out rather than encoded on the way in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effect {
    text: String,
    known: BTreeMap<String, Value>,
}

impl Effect {
    pub fn described(text: &str) -> Effect {
        Effect { text: text.to_string(), known: BTreeMap::new() }
    }

    /// A fact already established — a prior answer, or a history pre-fill.
    pub fn knowing(mut self, field: &str, value: Value) -> Effect {
        self.known.insert(field.to_string(), value);
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn known(&self) -> &BTreeMap<String, Value> {
        &self.known
    }

    fn tokens(&self) -> Vec<String> {
        self.text
            .split(|c: char| !c.is_alphanumeric() && c != '.' && c != '_')
            .filter(|t| !t.is_empty())
            .map(|t| t.to_lowercase())
            .collect()
    }
}

/// A candidate call, and how much of it was remembered rather than inferred.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposed {
    /// ★ A **candidate**. Nothing here ran it.
    pub call: EnzymeCall,
    /// Fields whose value came from [`Effect::known`] — history or a prior
    /// answer. Never empty-and-silent: a caller can always say which.
    pub from_history: BTreeSet<String>,
}

/// What the proposer concluded. ★ Four outcomes, and none of them is a guess.
///
/// ★ Named `CallProposal`, not `Proposal` — [`crate::operative::Proposal`] is
/// what an operative puts forward (a `LegalMove` and who proposed it), a
/// genuinely different thing. **Thirty-fifth collision**; the newcomer takes
/// the longer name. Likewise [`propose_call`] rather than `propose`, since
/// [`crate::consensus::propose`] is a Paxos ballot — **thirty-fourth**.
#[derive(Debug, Clone, PartialEq)]
pub enum CallProposal {
    /// A complete candidate. Still needs a human's confirm to become real.
    Ready(Proposed),
    /// ★ One question, with the sustain's **own** values as the options where
    /// there are enumerable ones. `options` is empty for a free-form field
    /// (an amount is not a good tap-list), which is *missing information*
    /// rather than manufactured ambiguity.
    NeedsDisambiguation { field: String, question: String, options: Vec<String> },
    /// ★★ The description names something this definition does not have — an
    /// operator outside its allow-list, or one the registry does not carry.
    /// **Rejected, not fabricated.**
    Rejected { reason: String },
    /// Nothing could be inferred at all, and the reason says why.
    CannotInfer { reason: String },
}

impl CallProposal {
    pub fn ready(&self) -> Option<&Proposed> {
        match self {
            CallProposal::Ready(p) => Some(p),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            CallProposal::Ready(p) => format!(
                "candidate: {}({} field(s)), {} from history — needs a confirm",
                p.call.operator,
                p.call.params.len(),
                p.from_history.len()
            ),
            CallProposal::NeedsDisambiguation { field, question, options } => {
                format!("needs '{field}': {question} ({} option(s))", options.len())
            }
            CallProposal::Rejected { reason } => format!("rejected: {reason}"),
            CallProposal::CannotInfer { reason } => format!("cannot infer: {reason}"),
        }
    }
}

/// `ℐ(ε)` over a real definition, registry and state.
///
/// ★★ **Read-only.** It consults the definition's allow-list, the registry's
/// declarations and the state's own keys, and writes nothing anywhere.
pub fn propose_call(
    id: &str,
    effect: &Effect,
    definition: &Definition,
    registry: &Registry,
    state: &Value,
) -> CallProposal {
    let tokens = effect.tokens();

    // ── o ────────────────────────────────────────────────────────────────────
    // Candidates are the definition's allow-list ∩ the registry. An operator in
    // one and not the other is a real finding, not a silent skip.
    let mut candidates: Vec<&str> = Vec::new();
    for name in &definition.operators {
        if registry.get(name).is_some() {
            candidates.push(name.as_str());
        }
    }
    if candidates.is_empty() {
        return CallProposal::CannotInfer {
            reason: "this definition permits no operator the registry carries".to_string(),
        };
    }

    // An explicitly known operator is honoured outright — but only if the
    // definition actually permits it.
    if let Some(Value::String(named)) = effect.known.get("operator") {
        if !candidates.iter().any(|c| c == named) {
            return CallProposal::Rejected {
                reason: format!(
                    "'{named}' is not an operator this definition permits"
                ),
            };
        }
        candidates.retain(|c| c == named);
    } else {
        // ★ Narrow by the description's own words, against each operator's
        // name segments and its declared description.
        let narrowed: Vec<&str> =
            candidates.iter().copied().filter(|c| mentions(c, &tokens)).collect();
        // ★★ "A hint that eliminates EVERY candidate is treated as NO hint."
        if !narrowed.is_empty() {
            candidates = narrowed;
        }
    }

    if candidates.len() > 1 {
        return CallProposal::NeedsDisambiguation {
            field: "operator".to_string(),
            question: "what should this do?".to_string(),
            options: candidates.iter().map(|c| (*c).to_string()).collect(),
        };
    }

    let operator = candidates[0];
    let meta = registry.get(operator).expect("filtered against the registry above");

    if meta.params.is_empty() {
        // ★ An operator that declares nothing is not a licence to invent θ.
        return CallProposal::CannotInfer {
            reason: format!(
                "'{operator}' declares no parameters, so there is nothing to build θ from"
            ),
        };
    }

    // ── θ ────────────────────────────────────────────────────────────────────
    let reader = State::new(state.clone());
    let mut params = Map::new();
    let mut from_history = BTreeSet::new();

    for decl in &meta.params {
        // 1 — already known. Recorded, never silent.
        if let Some(v) = effect.known.get(decl.name.as_ref()) {
            if let Some(rejection) = reject_unknown_name(decl, v, &reader) {
                return rejection;
            }
            params.insert(decl.name.to_string(), v.clone());
            from_history.insert(decl.name.to_string());
            continue;
        }

        // 2 — a name drawn from the sustain's own live state.
        if let Some(path) = decl.names_within {
            let Some(live) = collection_keys(&reader, path) else {
                return CallProposal::Rejected {
                    reason: format!(
                        "'{}' names a value from '{path}', which this state does not have",
                        decl.name
                    ),
                };
            };
            let hits: Vec<String> =
                live.iter().filter(|k| tokens.iter().any(|t| t == &k.to_lowercase())).cloned().collect();
            match hits.len() {
                1 => {
                    params.insert(decl.name.to_string(), Value::String(hits[0].clone()));
                    continue;
                }
                // ★ 0 or many: ask, with the sustain's OWN values as options.
                _ => {
                    return CallProposal::NeedsDisambiguation {
                        field: decl.name.to_string(),
                        question: format!("which {} does this belong to?", decl.name),
                        options: if hits.is_empty() { live } else { hits },
                    }
                }
            }
        }

        // 3 — by kind.
        match decl.kind {
            ParamKind::Number => match first_number(&tokens) {
                Some(n) => {
                    params.insert(decl.name.to_string(), Value::Number(n));
                }
                None if decl.required => {
                    return CallProposal::NeedsDisambiguation {
                        field: decl.name.to_string(),
                        question: format!("what {} is this?", decl.name),
                        options: vec![],
                    }
                }
                None => {}
            },
            ParamKind::Text | ParamKind::Any if decl.required => {
                return CallProposal::NeedsDisambiguation {
                    field: decl.name.to_string(),
                    question: format!("what {} is this?", decl.name),
                    options: vec![],
                }
            }
            _ => {}
        }
    }

    CallProposal::Ready(Proposed {
        call: EnzymeCall { id: id.to_string(), operator: operator.to_string(), params },
        from_history,
    })
}

/// A known value that names a collection member must still BE one.
///
/// ★ History pre-fill is not a bypass: a remembered pocket that has since been
/// renamed away is rejected rather than proposed.
fn reject_unknown_name(decl: &ParamDecl, v: &Value, reader: &State) -> Option<CallProposal> {
    let path = decl.names_within?;
    let named = v.as_str()?;
    let live = collection_keys(reader, path)?;
    if live.iter().any(|k| k == named) {
        return None;
    }
    Some(CallProposal::Rejected {
        reason: format!("'{named}' is not present in '{path}' in this state"),
    })
}

/// The keys of an object at `path`, or `None` if there is no object there.
fn collection_keys(reader: &State, path: &str) -> Option<Vec<String>> {
    match reader.get(path) {
        Some(Value::Object(map)) => Some(map.keys().cloned().collect()),
        _ => None,
    }
}

/// Does the description mention this operator — by a **segment of its name**?
///
/// ★★ **A real correction, caught by a test rather than by inspection.** The
/// first version also matched words from the operator's own `description`, and
/// *"spend 50 from food"* then matched `budget.allocate` too — because its
/// description contains the word **"from"**. A description is prose written for
/// a human; using it as a matching vocabulary turns ordinary English into
/// operator hints, which is precisely the accidental inference this module
/// exists to refuse. **The name is the contract; the prose is not.** So the
/// vocabulary is the dotted name's own segments and nothing else.
fn mentions(operator: &str, tokens: &[String]) -> bool {
    let vocabulary: BTreeSet<String> = operator.split('.').map(str::to_lowercase).collect();
    tokens.iter().any(|t| vocabulary.contains(t))
}

/// The first token that parses as a number. Deliberately dull — a currency
/// grammar belongs to the transducer, which is app-layer.
fn first_number(tokens: &[String]) -> Option<Number> {
    tokens
        .iter()
        .find_map(|t| t.parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .and_then(Number::from_f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DimType, Schema};
    use crate::semantic::{replay_under, CallOutcome, ReplayMode};
    use serde_json::json;

    fn registry() -> Registry {
        Registry::default()
    }

    fn definition() -> Definition {
        Definition::new(
            Schema::new()
                .declare("finances.liquid.balance", DimType::Any)
                .declare("finances.pockets", DimType::Any),
        )
        .with_operator("budget.record_income")
        .with_operator("budget.allocate")
        .with_operator("budget.spend")
    }

    fn state() -> Value {
        json!({"finances": {
            "liquid": {"balance": 500.0},
            "pockets": {
                "food": {"allocated": 100.0, "spent": 20.0, "limit": 0.0},
                "rent": {"allocated": 200.0, "spent": 0.0, "limit": 0.0}
            },
            "income": {"monthly_total": 0.0, "sources": []}
        }})
    }

    fn propose_from(text: &str) -> CallProposal {
        propose_call("c1", &Effect::described(text), &definition(), &registry(), &state())
    }

    // ── ε → a correct (o, θ) ─────────────────────────────────────────────────

    #[test]
    fn a_described_effect_becomes_a_candidate_over_the_real_registry() {
        let p = propose_from("spend 50 from food");
        let ready = p.ready().unwrap_or_else(|| panic!("{}", p.describe()));
        assert_eq!(ready.call.operator, "budget.spend");
        assert_eq!(ready.call.params["pocket_name"], json!("food"));
        assert_eq!(ready.call.params["amount"], json!(50.0));
        assert!(ready.from_history.is_empty(), "nothing was remembered here");
    }

    #[test]
    fn the_pocket_is_matched_against_this_states_own_keys() {
        // ★★ Generic: the core never learns what a pocket is — the parameter
        // declares `names_within` and the keys come from the state.
        let p = propose_from("spend 30 from rent");
        assert_eq!(p.ready().unwrap().call.params["pocket_name"], json!("rent"));
    }

    #[test]
    fn a_proposal_never_carries_a_field_the_operator_does_not_declare() {
        let p = propose_from("allocate 40 to food");
        let ready = p.ready().unwrap_or_else(|| panic!("{}", p.describe()));
        let reg = registry();
        let declared: BTreeSet<&str> = reg
            .get("budget.allocate")
            .unwrap()
            .params
            .iter()
            .map(|d| d.name.as_ref())
            .collect();
        assert!(ready.call.params.keys().all(|k| declared.contains(k.as_str())));
    }

    // ── ambiguity is a named gap, never a guess ──────────────────────────────

    #[test]
    fn an_unnamed_pocket_is_a_named_gap_with_the_real_options() {
        let p = propose_from("spend 50 at naivas");
        match p {
            CallProposal::NeedsDisambiguation { field, options, .. } => {
                assert_eq!(field, "pocket_name");
                assert_eq!(options, vec!["food".to_string(), "rent".to_string()]);
            }
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn a_missing_amount_is_a_gap_with_no_fabricated_options() {
        match propose_from("spend from food") {
            CallProposal::NeedsDisambiguation { field, options, .. } => {
                assert_eq!(field, "amount");
                assert!(options.is_empty(), "an amount is not a tap-list");
            }
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn a_description_matching_nothing_asks_rather_than_reporting_no_match() {
        // ★★ "A hint that eliminates EVERY candidate is treated as NO hint."
        match propose_from("something happened") {
            CallProposal::NeedsDisambiguation { field, options, .. } => {
                assert_eq!(field, "operator");
                assert_eq!(options.len(), 3, "the full candidate set, not zero");
            }
            other => panic!("{}", other.describe()),
        }
    }

    // ── rejection, not fabrication ───────────────────────────────────────────

    #[test]
    fn an_operator_outside_the_definition_is_rejected() {
        let e = Effect::described("do it").knowing("operator", json!("budget.add_pocket"));
        match propose_call("c1", &e, &definition(), &registry(), &state()) {
            CallProposal::Rejected { reason } => assert!(reason.contains("budget.add_pocket")),
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn a_remembered_pocket_that_no_longer_exists_is_rejected_not_proposed() {
        // ★ History pre-fill is not a bypass.
        let e = Effect::described("spend 50")
            .knowing("operator", json!("budget.spend"))
            .knowing("pocket_name", json!("holiday"));
        match propose_call("c1", &e, &definition(), &registry(), &state()) {
            CallProposal::Rejected { reason } => assert!(reason.contains("holiday")),
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn an_operator_with_no_declared_params_cannot_have_a_theta_invented_for_it() {
        let d = Definition::new(Schema::new()).with_operator("test.force_negative");
        match propose_call("c1", &Effect::described("force it"), &d, &registry(), &state()) {
            CallProposal::CannotInfer { reason } => assert!(reason.contains("declares no parameters")),
            other => panic!("{}", other.describe()),
        }
    }

    // ── history discloses, and never skips the confirm ───────────────────────

    #[test]
    fn a_remembered_value_is_reported_as_remembered() {
        let e = Effect::described("spend 50").knowing("pocket_name", json!("food"));
        let p = propose_call("c1", &e, &definition(), &registry(), &state());
        let ready = p.ready().unwrap_or_else(|| panic!("{}", p.describe()));
        assert!(ready.from_history.contains("pocket_name"));
        assert!(!ready.from_history.contains("amount"), "that one was inferred");
    }

    // ── the output is a candidate, and EnzymeCall's producer ─────────────────

    #[test]
    fn proposing_runs_nothing_and_leaves_the_state_alone() {
        let s = state();
        let _ = propose_call("c1", &Effect::described("spend 50 from food"), &definition(), &registry(), &s);
        assert_eq!(s, state());
    }

    #[test]
    fn a_proposed_call_is_exactly_what_evt_15s_replay_consumes() {
        // ★★★ The discharge, end to end: `EnzymeCall` finally has a producer,
        // and `replay_under` finally has something it was not handed by hand.
        let p = propose_from("spend 50 from food");
        let call = p.ready().unwrap().call.clone();
        let outcome = replay_under(
            &[call],
            &registry(),
            &definition(),
            &state(),
            ReplayMode::Replay,
        );
        assert!(
            matches!(outcome.steps.first(), Some(CallOutcome::Admitted { operator, .. }) if operator == "budget.spend"),
            "{:?}",
            outcome.steps
        );
    }
}
