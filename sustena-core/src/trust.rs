//! **Trust, out of signals you actually hold** — and *unrated* when you
//! hold none.
//!
//! The reference has `trust_score` as a `Float` written `0.0` at publish,
//! updated by nothing, and used as the catalogue's sort key. A number nobody
//! computed, ordering a list. This module is the answer to *what could that
//! number honestly be made of*, and the answer is short, because **an honest
//! trust reading can only be built from what this node can verify**.
//!
//! ★★★ **What is real, and it is a small list:** whether a package is
//! **signed** (authenticity, checked against the author's key), whether its
//! author is **you** or a **peer you chose to trust** (the web of trust you
//! actually hold — slice 3's keys and slice 6's peer book, not a reputation
//! service), how many of your own peers **hold** it (a count you can see,
//! because you asked each of them), and its **provenance** (authored here,
//! bundled, or imported from a named peer).
//!
//! ★★★ **What would be invented, and is therefore absent:** a global rating, a
//! star count, downloads across a network this node cannot see, an aggregate
//! reputation. None of those exist here, so none of them appear.
//!
//! ★★★ **`Unrated` is a first-class answer, not a zero.** A package that is
//! unsigned, by an author you have never met, held by nobody you know is
//! **unrated** — and a fabricated `0.0` would read as a judgement somebody
//! made. Nobody made it. Same discipline as ρ's excluded-and-named,
//! `Skew::Unknown`, and the curated feed's withdrawal-with-a-reason.
//!
//! ★★★ **Trust informs; the gate decides.** Nothing here is consulted by
//! `package::definition_installs` or its siblings, and nothing here can widen
//! a path. A well-corroborated package still faces `editing::typecheck`; an
//! unrated one still installs if a person chooses it. **Integrity ≠
//! authenticity ≠ quality** held across the wire in the last increment, and it
//! holds against reputation here: being liked is not being correct.

use serde::{Deserialize, Serialize};

/// One thing this node can actually verify about a package.
///
/// ★★ Every variant is a fact with a source. There is deliberately no
/// `Rating(f64)` or `Stars(u8)` — a variant like that would be a place for an
/// invented number to hide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustSignal {
    /// The signature verifies against the key the package names.
    Signed,
    /// There is no signature. ★ Not a negative — a package written here and
    /// never sent anywhere has nothing to prove.
    Unsigned,
    /// The signature does **not** verify. The one signal that is disqualifying.
    Forged,
    /// This node's own key wrote it.
    AuthoredHere,
    /// The author is a peer this node has explicitly trusted.
    AuthorIsATrustedPeer { handle: String },
    /// The author is a key this node has never decided anything about.
    AuthorUnknown,
    /// `n` of the peers you asked are holding these exact bytes.
    ///
    /// ★★★ **Held, not installed** — and the distinction is not pedantry. A
    /// peer's catalogue says what it offers; it does not say what it chose to
    /// run. Reporting "installed by" from an offer listing would be inventing
    /// the more flattering fact.
    HeldByPeers { count: usize, of: usize },
    /// This node has installed it. ★ The strongest first-hand signal there is:
    /// it passed your own gate.
    InstalledHere,
    /// It arrived from a named peer rather than being written here.
    ImportedFrom { peer: String },
}

impl TrustSignal {
    /// A short line for a surface. ★ Each says what it is, never a score.
    pub fn describe(&self) -> String {
        match self {
            TrustSignal::Signed => "signed by its author".into(),
            TrustSignal::Unsigned => "unsigned".into(),
            TrustSignal::Forged => "SIGNATURE DOES NOT VERIFY".into(),
            TrustSignal::AuthoredHere => "you wrote it".into(),
            TrustSignal::AuthorIsATrustedPeer { handle } => format!("by {handle}, a peer you trust"),
            TrustSignal::AuthorUnknown => "by a key you have not met".into(),
            TrustSignal::HeldByPeers { count, of } => {
                format!("held by {count} of the {of} peers you asked")
            }
            TrustSignal::InstalledHere => "you have installed it".into(),
            TrustSignal::ImportedFrom { peer } => format!("came from {peer}"),
        }
    }
}

