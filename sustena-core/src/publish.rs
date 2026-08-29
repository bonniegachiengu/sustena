//! **Publish is an Enzyme, not an upload** (Arena · §III).
//!
//! ```text
//!   g_publish = auth(author) ∧ ¬bound(name, v) ∧ Verify(σ) ∧ typecheck(spec) ∧ declares(ν)
//! ```
//!
//! ★★★ **Today it is a route, which is a second write path into `S_arena`** —
//! the exact failure complete mediation forbids. A guard that some callers go
//! through is not a guard; it is a suggestion with good adoption. So there is no
//! public constructor for [`Published`], and the only way to obtain one is
//! [`publish`], which evaluates all five conjuncts. A host that wanted to skip
//! the check has nothing to hand the catalogue.
//!
//! ## Immutability is the precondition for everything downstream
//!
//! ★★★ **If `(name, version)` can be rebound, per-version trust is not merely
//! weakened — it is *undefined*.** A content address stops being stable, a
//! reputation attaches to a moving target, and an author can earn standing on
//! one artefact and ship another under the same identifier. Everything
//! [`crate::reputation`] does rests on this one refusal.
//!
//! ## Three questions that are not the same question
//!
//! ★★★ **Integrity, authenticity and quality**, and conflating them is how a
//! registry becomes a supply chain. The bytes you got are the bytes published
//! (a hash); those bytes came from that author (a signature); those bytes do
//! something worth having (**neither**). A perfectly signed, perfectly hashed
//! artefact can be worthless or hostile.
//!
//! ★★★ **Type-checking is well-formedness, not merit**, and the vocabulary here
//! refuses to blur them: nothing returns `Valid` or `Good`. The compiler can say
//! a spec parses and is internally consistent; it cannot say the artefact is
//! useful, and a green check on the first must not do duty for the second.
//!
//! ## τ₀ is not zero
//!
//! ★★ A new version by an author with a long clean record should not enter at
//! the same standing as an anonymous first upload — and must not enter at the
//! author's own score either, because that *is* the malicious-update vector.
//! [`crate::reputation::Prior::from_author`] is the estimator that makes
//! "inherited but discounted" precise, and it is used here rather than restated.

use std::collections::BTreeSet;

use crate::package::{Authenticity, Integrity};
use crate::reputation::{Prior, Versioned};

/// What a publisher is offering.
#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    pub artefact: Versioned,
    pub author: String,
    /// `h = H(bytes)` — the content address.
    pub content_hash: String,
    pub integrity: Integrity,
    pub authenticity: Authenticity,
    /// Did the Embroidery AST typecheck? **Well-formed, not good.**
    pub well_formed: bool,
    /// `ν` — §VII's niche. `None` is a refusal, not a default.
    pub niche: Option<String>,
}

impl Submission {
    pub fn new(artefact: Versioned, author: &str, content_hash: &str) -> Self {
        Self {
            artefact,
            author: author.into(),
            content_hash: content_hash.into(),
            integrity: Integrity::Intact,
            authenticity: Authenticity::Unsigned,
            well_formed: true,
            niche: None,
        }
    }

    pub fn signed(mut self) -> Self {
        self.authenticity = Authenticity::Signed;
        self
    }

    pub fn in_niche(mut self, niche: &str) -> Self {
        self.niche = Some(niche.into());
        self
    }

    pub fn malformed(mut self) -> Self {
        self.well_formed = false;
        self
    }
}

/// Which conjunct of `g_publish` failed.
///
/// ★★ One variant per conjunct, and never a single `Rejected`. "Your submission
/// was refused" sends somebody guessing; "this name and version are already
/// bound" is a thing they can fix in a minute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// `auth(author)` — `B_arena`.
    NotAuthenticated { author: String },
    /// ★★★ `¬bound(name, v)` — **a version is written once, ever.**
    AlreadyBound { artefact: Versioned },
    /// `Verify(σ)` — the signature does not verify.
    Forged { author: String },
    /// The bytes are not the bytes that were hashed.
    Altered { claimed: String, actual: String },
    /// `typecheck(spec)` — the spec does not parse or is not internally
    /// consistent. **Well-formedness, not merit.**
    NotWellFormed,
    /// `declares(ν)` — §VII: an unconditioned ranking is meaningless, so an
    /// artefact that names no niche cannot be ranked and must not be listed.
    NoNiche,
}

