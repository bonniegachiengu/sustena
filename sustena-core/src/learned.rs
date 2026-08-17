//! Fixed and learned rules, and the bound the learned tier holds (IMM-10; the
//! Immune paper, §V).
//!
//! ## What this is, and what it deliberately is not
//!
//! ★★ **The ParseRule system is Python-only** — `ParseRule`, `parse_rule`,
//! `transducer`, `OTP` and `R_fixed` are all **grep-0** in this core, checked
//! before anything here was designed. So IMM-10's Rust work is *not* to port a
//! subsystem across just to have somewhere to hang a bound. It is the **generic
//! mechanism** §V is really about, of which a ParseRule is one instance: a
//! rule set split into a tier no principal can edit and a tier derived from
//! untrusted input, with the untrusted tier holding an **attenuated capability**
//! rather than ambient authority.
//!
//! Porting the regex library, the seed set and the adoption verifier stays
//! **app-layer** and out of scope. What a host gets from here is the split, the
//! ordering, and the bound.
//!
//! ## `R_fixed` — a fixed point of the edit operator
//!
//! > `∀e, ∀r ∈ R_fixed: e(r) = r` — no edit operation's range includes them.
//!
//! ★★ **Structural here, not enforced.** [`crate::editing::Edit::apply`] has
//! the signature `(&self, &Definition) -> Definition`: it acts on `𝒮`, `Inv`
//! and `T`, and there is **no parameter through which a rule set could enter
//! and no return through which one could leave**. A [`FixedRule`] is not part of
//! a `Definition`, so every edit is the identity on it — the same shape as
//! EVT-15's finding that the patch fold has no parameter for a definition. The
//! test suite runs all eight edit kinds and checks the fixed tier byte-for-byte
//! rather than resting on the argument alone.
//!
//! ★ And a [`FixedRule`] carries **no operator and no capability**, because a
//! pre-gate does not *act* — it refuses. The OTP secret filter drops a message;
//! it does not run an Enzyme. There is nothing to bound because there is
//! nothing it can reach.
//!
//! ## Ordered-before, structurally
//!
//! [`RuleSet::screen`] is the only entry point, it runs the fixed tier first,
//! and the order is **not a parameter**. There is no `screen_learned_first` and
//! no flag, so a caller cannot get the tiers the wrong way round — which
//! matters, because a learned rule matching an OTP before the secret filter saw
//! it is the whole failure §V exists to prevent.
//!
//! ## ★★ Least privilege on the learned tier — the keystone
//!
//! A learned rule derived from untrusted ingest maps text to an **operator
//! invocation**. With nothing constraining *which* operators it may target, the
//! learned set widens the effective attack surface every time it grows, without
//! review. That is precisely the configuration [`crate::capability`] was built
//! for: the untrusted tier is a **deputy**, and it should hold an attenuated
//! capability rather than the authority of whoever it is acting for.
//!
//! So [`IngressBound`] declares the operators reachable from ingest **at all**,
//! and [`RuleSet::capability_for`] attenuates a principal's own capability down
//! to that set. The capability machinery is IMM-7's and is not rebuilt; what is
//! new is *who holds one* and *how the bound is derived*.
//!
//! ★★★ **No trust level lifts the ingress bound.** Trust orders rules *within*
//! the bound and can only narrow further; there is no trust value that promotes
//! a learned rule out of the learned tier. Asserted directly, because a
//! least-privilege ceiling with an exception is not a ceiling.
//!
//! ## Trust as a real input, with its meaning declared
//!
//! §V's finding is that trust is *"a declared field awaiting an evaluator"* —
//! recorded, displayed, and driving no comparison, branch or ordering.
//! [`TrustPolicy`] is that evaluator, and it drives **two** real decisions:
//!
//! 1. **Precedence** — when two learned rules match, the stronger-trusted one
//!    wins ([`RuleSet::screen`]).
//! 2. **Privilege** — trust sets the tier ceiling on the capability the rule
//!    may hold ([`RuleSet::capability_for`]).
//!
//! ★ **`TrustPolicy` implements no `Default`.** What a given trust level ought
//! to be allowed to do is a policy statement about a particular deployment, and
//! a library that invented one would be making that decision invisibly — the
//! same reason [`crate::watermark::Lateness`] has no default. The host names
//! all three, and both decisions then derive from that **one** declared thing
//! rather than from two orderings that could disagree.
//!
//! ## Economy of mechanism
//!
//! HRU carries over from IMM-7: the reachable set is a small declared list, not
//! an access matrix. Two tiers, one policy, one bound.