/// How much this node can say about a package.
///
/// ★★ A band rather than a number, because a number invites the question *out
/// of what?* and the honest answer would have to be *out of nothing anyone
/// agreed on*. The band names how much **corroboration** exists, and the
/// signals beside it say exactly which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustStanding {
    /// ★★★ **Nothing to go on.** Unsigned, by a stranger, held by nobody you
    /// know. Not low trust — *no* trust reading, because there is nothing to
    /// read. Never rendered as a number.
    Unrated,
    /// A signature that does not verify. ★ The one band that is a warning
    /// rather than a measure, and it outranks everything else in the reading.
    Repudiated,
    /// Something is known — it is signed, or the author is someone — but
    /// nobody you know has taken it up.
    Thin,
    /// Signed by someone you know, or held by peers, or both.
    Corroborated,
    /// You wrote it, or you have already installed it. First-hand.
    FirstHand,
}

impl TrustStanding {
    pub fn label(&self) -> &'static str {
        match self {
            TrustStanding::Unrated => "unrated",
            TrustStanding::Repudiated => "repudiated",
            TrustStanding::Thin => "thin",
            TrustStanding::Corroborated => "corroborated",
            TrustStanding::FirstHand => "first-hand",
        }
    }

    /// ★★★ Whether a surface may show this as a *reading* at all. `Unrated`
    /// must not be rendered on the same scale as the others — that is the
    /// whole point of it being a separate answer.
    pub fn is_a_reading(&self) -> bool {
        !matches!(self, TrustStanding::Unrated)
    }
}

/// What this node can say about one package, and why.
///
/// ★ Named `PackageTrust`, `TrustSignal`, `TrustStanding` — name-collision
/// rules **#48**, **#49** and **#50**, the newcomer taking the longer name:
/// `event::Trust` is how much a SOURCE is believed, `signal::Signal` is
/// §III's relayed pulse, and the host's `peers::Standing` is what you decided
/// about a PEER. Three adjacent concepts that are emphatically not this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageTrust {
    pub standing: TrustStanding,
    /// ★★ The facts the standing was composed from, kept alongside it. A band
    /// without its reasons is the reference's float with a nicer name.
    pub signals: Vec<TrustSignal>,
}

impl PackageTrust {
    /// Compose a reading from the signals this node actually gathered.
    ///
    /// ★★★ **The rules are declared here and nowhere else**, so *what makes a
    /// package well-regarded* is one readable function rather than a
    /// heuristic smeared across a UI.
    pub fn from_signals(signals: Vec<TrustSignal>) -> PackageTrust {
        // A signature that does not verify ends the conversation.
        if signals.contains(&TrustSignal::Forged) {
            return PackageTrust { standing: TrustStanding::Repudiated, signals };
        }

        let first_hand = signals
            .iter()
            .any(|s| matches!(s, TrustSignal::AuthoredHere | TrustSignal::InstalledHere));
        if first_hand {
            return PackageTrust { standing: TrustStanding::FirstHand, signals };
        }

        let known_author = signals
            .iter()
            .any(|s| matches!(s, TrustSignal::AuthorIsATrustedPeer { .. }));
        let held = signals
            .iter()
            .any(|s| matches!(s, TrustSignal::HeldByPeers { count, .. } if *count > 0));
        if known_author || held {
            return PackageTrust { standing: TrustStanding::Corroborated, signals };
        }

        let signed = signals.contains(&TrustSignal::Signed);
        if signed {
            return PackageTrust { standing: TrustStanding::Thin, signals };
        }

        // ★★★ Unsigned, by a stranger, held by nobody. There is nothing to
        //     read, and saying so is the honest answer.
        PackageTrust { standing: TrustStanding::Unrated, signals }
    }

    /// One line, in the signals' own words.
    pub fn describe(&self) -> String {
        if self.signals.is_empty() {
            return "unrated — nothing is known about this package".to_string();
        }
        let reasons: Vec<String> = self.signals.iter().map(TrustSignal::describe).collect();
        format!("{} · {}", self.standing.label(), reasons.join(" · "))
    }