impl Refusal {
    pub fn describe(&self) -> String {
        match self {
            Self::NotAuthenticated { author } => {
                format!("'{author}' is not a member of this Arena")
            }
            Self::AlreadyBound { artefact } => format!(
                "{} {} is already published — a version is written once, ever, because a trust \
                 score attached to a rebindable name is attached to a moving target",
                artefact.name, artefact.version
            ),
            Self::Forged { author } => {
                format!("the signature does not verify against '{author}' key")
            }
            Self::Altered { claimed, actual } => {
                format!("the bytes hash to {actual}, and the submission claims {claimed}")
            }
            Self::NotWellFormed => {
                "the spec does not typecheck — this is well-formedness, and says nothing about \
                 whether the artefact is any good"
                    .into()
            }
            Self::NoNiche => {
                "no niche declared — an unconditioned ranking is meaningless, so an artefact \
                 that names no niche cannot be listed"
                    .into()
            }
        }
    }
}

/// A catalogue entry that got through the guard.
///
/// ★★★ **No public constructor.** The only way to hold one is [`publish`],
/// which is what makes publishing an Enzyme rather than a route: there is no
/// second write path into the catalogue, because there is nothing else to write.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    artefact: Versioned,
    author: String,
    content_hash: String,
    niche: String,
    prior: Prior,
}

impl Published {
    pub fn artefact(&self) -> &Versioned {
        &self.artefact
    }
    pub fn author(&self) -> &str {
        &self.author
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
    pub fn niche(&self) -> &str {
        &self.niche
    }
    /// `τ₀` — inherited from the author, discounted, and never their own score.
    pub fn prior(&self) -> Prior {
        self.prior
    }
}

/// What the Arena has already bound, and who may publish into it.
#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    bound: BTreeSet<Versioned>,
    members: BTreeSet<String>,
}

impl Catalogue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn admitting(mut self, author: &str) -> Self {
        self.members.insert(author.to_string());
        self
    }

    pub fn is_bound(&self, artefact: &Versioned) -> bool {
        self.bound.contains(artefact)
    }

    /// Every version of a name that has been published.
    ///
    /// ★★ A name is not an entry; it is a family of them. Asking "what is
    /// published under this name" and getting one answer is what a rebindable
    /// identifier lets you believe.
    pub fn versions_of(&self, name: &str) -> Vec<&Versioned> {
        self.bound.iter().filter(|v| v.name == name).collect()
    }

    fn record(&mut self, artefact: &Versioned) {
        self.bound.insert(artefact.clone());
    }
}

/// **`g_publish`, all five conjuncts.**
///
/// ★★ Every failure is collected rather than the first, because an author who
/// fixes a signature and is then told about a missing niche has been made to
/// resubmit twice.
pub fn check(
    submission: &Submission,
    catalogue: &Catalogue,
) -> Vec<Refusal> {
    let mut out = Vec::new();
    if !catalogue.members.contains(&submission.author) {
        out.push(Refusal::NotAuthenticated { author: submission.author.clone() });
    }
    if catalogue.is_bound(&submission.artefact) {
        out.push(Refusal::AlreadyBound { artefact: submission.artefact.clone() });
    }
    if let Integrity::Altered { claimed, actual } = &submission.integrity {
        out.push(Refusal::Altered { claimed: claimed.clone(), actual: actual.clone() });
    }
    if submission.authenticity == Authenticity::Forged {
        out.push(Refusal::Forged { author: submission.author.clone() });
    }
    if !submission.well_formed {
        out.push(Refusal::NotWellFormed);
    }
    if submission.niche.is_none() {
        out.push(Refusal::NoNiche);
    }
    out
}