use std::collections::BTreeSet;

use crate::capability::{Attenuation, Capability, Rights};
use crate::principal::Tier;

/// How a learned rule came to be believed — §V's three levels.
///
/// ★ Named `RuleTrust`, not `Trust` — [`crate::event::Trust`] is **source**
/// provenance (is this connector the authority for what it reports), a
/// genuinely different question from how a *rule* was adopted. Twenty-second
/// collision; the newcomer takes the longer name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuleTrust {
    /// Authored and reviewed before release.
    Shipped,
    /// A human explicitly corrected this, against their own real data.
    UserCorrected,
    /// Machine-synthesised, human-confirmed. The weakest of the three:
    /// confirming a proposal is cheaper than authoring a rule.
    ProposedConfirmed,
}

/// What each trust level is permitted to reach. **Declared, never inferred.**
///
/// No `Default`: see the module header. A host that has not stated its policy
/// does not have one, and this type will not invent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustPolicy {
    shipped: Tier,
    user_corrected: Tier,
    proposed_confirmed: Tier,
}

impl TrustPolicy {
    /// Name all three. There is no partial constructor, because a policy with a
    /// hole in it is a policy that fails open somewhere.
    pub fn declare(shipped: Tier, user_corrected: Tier, proposed_confirmed: Tier) -> Self {
        TrustPolicy { shipped, user_corrected, proposed_confirmed }
    }

    /// The weakest authority a rule at this trust may hold.
    pub fn ceiling_for(&self, trust: RuleTrust) -> Tier {
        match trust {
            RuleTrust::Shipped => self.shipped,
            RuleTrust::UserCorrected => self.user_corrected,
            RuleTrust::ProposedConfirmed => self.proposed_confirmed,
        }
    }
}

/// A rule no principal can modify — `R_fixed`.
///
/// It carries a matcher and **no operator**: a pre-gate refuses, it does not
/// act. The OTP secret filter drops a message rather than running an Enzyme, so
/// there is nothing here to bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedRule {
    pub id: String,
    /// A substring that, if present, rejects the input outright.
    ///
    /// Deliberately the dullest possible matcher: this core is not the place
    /// for the reference's regex library, and a richer matcher would invite
    /// porting one.
    pub rejects_containing: String,
}

impl FixedRule {
    pub fn new(id: impl Into<String>, rejects_containing: impl Into<String>) -> Self {
        FixedRule { id: id.into(), rejects_containing: rejects_containing.into() }
    }

    fn matches(&self, input: &str) -> bool {
        input.to_lowercase().contains(&self.rejects_containing.to_lowercase())
    }
}

/// A rule adopted into `R_learned` — derived from untrusted input, and
/// therefore bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedRule {
    pub id: String,
    /// A substring that, if present, selects this rule.
    pub matches_containing: String,
    /// The operator it would invoke. **Not itself an authorisation** — see
    /// [`RuleSet::capability_for`].
    pub emits: String,
    pub trust: RuleTrust,
}

impl LearnedRule {
    pub fn new(
        id: impl Into<String>,
        matches_containing: impl Into<String>,
        emits: impl Into<String>,
        trust: RuleTrust,
    ) -> Self {
        LearnedRule {
            id: id.into(),
            matches_containing: matches_containing.into(),
            emits: emits.into(),
            trust,
        }
    }

    fn matches(&self, input: &str) -> bool {
        input.to_lowercase().contains(&self.matches_containing.to_lowercase())
    }
}

/// The operators reachable from ingest **at all** — least privilege, declared.
///
/// Small by intent (HRU): a list a person can read, not a matrix a person must
/// query.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IngressBound {
    reachable: BTreeSet<String>,
}

impl IngressBound {
    /// Nothing is reachable from ingest. The safe starting point, and a real
    /// choice a host may keep.
    pub fn nothing() -> Self {
        Self::default()
    }

    pub fn declare<I, S>(operators: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        IngressBound { reachable: operators.into_iter().map(Into::into).collect() }
    }

    pub fn reaches(&self, operator: &str) -> bool {
        self.reachable.contains(operator)
    }

    pub fn operators(&self) -> impl Iterator<Item = &str> {
        self.reachable.iter().map(String::as_str)
    }
}

/// What screening an input decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screening {
    /// `R_fixed` rejected it. Nothing downstream runs — and the rule that did
    /// it is named, because a rejection nobody can attribute is an alarm.
    Rejected { by: String },
    /// A learned rule selected it. **This is a match, not an authorisation** —
    /// the capability is a separate question.
    Matched { rule_id: String, operator: String, trust: RuleTrust },
    /// Nothing matched. Distinct from rejected: *we do not recognise this* and
    /// *this must never be processed* are different findings.
    Unmatched,
}