    /// ★★★ **Never a gate.** Provided so a caller cannot mistake this type for
    /// one: trust orders a list and informs a person; admission is
    /// `package::definition_installs` and its siblings, which do not consult
    /// this module and cannot be widened by it.
    pub fn decides_admission(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stranger_with_no_signature_and_no_takers_is_unrated() {
        // ★★★ Not zero. Nobody rated this low; nobody rated it at all.
        let t = PackageTrust::from_signals(vec![TrustSignal::Unsigned, TrustSignal::AuthorUnknown]);
        assert_eq!(t.standing, TrustStanding::Unrated);
        assert!(!t.standing.is_a_reading(), "it must not render on the same scale");
        assert!(t.describe().contains("unrated"));
    }

    #[test]
    fn nothing_at_all_is_also_unrated() {
        let t = PackageTrust::from_signals(vec![]);
        assert_eq!(t.standing, TrustStanding::Unrated);
        assert!(t.describe().contains("nothing is known"));
    }

    #[test]
    fn signed_by_a_trusted_peer_and_held_by_peers_reads_corroborated() {
        let t = PackageTrust::from_signals(vec![
            TrustSignal::Signed,
            TrustSignal::AuthorIsATrustedPeer { handle: "bob".into() },
            TrustSignal::HeldByPeers { count: 2, of: 3 },
        ]);
        assert_eq!(t.standing, TrustStanding::Corroborated);
        assert!(t.standing.is_a_reading());
        let shown = t.describe();
        assert!(shown.contains("bob"), "{shown}");
        assert!(shown.contains("2 of the 3"), "{shown}");
    }

    #[test]
    fn signed_by_a_stranger_nobody_holds_is_thin_not_unrated() {
        // ★★ Something IS known — the signature verifies — so it is not
        //    unrated. It is just thin, and the word says so.
        let t = PackageTrust::from_signals(vec![TrustSignal::Signed, TrustSignal::AuthorUnknown]);
        assert_eq!(t.standing, TrustStanding::Thin);
        assert!(t.standing.is_a_reading());
    }

    #[test]
    fn your_own_work_and_your_own_installs_are_first_hand() {
        assert_eq!(
            PackageTrust::from_signals(vec![TrustSignal::AuthoredHere, TrustSignal::Unsigned]).standing,
            TrustStanding::FirstHand,
            "an unsigned package you wrote yourself is not thin",
        );
        assert_eq!(
            PackageTrust::from_signals(vec![
                TrustSignal::Signed,
                TrustSignal::AuthorUnknown,
                TrustSignal::InstalledHere,
            ])
            .standing,
            TrustStanding::FirstHand,
            "it passed your own gate, which beats anyone's opinion",
        );
    }

    #[test]
    fn a_forged_signature_outranks_every_other_signal() {
        // ★★★ Held by everyone, installed here, written by a trusted peer —
        //     and the signature still does not verify. Repudiated.
        let t = PackageTrust::from_signals(vec![
            TrustSignal::Forged,
            TrustSignal::AuthorIsATrustedPeer { handle: "bob".into() },
            TrustSignal::HeldByPeers { count: 9, of: 9 },
            TrustSignal::InstalledHere,
        ]);
        assert_eq!(t.standing, TrustStanding::Repudiated);
        assert!(t.describe().contains("DOES NOT VERIFY"));
    }

    #[test]
    fn held_by_zero_peers_corroborates_nothing() {
        // ★ Asking three peers and finding none is a real answer, and it is not
        //   corroboration.
        let t = PackageTrust::from_signals(vec![
            TrustSignal::Unsigned,
            TrustSignal::AuthorUnknown,
            TrustSignal::HeldByPeers { count: 0, of: 3 },
        ]);
        assert_eq!(t.standing, TrustStanding::Unrated);
        assert!(t.describe().contains("0 of the 3"), "and it still says it asked");
    }

    #[test]
    fn trust_is_never_a_gate() {
        // ★★★ A structural assertion: this type answers no admission question,
        //     and the method exists so nobody can believe otherwise.
        for signals in [
            vec![TrustSignal::AuthoredHere],
            vec![TrustSignal::Forged],
            vec![],
        ] {
            assert!(!PackageTrust::from_signals(signals).decides_admission());
        }
    }

    #[test]
    fn unrated_sorts_apart_from_the_readings() {
        // ★★ `Unrated` is the lowest variant so a list can put it in its own
        //    bucket, never interleaved with real readings as if it were a low
        //    score.
        let mut bands = [
            TrustStanding::Corroborated,
            TrustStanding::Unrated,
            TrustStanding::FirstHand,
            TrustStanding::Thin,
        ];
        bands.sort();
        assert_eq!(bands[0], TrustStanding::Unrated);
        assert_eq!(bands[bands.len() - 1], TrustStanding::FirstHand);
    }
}
