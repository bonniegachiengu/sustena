//! **`Σ = ⟨B, S, V, T, ⊕⟩`** — the one recursive object (Sustain · CELL §I).
//!
//! ★★★ **Every component existed; the OBJECT did not.** `boundary.rs` is `B`,
//! `schema.rs` is `S`, `region.rs` is `V`, the registry and `Definition` are
//! `T`, and `rollup.rs` composes — five real things, assembled by whichever
//! caller needed them, in whatever combination it happened to want. A household
//! was a `Definition` here, a `Region` there, and a list of children somewhere
//! else, and nothing anywhere held the claim that those are one thing.
//!
//! ★★★ **"Components of a Sustain are Sustains" is the whole article, and it
//! only becomes true when one type says so.** A habitat, a household and a
//! village are the same kind of object at different scales — so an aggregate
//! over habitats and an aggregate over households need no separate machinery,
//! and a village can be adopted into a county without anything being rewritten.
//! Two types, one for "a Sustain" and one for "a Sustain that has children",
//! would put a ceiling in the model that the world does not have.
//!
//! ★★★ **No privileged top, and here that is structural rather than a
//! convention.** There is no `Root` type and no `is_root` flag: a root is
//! simply a `Σ` nobody has made a child of yet. The moment somebody does, it is
//! a child, and nothing about it changes.
//!
//! ## What the shape buys for free
//!
//! ★★ **A cycle is unrepresentable.** Children are *owned*, so a Sustain cannot
//! contain itself — the compiler refuses to build the value. Every other part of
//! this codebase that walks a holon has to guard against a loop; this one cannot
//! have one to guard against.
//!
//! ★★ **Depth is not a special case.** `path_to` and `typecheck` recurse, so a
//! village of households of habitats works because it is the same three lines,
//! not because somebody remembered to handle three levels.

use crate::editing::Definition;
use crate::schema::{Schema, TypeError};

/// One Sustain, whatever its scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Sigma {
    pub id: String,
    /// `S`, `V` and `T` together — the definition already holds all three.
    ///
    /// ★★ Not split into three fields. They are checked together, versioned
    /// together and edited together, and a shape that separated them would
    /// invite an edit to one that the other two never saw.
    pub definition: Definition,
    /// `⊕` — and they are `Σ`s, which is the article's whole claim.
    pub children: Vec<Sigma>,
}

impl Sigma {
    pub fn new(id: &str, definition: Definition) -> Self {
        Self { id: id.into(), definition, children: Vec::new() }
    }

    /// ★★★ Takes a `Sigma`, not a lesser type. Adopting a village into a county
    /// is the same act as adopting a habitat into a household.
    pub fn with_child(mut self, child: Sigma) -> Self {
        self.children.push(child);
        self
    }