/// Why a learned rule could not be given a capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundError {
    /// The rule wants an operator ingest may not reach at all.
    OutsideIngressBound { rule_id: String, operator: String },
    /// A fixed rule was asked for a capability. It performs no action, so there
    /// is nothing to authorise — asked-and-refused rather than silently handed
    /// an empty one, because an empty capability would read as *bounded* where
    /// the truth is *not applicable*.
    FixedRulesDoNotAct { rule_id: String },
    /// Attenuating the base capability down to the bound would have widened it.
    WouldAmplify(crate::capability::Amplification),
}

impl std::fmt::Display for BoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundError::OutsideIngressBound { rule_id, operator } => write!(
                f,
                "learned rule '{rule_id}' would invoke '{operator}', which is not \
                 reachable from ingest"
            ),
            BoundError::FixedRulesDoNotAct { rule_id } => write!(
                f,
                "'{rule_id}' is a fixed rule: it refuses input and invokes nothing, \
                 so there is no authority to grant"
            ),
            BoundError::WouldAmplify(a) => write!(f, "{a}"),
        }
    }
}

/// `R_fixed ∪ R_learned`, with the ordering and the bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    fixed: Vec<FixedRule>,
    learned: Vec<LearnedRule>,
    bound: IngressBound,
    policy: TrustPolicy,
}

impl RuleSet {
    pub fn new(bound: IngressBound, policy: TrustPolicy) -> Self {
        RuleSet { fixed: Vec::new(), learned: Vec::new(), bound, policy }
    }

    pub fn with_fixed(mut self, rule: FixedRule) -> Self {
        self.fixed.push(rule);
        self
    }

    /// Adopt a learned rule.
    ///
    /// ★ Deliberately does **not** refuse a rule outside the ingress bound.
    /// Adoption is one question and authority is another, and collapsing them
    /// would hide an adopted rule that can never fire — better that it is
    /// visible in the set and refused at [`capability_for`], where the reason
    /// names the bound.
    ///
    /// [`capability_for`]: RuleSet::capability_for
    pub fn with_learned(mut self, rule: LearnedRule) -> Self {
        self.learned.push(rule);
        self
    }

    pub fn fixed(&self) -> &[FixedRule] {
        &self.fixed
    }

    pub fn learned(&self) -> &[LearnedRule] {
        &self.learned
    }

    pub fn bound(&self) -> &IngressBound {
        &self.bound
    }

    /// The learned tier in precedence order — strongest authority first.
    ///
    /// ★ The ordering comes from the **declared policy's ceiling**, not from a
    /// second ordering that could drift out of step with it. Ties keep the
    /// set's own order, stably: the order a host wrote its rules in is a real
    /// statement and is not worth scrambling.
    pub fn by_precedence(&self) -> Vec<&LearnedRule> {
        let mut ordered: Vec<&LearnedRule> = self.learned.iter().collect();
        // Lower tier number is MORE authority, so ascending is strongest-first.
        ordered.sort_by_key(|r| self.policy.ceiling_for(r.trust));
        ordered
    }

    /// Screen an input. **`R_fixed` runs first, and the order is not a
    /// parameter.**
    pub fn screen(&self, input: &str) -> Screening {
        for rule in &self.fixed {
            if rule.matches(input) {
                return Screening::Rejected { by: rule.id.clone() };
            }
        }
        for rule in self.by_precedence() {
            if rule.matches(input) {
                return Screening::Matched {
                    rule_id: rule.id.clone(),
                    operator: rule.emits.clone(),
                    trust: rule.trust,
                };
            }
        }
        Screening::Unmatched
    }

    /// ★★ The attenuated capability a learned rule may hold.
    ///
    /// `base` is the principal's own capability over the target sustain. The
    /// result is attenuated **twice**: rights narrowed to the ingress bound,
    /// and tier weakened to whatever the declared policy allows this trust
    /// level. Both are narrowings of `base`, so a rule can never hold more than
    /// the principal it acts for — which is the whole point of handing it a
    /// capability instead of the principal's own authority.
    pub fn capability_for(
        &self,
        rule: &LearnedRule,
        base: &Capability,
    ) -> Result<Capability, BoundError> {
        if !self.bound.reaches(&rule.emits) {
            return Err(BoundError::OutsideIngressBound {
                rule_id: rule.id.clone(),
                operator: rule.emits.clone(),
            });
        }

        // ★★★ Rights are the WHOLE ingress bound, not just this rule's own
        // operator — and the ceiling applies at every trust level. There is no
        // branch here that lets a trust value reach outside `self.bound`.
        let ceiling = self.policy.ceiling_for(rule.trust);
        let request = Attenuation::to_rights(Rights::Only(
            self.bound.operators().map(str::to_string).collect(),
        ))
        .and_tier(ceiling.max(base.tier()));

        base.attenuate(&request).map_err(BoundError::WouldAmplify)
    }

