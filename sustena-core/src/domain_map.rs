//! **`domain : R → {clear, complicated, complex, chaotic}`** — a domain per
//! *region* of `S` (Sustain · CELL §X).
//!
//! ★★★ **`cynefin.rs` had the taxonomy and nothing that could produce a
//! reading from a state.** It could say what each domain presupposes and refuse
//! a mismatched operative — but the classifier's own doc says plainly that
//! inventing one would be *a confident classification from nowhere*. So the
//! reading had to come from somewhere, and the article says where: **the
//! household declares it, per region of its own state space.**
//!
//! ★★★ **A household is not in one domain.** Its rent is clear — there is a
//! best practice and it works. A new side business is complex — you probe, you
//! see what happens, you amplify what worked. Treating the household as one
//! regime forces one method onto both, and Conant–Ashby's *good regulator is a
//! model of the system* is exactly the claim that a one-domain model of a
//! many-domain household regulates it badly.
//!
//! ## Disorder is detected, never defaulted
//!
//! ★★★ **A state no declared region covers is DISORDER**, Cynefin's fifth
//! state, and it is the one that has to be surfaced rather than filled in.
//! Defaulting to `clear` would be the worst possible guess: *clear* licenses
//! best practice, so an unrecognised situation would be met with the method
//! that presupposes it is already understood. Disorder means *we do not know
//! which rules apply*, and saying so is what lets somebody find out.
//!
//! ★★★ **Two declarations that disagree are also disorder.** Not a
//! precedence puzzle to resolve by order-of-declaration — genuinely *we hold
//! two incompatible models of this situation*, which is precisely the failure
//! mode §X names. Picking the first would hide a real contradiction behind a
//! confident answer.
//!
//! ★★ Two declarations that AGREE are not a conflict. A state can sit inside
//! two overlapping descriptions of the same regime, and calling that a
//! contradiction would make the map unusable for any household whose regions
//! are not a perfect partition.

use serde_json::Value;

use crate::cynefin::DomainReading;
use crate::operative::Cynefin;
use crate::predicate::{eval::evaluate, parse_predicate};
use crate::schema::{bind, Schema, TypeError};

/// One region of `S`, and what kind of situation it is.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainRegion {
    pub id: String,
    /// The predicate that says a state is in this region.
    pub when: String,
    pub domain: Cynefin,
}

impl DomainRegion {
    pub fn new(id: &str, when: &str, domain: Cynefin) -> Self {
        Self { id: id.into(), when: when.into(), domain }
    }
}

/// `domain : R → Cynefin`, as a household declares it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DomainMap {
    regions: Vec<DomainRegion>,
}

impl DomainMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn covering(mut self, region: DomainRegion) -> Self {
        self.regions.push(region);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Every region declared, in declaration order.
    pub fn regions(&self) -> &[DomainRegion] {
        &self.regions
    }

    /// **Load-time check** — a region whose predicate cannot be read or names a
    /// dimension the schema does not declare.
    ///
    /// ★★ The same discipline every other declared predicate gets. A domain map
    /// that silently never matches would leave a household permanently in
    /// disorder for a reason nobody could see.
    pub fn typecheck(&self, schema: &Schema) -> Vec<TypeError> {
        let mut errors = Vec::new();
        for region in &self.regions {
            match parse_predicate(&region.when) {
                Err(e) => errors.push(TypeError {
                    path: region.id.clone(),
                    detail: format!("this region cannot be read: {e}"),
                }),
                Ok(node) => {
                    for mut err in bind(&node, schema) {
                        err.detail = format!("region '{}': {}", region.id, err.detail);
                        errors.push(err);
                    }
                }
            }
        }
        errors
    }