    /// Every Sustain in this tree, outermost first.
    pub fn walk(&self) -> Vec<&Sigma> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.walk());
        }
        out
    }

    pub fn find(&self, id: &str) -> Option<&Sigma> {
        self.walk().into_iter().find(|s| s.id == id)
    }

    /// The membership path from here down to `id`, inclusive at both ends.
    ///
    /// ★★★ This is what `⋀_{H∈path} admit_H` walks. It is a plain recursion
    /// because the structure is a tree; a graph would need a visited-set and a
    /// cycle check, and the shape is what removes both.
    pub fn path_to(&self, id: &str) -> Option<Vec<&Sigma>> {
        if self.id == id {
            return Some(vec![self]);
        }
        for child in &self.children {
            if let Some(mut rest) = child.path_to(id) {
                let mut path = vec![self];
                path.append(&mut rest);
                return Some(path);
            }
        }
        None
    }

    /// How deep this tree goes. A leaf is 1.
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(Sigma::depth).max().unwrap_or(0)
    }

    /// **Type-check this Sustain and everything under it, as one program.**
    ///
    /// ★★★ Including the holon path: a parent's aggregate is only sound if its
    /// *children* declare what it reads, which is a fact neither of them holds
    /// alone. Checking each Sustain separately would pass a household whose
    /// total silently ranges over a dimension no habitat has.
    ///
    /// ★★ Every finding is collected and named by the Sustain it came from. A
    /// tree of thirty households reporting "invalid" would be a report nobody
    /// can act on.
    pub fn typecheck(&self, registry: &crate::operator::Registry) -> Result<(), Vec<TypeError>> {
        let mut errors = Vec::new();
        self.check_into(registry, &mut errors);
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    fn check_into(&self, registry: &crate::operator::Registry, errors: &mut Vec<TypeError>) {
        let schemas: Vec<(&str, &Schema)> =
            self.children.iter().map(|c| (c.id.as_str(), &c.definition.schema)).collect();
        let context = crate::spec::SpecContext { children: schemas };

        if let Err(found) = crate::spec::validate_spec(&self.definition, registry, &context) {
            for mut e in found {
                e.detail = format!("{}: {}", self.id, e.detail);
                errors.push(e);
            }
        }
        for child in &self.children {
            child.check_into(registry, errors);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::Registry;
    use crate::schema::DimType;

    fn number() -> DimType {
        DimType::Number { lo: None, hi: None }
    }

    fn finances() -> Schema {
        Schema::new().declare(
            "finances",
            DimType::Record {
                fields: [(
                    "liquid".to_string(),
                    DimType::Record {
                        fields: [("balance".to_string(), number())].into_iter().collect(),
                    },
                )]
                .into_iter()
                .collect(),
            },
        )
    }

    fn plain(id: &str) -> Sigma {
        Sigma::new(id, Definition::new(finances()))
    }

    /// A village of households of habitats — three scales, one type.
    fn village() -> Sigma {
        plain("the village")
            .with_child(
                plain("the Gachiengu household")
                    .with_child(plain("Bonnie"))
                    .with_child(plain("Aida")),
            )
            .with_child(plain("the Otieno household").with_child(plain("Jack")))
    }

    #[test]
    fn a_habitat_a_household_and_a_village_are_the_same_kind_of_thing() {
        // ★★★ The article's whole claim, and it only becomes true when one type
        //     says so. Two types — one for "a Sustain", one for "a Sustain with
        //     children" — would put a ceiling in the model the world does not
        //     have.
        let v = village();
        assert_eq!(v.depth(), 3);
        assert_eq!(v.walk().len(), 6);
        // And a village adopts into a county by exactly the same call.
        let county = plain("the county").with_child(v);
        assert_eq!(county.depth(), 4);
    }

    #[test]
    fn a_root_is_just_one_nobody_has_made_a_child_of_yet() {
        // ★★★ No `Root` type and no `is_root` flag. The moment somebody adopts
        //     it, it is a child, and nothing about it changes.
        let household = plain("the Gachiengu household").with_child(plain("Bonnie"));
        let alone = household.clone();
        let adopted = plain("the village").with_child(household);
        let found = adopted.find("the Gachiengu household").expect("found");
        assert_eq!(&alone, found, "the same value either way");
    }

    #[test]
    fn the_membership_path_is_what_admissibility_walks() {
        // ★★ `⋀_{H∈path} admit_H` needs exactly this, outermost first.
        let v = village();
        let path = v.path_to("Aida").expect("reachable");
        let ids: Vec<&str> = path.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["the village", "the Gachiengu household", "Aida"]);
    }

    #[test]
    fn a_sustain_that_is_not_in_this_tree_has_no_path() {
        let v = village();
        assert!(v.path_to("somebody else").is_none());
    }

    #[test]
    fn the_path_to_itself_is_itself() {
        // ★★ A Sustain with no parents is not a Sustain with no path — its own
        //    gate is the whole of it.
        let v = village();
        let path = v.path_to("the village").expect("reachable");
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn depth_is_not_a_special_case_at_any_level() {
        assert_eq!(plain("alone").depth(), 1);
        assert_eq!(plain("a").with_child(plain("b")).depth(), 2);
        assert_eq!(village().depth(), 3);
    }

    #[test]
    fn a_well_formed_tree_type_checks_at_every_level() {
        assert!(village().typecheck(&Registry::default()).is_ok());
    }

    #[test]
    fn a_finding_names_the_sustain_it_came_from() {
        // ★★★ A tree of thirty households reporting "invalid" is a report
        //     nobody can act on.
        let broken = plain("the village")
            .with_child(Sigma::new("Aida", Definition::new(finances()).with_operator("budget.spned")));
        let errors = broken.typecheck(&Registry::default()).expect_err("refuses");
        assert!(errors[0].detail.starts_with("Aida:"), "{:?}", errors[0]);
    }

    #[test]
    fn a_parents_aggregate_is_checked_against_its_real_children() {
        // ★★★ A fact neither holds alone. Checking each Sustain separately
        //     would pass a household whose total silently ranges over a
        //     dimension no habitat has — and that reads as a household with no
        //     money rather than as a mistake.
        let household = Sigma::new(
            "the household",
            Definition::new(finances())
                .with_aggregate("total", "finances.savings.balance", "sum")
                .expect("declares"),
        )
        .with_child(plain("Bonnie"));
        let errors = household.typecheck(&Registry::default()).expect_err("refuses");
        assert!(
            errors.iter().any(|e| e.detail.contains("does not declare")),
            "{errors:?}",
        );
    }

    #[test]
    fn an_aggregate_its_children_do_declare_is_fine() {
        let household = Sigma::new(
            "the household",
            Definition::new(finances())
                .with_aggregate("total", "finances.liquid.balance", "sum")
                .expect("declares"),
        )
        .with_child(plain("Bonnie"));
        assert!(household.typecheck(&Registry::default()).is_ok());
    }

    #[test]
    fn a_childless_sustain_is_checked_as_itself_and_not_skipped() {
        let alone =
            Sigma::new("alone", Definition::new(finances()).with_operator("nope.missing"));
        assert!(alone.typecheck(&Registry::default()).is_err());
    }
}