    /// A fixed rule has no capability, and asking is an error rather than an
    /// empty answer.
    pub fn capability_for_fixed(&self, rule: &FixedRule) -> Result<Capability, BoundError> {
        Err(BoundError::FixedRulesDoNotAct { rule_id: rule.id.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::principal::{
        MembershipEdge, Memberships, SkinRegistry, TIER_CONTRIBUTOR, TIER_MEMBER, TIER_OBSERVER,
        TIER_OWNER,
    };

    fn policy() -> TrustPolicy {
        TrustPolicy::declare(TIER_MEMBER, TIER_CONTRIBUTOR, TIER_OBSERVER)
    }

    fn bound() -> IngressBound {
        IngressBound::declare(["budget.record_income", "budget.spend"])
    }

    fn memberships() -> Memberships {
        let mut m = Memberships::new();
        m.grant(MembershipEdge {
            principal: "bonnie".into(),
            sustain: "habitat".into(),
            tier: TIER_OWNER,
            skin: None,
        });
        m
    }

    fn base_capability() -> Capability {
        Capability::issue(&memberships(), SkinRegistry::none(), "bonnie", "habitat").unwrap()
    }

    fn set() -> RuleSet {
        RuleSet::new(bound(), policy())
            .with_fixed(FixedRule::new("otp", "one-time password"))
            .with_learned(LearnedRule::new(
                "kcb_receive",
                "received",
                "budget.record_income",
                RuleTrust::Shipped,
            ))
    }

    // ── R_fixed ──────────────────────────────────────────────────────────────

    #[test]
    fn the_fixed_tier_runs_first_and_the_order_is_not_a_parameter() {
        // An input that BOTH the OTP filter and a learned rule would match.
        // There is no `screen_learned_first`, so this cannot come out the other
        // way round.
        let s = set().with_learned(LearnedRule::new(
            "greedy",
            "password",
            "budget.spend",
            RuleTrust::Shipped,
        ));
        assert_eq!(
            s.screen("Your one-time password is 123456, received now"),
            Screening::Rejected { by: "otp".into() }
        );
    }

    #[test]
    fn the_fixed_tier_is_a_fixed_point_of_every_edit_kind() {
        // ★★ `Edit::apply` is `(&Definition) -> Definition`: no parameter
        // through which a rule set could enter. Exercised over all eight kinds
        // rather than argued from the signature alone.
        use crate::editing::{Definition, Edit};
        use crate::schema::{DimType, Schema};
        use serde_json::json;

        let s = set();
        let before = s.fixed().to_vec();
        let d = Definition::new(Schema::new().declare("x", DimType::Number { lo: None, hi: None }))
            .with_invariant("i", "x >= 0")
            .with_operator("budget.spend");

        let edits = vec![
            Edit::AddDim { name: "y".into(), ty: DimType::Number { lo: None, hi: None }, default: json!(0) },
            Edit::RetypeDim { name: "x".into(), ty: DimType::Any },
            Edit::RetireDim { name: "x".into() },
            Edit::AddInv { id: "j".into(), expression: "x >= 1".into() },
            Edit::DropInv { id: "i".into() },
            Edit::ModifyInv { id: "i".into(), expression: "x >= 2".into() },
            Edit::AddOp { name: "budget.allocate".into() },
            Edit::RetireOp { name: "budget.spend".into() },
        ];
        for e in &edits {
            let _ = e.apply(&d);
        }
        assert_eq!(s.fixed(), before.as_slice(), "∀e, ∀r ∈ R_fixed: e(r) = r");
    }

    #[test]
    fn a_fixed_rule_has_no_capability_and_asking_is_an_error() {
        let s = set();
        let rule = s.fixed()[0].clone();
        assert_eq!(
            s.capability_for_fixed(&rule),
            Err(BoundError::FixedRulesDoNotAct { rule_id: "otp".into() }),
            "an empty capability would read as bounded where the truth is not-applicable"
        );
    }

    #[test]
    fn unmatched_is_a_different_answer_from_rejected() {
        assert_eq!(set().screen("nothing here"), Screening::Unmatched);
    }

    // ── trust as a real input ────────────────────────────────────────────────

    #[test]
    fn trust_decides_precedence_when_two_learned_rules_match() {
        let s = RuleSet::new(bound(), policy())
            .with_learned(LearnedRule::new(
                "weak",
                "paid",
                "budget.spend",
                RuleTrust::ProposedConfirmed,
            ))
            .with_learned(LearnedRule::new(
                "strong",
                "paid",
                "budget.record_income",
                RuleTrust::Shipped,
            ));

        // Declaration order puts the weak one first; trust puts the strong one
        // in front. This is the comparison §V said was missing.
        match s.screen("paid to naivas") {
            Screening::Matched { rule_id, trust, .. } => {
                assert_eq!(rule_id, "strong");
                assert_eq!(trust, RuleTrust::Shipped);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn trust_decides_the_privilege_ceiling() {
        let s = RuleSet::new(bound(), policy())
            .with_learned(LearnedRule::new("a", "x", "budget.spend", RuleTrust::Shipped))
            .with_learned(LearnedRule::new(
                "b",
                "x",
                "budget.spend",
                RuleTrust::ProposedConfirmed,
            ));
        let base = base_capability();

        let strong = s.capability_for(&s.learned()[0], &base).unwrap();
        let weak = s.capability_for(&s.learned()[1], &base).unwrap();
        assert_eq!(strong.tier(), TIER_MEMBER);
        assert_eq!(weak.tier(), TIER_OBSERVER);
        assert!(weak.tier() > strong.tier(), "less trust, less authority");
    }

    #[test]
    fn ties_keep_the_declared_order_stably() {
        let s = RuleSet::new(bound(), policy())
            .with_learned(LearnedRule::new("first", "x", "budget.spend", RuleTrust::Shipped))
            .with_learned(LearnedRule::new("second", "x", "budget.spend", RuleTrust::Shipped));
        assert_eq!(
            s.by_precedence().iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["first", "second"]
        );
    }

    // ── least privilege ──────────────────────────────────────────────────────

    #[test]
    fn a_rule_outside_the_ingress_bound_gets_no_capability() {
        let s = RuleSet::new(bound(), policy()).with_learned(LearnedRule::new(
            "sneaky",
            "x",
            "egress.prepare_household_summary",
            RuleTrust::Shipped,
        ));
        assert_eq!(
            s.capability_for(&s.learned()[0], &base_capability()),
            Err(BoundError::OutsideIngressBound {
                rule_id: "sneaky".into(),
                operator: "egress.prepare_household_summary".into(),
            })
        );
    }

    #[test]
    fn no_trust_level_lifts_the_ingress_bound() {
        // ★★★ The keystone. A ceiling with an exception is not a ceiling.
        let s = RuleSet::new(bound(), policy());
        let base = base_capability();
        for trust in [RuleTrust::Shipped, RuleTrust::UserCorrected, RuleTrust::ProposedConfirmed] {
            let rule = LearnedRule::new("r", "x", "budget.spend", trust);
            let cap = s.capability_for(&rule, &base).unwrap();
            assert!(cap.rights().carries("budget.spend"));
            assert!(
                !cap.rights().carries("egress.prepare_household_summary"),
                "{trust:?} must not reach outside the bound"
            );
        }
    }

    #[test]
    fn an_empty_bound_reaches_nothing_at_all() {
        let s = RuleSet::new(IngressBound::nothing(), policy());
        let rule = LearnedRule::new("r", "x", "budget.spend", RuleTrust::Shipped);
        assert!(matches!(
            s.capability_for(&rule, &base_capability()),
            Err(BoundError::OutsideIngressBound { .. })
        ));
    }

    #[test]
    fn the_bound_never_exceeds_the_principal_it_acts_for() {
        // A capability is a transfer, so an observer's rule cannot outrank the
        // observer — the ceiling takes the weaker of policy and base.
        let mut m = Memberships::new();
        m.grant(MembershipEdge {
            principal: "guest".into(),
            sustain: "habitat".into(),
            tier: TIER_OBSERVER,
            skin: None,
        });
        let base = Capability::issue(&m, SkinRegistry::none(), "guest", "habitat").unwrap();

        let s = RuleSet::new(bound(), policy());
        // Shipped's ceiling is MEMBER, which is STRONGER than the base holder.
        let rule = LearnedRule::new("r", "x", "budget.spend", RuleTrust::Shipped);
        let cap = s.capability_for(&rule, &base).unwrap();
        assert_eq!(cap.tier(), TIER_OBSERVER, "the weaker of the two wins");
    }
}