    /// **Which kind of situation is this?**
    ///
    /// ★★★ Disorder is an answer. It arrives when nothing covers the state, or
    /// when two declarations disagree about it, and either way saying so is
    /// what lets somebody find out which rules apply.
    pub fn read(&self, state: &Value) -> DomainReading {
        let empty = serde_json::Map::new();
        let mut matched: Vec<&DomainRegion> = Vec::new();

        for region in &self.regions {
            let Ok(node) = parse_predicate(&region.when) else {
                // ★★ An unreadable region cannot claim the state, and it cannot
                //    be silently skipped either — `typecheck` is where that is
                //    reported, and this stays honest by not matching.
                continue;
            };
            if evaluate(&node, state, &empty).0 {
                matched.push(region);
            }
        }

        match matched.as_slice() {
            [] => DomainReading::indeterminate(
                "no declared region covers this state — disorder, not clear",
            ),
            [only] => DomainReading::known(only.domain),
            many => {
                let first = many[0].domain;
                if many.iter().all(|r| r.domain == first) {
                    // ★★ Overlapping descriptions of ONE regime. Not a
                    //    contradiction, and calling it one would make the map
                    //    unusable for any household whose regions are not a
                    //    perfect partition.
                    DomainReading::known(first)
                } else {
                    let names: Vec<String> = many
                        .iter()
                        .map(|r| format!("{} says {}", r.id, r.domain.name()))
                        .collect();
                    DomainReading::indeterminate(&format!(
                        "two declarations disagree about this state — {}",
                        names.join("; ")
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn number() -> crate::schema::DimType {
        crate::schema::DimType::Number { lo: None, hi: None }
    }

    fn schema() -> Schema {
        Schema::new().declare("rent_months_paid", number()).declare("new_venture_age", number())
    }

    /// A household whose rent is routine and whose side business is not.
    fn household_map() -> DomainMap {
        DomainMap::new()
            .covering(DomainRegion::new("the rent", "rent_months_paid >= 6", Cynefin::Clear))
            .covering(DomainRegion::new(
                "the new venture",
                "new_venture_age <= 3",
                Cynefin::Complex,
            ))
    }

    #[test]
    fn a_household_can_be_in_two_domains_at_once_in_different_regions() {
        // ★★★ The row's whole point. Rent is clear; a new side business is
        //     complex. Treating the household as one regime forces one method
        //     onto both.
        let map = household_map();
        assert_eq!(
            map.read(&json!({"rent_months_paid": 12, "new_venture_age": 99})).domain(),
            Some(Cynefin::Clear),
        );
        assert_eq!(
            map.read(&json!({"rent_months_paid": 0, "new_venture_age": 1})).domain(),
            Some(Cynefin::Complex),
        );
    }

    #[test]
    fn a_state_nothing_covers_is_disorder_and_never_clear() {
        // ★★★ The dangerous default. `clear` licenses best practice, so an
        //     unrecognised situation would be met with the method that
        //     presupposes it is already understood.
        let reading = household_map().read(&json!({"rent_months_paid": 1, "new_venture_age": 40}));
        assert_eq!(reading.domain(), None);
        match reading {
            DomainReading::Indeterminate { why } => {
                assert!(why.contains("disorder"), "{why}");
                assert!(why.contains("not clear"), "and says which mistake it is avoiding");
            }
            other => panic!("expected disorder: {other:?}"),
        }
    }

    #[test]
    fn two_declarations_that_disagree_are_disorder_not_a_precedence_puzzle() {
        // ★★★ Genuinely "we hold two incompatible models of this situation",
        //     which is exactly the failure mode §X names. Picking the first
        //     would hide a real contradiction behind a confident answer.
        let map = DomainMap::new()
            .covering(DomainRegion::new("optimist", "n >= 0", Cynefin::Clear))
            .covering(DomainRegion::new("pessimist", "n >= 0", Cynefin::Chaotic));
        match map.read(&json!({"n": 5})) {
            DomainReading::Indeterminate { why } => {
                assert!(why.contains("disagree"), "{why}");
                assert!(why.contains("optimist") && why.contains("pessimist"), "{why}");
            }
            other => panic!("expected disorder: {other:?}"),
        }
    }

    #[test]
    fn two_declarations_that_agree_are_not_a_contradiction() {
        // ★★ A state can sit inside two overlapping descriptions of one regime.
        //    Calling that a conflict would make the map unusable for any
        //    household whose regions are not a perfect partition.
        let map = DomainMap::new()
            .covering(DomainRegion::new("a", "n >= 0", Cynefin::Complicated))
            .covering(DomainRegion::new("b", "n >= 5", Cynefin::Complicated));
        assert_eq!(map.read(&json!({"n": 10})).domain(), Some(Cynefin::Complicated));
    }

    #[test]
    fn a_household_that_declared_nothing_is_in_disorder_rather_than_in_clear() {
        // ★★★ Having declared no model is not the same as having a simple one.
        assert_eq!(DomainMap::new().read(&json!({"n": 1})).domain(), None);
    }

    #[test]
    fn a_region_naming_a_dimension_that_does_not_exist_is_caught_at_load() {
        // ★★ Otherwise it silently never matches, and the household sits in
        //    disorder for a reason nobody can see.
        let map = DomainMap::new().covering(DomainRegion::new(
            "typo",
            "rent_months_pade >= 6",
            Cynefin::Clear,
        ));
        let errors = map.typecheck(&schema());
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].detail.contains("undeclared"));
    }

    #[test]
    fn a_region_that_cannot_be_read_is_reported_and_never_matches() {
        let map = DomainMap::new().covering(DomainRegion::new("broken", "n >>>", Cynefin::Clear));
        assert!(!map.typecheck(&schema()).is_empty());
        assert_eq!(map.read(&json!({"n": 1})).domain(), None, "and it claims nothing");
    }

    #[test]
    fn a_well_formed_map_passes_its_load_check() {
        assert!(household_map().typecheck(&schema()).is_empty());
    }

    #[test]
    fn the_reading_carries_the_response_mode_that_goes_with_it() {
        // ★★ A domain is not a label — it is a claim about which method works.
        //    The reading is only useful if that claim travels with it.
        let d = household_map()
            .read(&json!({"rent_months_paid": 0, "new_venture_age": 1}))
            .domain()
            .expect("known");
        assert_eq!(d.response_mode(), crate::cynefin::ResponseMode::ProbeSenseRespond);
    }
}