/// **Publish, or say why not.**
///
/// ★★★ The one door. `author_standing` is the author's own `(τ, weight)`, and
/// `discount` is how much of it a new version inherits — passed in rather than
/// assumed, because "how much does a good record buy you" is a market's policy
/// and not this function's opinion.
pub fn publish(
    submission: Submission,
    catalogue: &mut Catalogue,
    author_standing: (f64, f64),
    discount: f64,
) -> Result<Published, Vec<Refusal>> {
    let refusals = check(&submission, catalogue);
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let (tau, weight) = author_standing;
    catalogue.record(&submission.artefact);
    Ok(Published {
        artefact: submission.artefact,
        author: submission.author,
        content_hash: submission.content_hash,
        niche: submission.niche.expect("checked"),
        prior: Prior::from_author(tau, weight, discount),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena() -> Catalogue {
        Catalogue::new().admitting("bonnie")
    }

    fn good(version: &str) -> Submission {
        Submission::new(Versioned::new("a budget widget", version), "bonnie", "h-abc")
            .signed()
            .in_niche("household finance")
    }

    fn unknown_author() -> (f64, f64) {
        (0.5, 0.0)
    }

    #[test]
    fn there_is_no_way_into_the_catalogue_except_through_the_guard() {
        // ★★★ `Published` has no public constructor. A route would be a second
        //     write path into S_arena, which is the exact failure complete
        //     mediation forbids — a guard some callers go through is not a
        //     guard, it is a suggestion with good adoption.
        let mut c = arena();
        let p = publish(good("1.0"), &mut c, unknown_author(), 0.1).expect("admitted");
        assert_eq!(p.artefact().version, "1.0");
        assert!(c.is_bound(&Versioned::new("a budget widget", "1.0")));
    }

    #[test]
    fn a_version_is_written_once_ever() {
        // ★★★ The precondition for everything downstream. Rebind it and a
        //     content address stops being stable, and per-version trust is not
        //     weakened but undefined.
        let mut c = arena();
        publish(good("1.0"), &mut c, unknown_author(), 0.1).expect("first");
        let again = publish(good("1.0"), &mut c, unknown_author(), 0.1).expect_err("refused");
        assert!(matches!(again[0], Refusal::AlreadyBound { .. }));
        assert!(again[0].describe().contains("attached to a moving target"));
    }

    #[test]
    fn a_new_version_of_the_same_name_is_fine_and_is_a_different_entry() {
        // ★★ A name is a family of entries, not one. Asking "what is published
        //    under this name" and getting a single answer is exactly what a
        //    rebindable identifier lets you believe.
        let mut c = arena();
        publish(good("1.0"), &mut c, unknown_author(), 0.1).expect("first");
        publish(good("1.1"), &mut c, unknown_author(), 0.1).expect("second");
        assert_eq!(c.versions_of("a budget widget").len(), 2);
    }

    #[test]
    fn a_perfectly_signed_artefact_can_still_be_refused_for_something_else() {
        // ★★★ Integrity, authenticity and quality are three questions.
        //     Cryptography answers two and is silent on the third — and the
        //     guard is silent on it too, which is why nothing here says "valid".
        let mut c = arena();
        let no_niche = Submission::new(Versioned::new("w", "1.0"), "bonnie", "h").signed();
        let refused = publish(no_niche, &mut c, unknown_author(), 0.1).expect_err("refused");
        assert_eq!(refused, vec![Refusal::NoNiche]);
    }

    #[test]
    fn a_forged_signature_is_refused_and_an_unsigned_one_is_not() {
        // ★★ No signature at all is not the same as a bad one — `package.rs`
        //    own distinction, reused rather than re-decided here.
        let mut c = arena();
        let mut forged = good("1.0");
        forged.authenticity = Authenticity::Forged;
        assert!(publish(forged, &mut c, unknown_author(), 0.1).is_err());

        let mut c2 = arena();
        let mut unsigned = good("1.0");
        unsigned.authenticity = Authenticity::Unsigned;
        assert!(publish(unsigned, &mut c2, unknown_author(), 0.1).is_ok());
    }

    #[test]
    fn altered_bytes_are_refused_with_both_hashes() {
        // ★★ A mismatch a person cannot inspect is an alarm rather than a
        //    diagnosis.
        let mut c = arena();
        let mut tampered = good("1.0");
        tampered.integrity =
            Integrity::Altered { claimed: "h-abc".into(), actual: "h-xyz".into() };
        let refused = publish(tampered, &mut c, unknown_author(), 0.1).expect_err("refused");
        let said = refused[0].describe();
        assert!(said.contains("h-abc") && said.contains("h-xyz"));
    }

    #[test]
    fn typechecking_is_wellformedness_and_the_refusal_says_so() {
        // ★★★ "Do not let a green check mark on the first do duty for the
        //     second." Nothing in this module returns `Valid` or `Good`.
        let mut c = arena();
        let refused = publish(good("1.0").malformed(), &mut c, unknown_author(), 0.1)
            .expect_err("refused");
        assert!(refused[0].describe().contains("says nothing about"));
    }

    #[test]
    fn an_artefact_with_no_niche_cannot_be_listed() {
        // ★★★ §VII: an unconditioned ranking is meaningless, so something that
        //     names no niche has nothing to be ranked within.
        let mut c = arena();
        let bare = Submission::new(Versioned::new("w", "1.0"), "bonnie", "h").signed();
        assert!(publish(bare, &mut c, unknown_author(), 0.1).is_err());
    }

    #[test]
    fn a_stranger_cannot_publish() {
        let mut c = Catalogue::new();
        let refused = publish(good("1.0"), &mut c, unknown_author(), 0.1).expect_err("refused");
        assert!(refused.iter().any(|r| matches!(r, Refusal::NotAuthenticated { .. })));
    }

    #[test]
    fn every_failed_conjunct_is_reported_rather_than_the_first() {
        // ★★ An author who fixes a signature and is then told about a missing
        //    niche has been made to resubmit twice.
        let mut c = Catalogue::new();
        let mut awful = Submission::new(Versioned::new("w", "1.0"), "nobody", "h").malformed();
        awful.authenticity = Authenticity::Forged;
        let refused = publish(awful, &mut c, unknown_author(), 0.1).expect_err("refused");
        assert!(refused.len() >= 4, "{refused:?}");
    }

    #[test]
    fn tau_zero_is_neither_zero_nor_the_authors_own_score() {
        // ★★★ A long clean record should not enter at an anonymous upload's
        //     standing, and must not enter at the author's own — the latter IS
        //     the malicious-update vector.
        let mut c = arena();
        let by_a_stranger =
            publish(good("1.0"), &mut c, (0.5, 0.0), 0.1).expect("admitted").prior();
        let mut c2 = arena();
        let by_a_veteran =
            publish(good("1.0"), &mut c2, (0.95, 200.0), 0.1).expect("admitted").prior();

        let none = crate::reputation::Evidence::none();
        let stranger_tau = crate::reputation::tau(&none, &by_a_stranger);
        let veteran_tau = crate::reputation::tau(&none, &by_a_veteran);
        assert!(veteran_tau > stranger_tau, "a record counts for something");
        assert!(veteran_tau < 0.95, "and never for everything");
    }

    #[test]
    fn a_refused_submission_binds_nothing() {
        // ★★★ The catalogue must be untouched by a refusal, or a failed publish
        //     would burn the version number it failed on.
        let mut c = arena();
        let _ = publish(good("1.0").malformed(), &mut c, unknown_author(), 0.1);
        assert!(!c.is_bound(&Versioned::new("a budget widget", "1.0")));
        assert!(publish(good("1.0"), &mut c, unknown_author(), 0.1).is_ok());
    }
}
