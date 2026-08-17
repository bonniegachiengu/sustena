//! The operative and utility-as-a-vector, replayed against its conformance
//! vectors (R2 · Operative §I + §II · OPV-1, OPV-3).
//!
//! **SPEC vectors, Rust-only** — with Nash's degenerate case at parity, as §II
//! itself says: `nash_utility_score` is *"a correct implementation of a
//! principled thing over an impoverished input."*
//!
//! The reference's `OperativeVote.utility` is `float = 0.5` — a scalar
//! confidence attached to a vote, produced as a literal at the call sites, so
//! there is no `S` it is a function *of*. §II's whole argument cannot be
//! stated against a value that was never a vector.
//!
//! The proof of value: a genuinely non-dominated option, sitting in a
//! non-convex notch, kept by the frontier and selected by **none** of a
//! thousand weight vectors.
//!
//! See `conformance/README.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    dominance, geometric_mean, nash_product, pareto_frontier, scalarise, Alternative, Cynefin,
    Dominance, NashOutcome, Objective, Omega, Operative, OperativeError, Sense, Shared, StatePoint,
    Utility, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("operative.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "operative.json was written for a different contract version"
    );
    doc
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap_or_else(|| panic!("group {group} missing"))
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("case {group}/{name} missing"))
}

// ── fixture, built from the vector ──────────────────────────────────────────

fn world(doc: &Value) -> Shared {
    let mut w = Shared::new();
    for d in doc["fixture"]["dimensions"].as_array().unwrap() {
        w = w.with_dimension(d.as_str().unwrap());
    }
    for m in doc["fixture"]["moves"].as_array().unwrap() {
        w = w.with_move(m.as_str().unwrap());
    }
    for t in doc["fixture"]["transitions"].as_array().unwrap() {
        w = w
            .with_transition(
                t[0].as_str().unwrap(),
                t[1].as_str().unwrap(),
                t[2].as_str().unwrap(),
            )
            .expect("declared move");
    }
    w
}

fn sense_of(s: &str) -> Sense {
    match s {
        "maximise" => Sense::Maximise,
        "minimise" => Sense::Minimise,
        other => panic!("unknown sense {other}"),
    }
}

fn cynefin_of(s: &str) -> Cynefin {
    Cynefin::ALL
        .into_iter()
        .find(|c| c.name() == s)
        .unwrap_or_else(|| panic!("unknown domain {s}"))
}

fn operative(doc: &Value, id: &str) -> Operative {
    let spec = &doc["fixture"]["operatives"][id];
    let mut u = Utility::new();
    for o in spec["objectives"].as_array().unwrap() {
        u = u
            .with(Objective::new(
                o[0].as_str().unwrap(),
                o[1].as_str().unwrap(),
                sense_of(o[2].as_str().unwrap()),
            ))
            .expect("distinct objective");
    }
    let suited: Vec<Cynefin> = spec["suited"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| cynefin_of(s.as_str().unwrap()))
        .collect();
    Operative::new(id, u, &suited).expect("a real preference")
}

fn point(obj: &Value, keys: &[&str]) -> StatePoint {
    keys.iter()
        .filter_map(|k| obj.get(*k).and_then(|v| v.as_f64()).map(|v| (k.to_string(), v)))
        .collect()
}

fn vec_of(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

// ── the sharing constraint ──────────────────────────────────────────────────

/// ★ Proposition 1, as a shape: a move outside `T` is unconstructible.
#[test]
fn an_operative_can_propose_only_elements_of_t() {
    let doc = load();
    let c = case(
        &doc,
        "sharing_constraint_cases",
        "★_an_operative_can_propose_only_elements_of_T",
    );
    let w = world(&doc);
    let op = operative(&doc, "mentor");
    let e = &c["expect"];

    let legal = e["legal"].as_str().unwrap();
    assert_eq!(op.propose(&w, legal).unwrap().move_name(), legal);
    assert_eq!(op.propose(&w, legal).unwrap().by(), "mentor");

    assert!(matches!(
        op.propose(&w, e["illegal"].as_str().unwrap()),
        Err(OperativeError::MoveNotInT(_))
    ));
}

#[test]
fn an_objective_over_a_dimension_outside_s_is_refused() {
    let doc = load();
    let _ = case(
        &doc,
        "sharing_constraint_cases",
        "an_objective_over_a_dimension_outside_S_is_refused",
    );
    let stray = Operative::new(
        "stray",
        Utility::new()
            .with(Objective::new("glory", "glory", Sense::Maximise))
            .unwrap(),
        &[Cynefin::Chaotic],
    )
    .unwrap();
    assert!(matches!(
        Omega::over(world(&doc)).with(stray),
        Err(OperativeError::DimensionNotInS { .. })
    ));
}

#[test]
fn a_transition_on_an_undeclared_move_is_refused() {
    let doc = load();
    let _ = case(
        &doc,
        "sharing_constraint_cases",
        "a_transition_on_an_undeclared_move_is_refused",
    );
    assert!(matches!(
        Shared::new().with_transition("s0", "teleport", "s9"),
        Err(OperativeError::MoveNotInT(_))
    ));
}

// ── ★★ Proposition 2 ────────────────────────────────────────────────────────

/// ★★ Capability invariant, ranking variant — the pair the proposition is
/// actually about.
#[test]
fn adding_an_operative_leaves_reachable_unchanged_and_only_re_ranks() {
    let doc = load();
    let c = case(
        &doc,
        "proposition_cases",
        "★_adding_an_operative_leaves_R(s)_unchanged_and_only_re_ranks",
    );
    let alts: Vec<Alternative> = c["alternatives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| Alternative {
            id: a["id"].as_str().unwrap().to_string(),
            point: point(a, &["savings", "free_time", "noise"]),
        })
        .collect();

    let one = Omega::over(world(&doc)).with(operative(&doc, "mentor")).unwrap();
    let r_before = one.reachable("s0");
    let rank_before = one.rank(&alts).unwrap();

    let two = one.clone().with(operative(&doc, "curator")).unwrap();
    let r_after = two.reachable("s0");
    let rank_after = two.rank(&alts).unwrap();

    let e = &c["expect"];
    let want: BTreeSet<String> = e["reachable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(r_before, want);
    // ★ Capability: byte-identical. Ω is not an argument to `reachable`.
    assert_eq!(r_before == r_after, e["reachable_unchanged"].as_bool().unwrap());
    // ★ Ranking: genuinely different, with no move added.
    assert_eq!(rank_before != rank_after, e["ranking_changed"].as_bool().unwrap());

    let before: Vec<&str> = rank_before.dominated_for_all.iter().map(|s| s.as_str()).collect();
    let want_before: Vec<&str> = e["dominated_for_all_before"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(before, want_before);
    assert!(rank_after.dominated_for_all.is_empty());
    assert_eq!(two.size(), 2);
}

#[test]
fn the_observed_and_unwatched_sets_are_real() {
    let doc = load();
    let c = case(&doc, "proposition_cases", "the_observed_and_unwatched_sets_are_real");
    let e = &c["expect"];

    let one = Omega::over(world(&doc)).with(operative(&doc, "mentor")).unwrap();
    let as_set = |v: &Value| -> BTreeSet<String> {
        v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
    };
    let owned = |s: BTreeSet<&str>| -> BTreeSet<String> {
        s.into_iter().map(|x| x.to_string()).collect()
    };

    assert_eq!(owned(one.observed_dimensions()), as_set(&e["observed_one"]));
    assert_eq!(owned(one.unwatched_dimensions()), as_set(&e["unwatched_one"]));

    let both = one.with(operative(&doc, "curator")).unwrap();
    assert_eq!(owned(both.unwatched_dimensions()), as_set(&e["unwatched_both"]));
}

/// ★ OPV-16's recorded trap, honoured.
#[test]
fn suited_domains_are_the_cynefin_axis_not_a_subject_matter_tag() {
    let doc = load();
    let c = case(
        &doc,
        "proposition_cases",
        "suited_domains_are_the_cynefin_axis_not_a_subject_matter_tag",
    );
    let e = &c["expect"];
    let mentor = operative(&doc, "mentor");
    let curator = operative(&doc, "curator");
    assert_eq!(mentor.suited_to(Cynefin::Clear), e["mentor_clear"].as_bool().unwrap());
    assert_eq!(mentor.suited_to(Cynefin::Complex), e["mentor_complex"].as_bool().unwrap());
    assert_eq!(curator.suited_to(Cynefin::Complex), e["curator_complex"].as_bool().unwrap());
    assert_eq!(Cynefin::ALL.len(), e["axis_size"].as_u64().unwrap() as usize);
}

// ── ★★ the vector ───────────────────────────────────────────────────────────

/// ★★ THE PROOF OF VALUE — collapsing to a scalar does not hide a trade-off,
/// it deletes an option.
#[test]
fn a_non_convex_option_is_kept_by_the_frontier_and_selected_by_no_weight_vector() {
    let doc = load();
    let c = case(
        &doc,
        "vector_cases",
        "★_a_non_convex_option_is_kept_by_the_frontier_and_selected_by_no_weight_vector",
    );
    let pts = c["points"].as_object().unwrap();
    let points: Vec<(String, Vec<f64>)> = ["a", "b", "c"]
        .iter()
        .map(|k| ((*k).to_string(), vec_of(&pts[*k])))
        .collect();
    let e = &c["expect"];

    // Genuinely non-dominated — the case a scalar cannot express.
    assert_eq!(
        dominance(&points[0].1, &points[2].1).unwrap(),
        Dominance::Incomparable
    );
    assert_eq!(e["a_vs_c"].as_str().unwrap(), "Incomparable");
    assert_eq!(
        dominance(&points[1].1, &points[2].1).unwrap(),
        Dominance::Incomparable
    );

    let frontier = pareto_frontier(&points).unwrap();
    let want: Vec<String> = e["frontier"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(frontier, want, "the frontier keeps all three");

    // ...and no weight vector selects 'c'.
    let swept = c["weights_swept"].as_u64().unwrap() as usize;
    let steps = swept - 1;
    let mut c_ever_won = false;
    for i in 0..=steps {
        let l0 = i as f64 / steps as f64;
        let w = [l0, 1.0 - l0];
        let scores: Vec<f64> = points.iter().map(|(_, p)| scalarise(p, &w).unwrap()).collect();
        let best = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        if scores[2] >= best - 1e-12 {
            c_ever_won = true;
        }
    }
    assert_eq!(c_ever_won, e["c_ever_selected"].as_bool().unwrap());
    assert!(!c_ever_won, "{swept} weight vectors, and none of them picks 'c'");
}

/// ★ The companion — the defect is specific, and overstating it would be as
/// wrong as ignoring it.
#[test]
fn a_convex_option_is_selected_by_some_weight_vector() {
    let doc = load();
    let c = case(&doc, "vector_cases", "a_convex_option_is_selected_by_some_weight_vector");
    let pts = c["points"].as_object().unwrap();
    let points: Vec<Vec<f64>> = ["a", "b", "c"].iter().map(|k| vec_of(&pts[*k])).collect();
    let w = [0.5, 0.5];
    let scores: Vec<f64> = points.iter().map(|p| scalarise(p, &w).unwrap()).collect();
    let wins = scores[2] > scores[0] && scores[2] > scores[1];
    assert_eq!(wins, c["expect"]["c_wins_at_equal_weights"].as_bool().unwrap());
}

#[test]
fn dominance_is_four_valued_and_incomparable_is_a_real_answer() {
    let doc = load();
    let c = case(
        &doc,
        "vector_cases",
        "dominance_is_four_valued_and_incomparable_is_a_real_answer",
    );
    let e = &c["expect"];
    for (key, want) in [
        ("dominates", Dominance::Dominates),
        ("dominated_by", Dominance::DominatedBy),
        ("equal", Dominance::Equal),
        ("incomparable", Dominance::Incomparable),
    ] {
        let pair = e[key].as_array().unwrap();
        assert_eq!(
            dominance(&vec_of(&pair[0]), &vec_of(&pair[1])).unwrap(),
            want,
            "{key}"
        );
    }
}

#[test]
fn comparing_vectors_of_different_length_is_refused() {
    let doc = load();
    let _ = case(&doc, "vector_cases", "comparing_vectors_of_different_length_is_refused");
    assert!(matches!(
        dominance(&[1.0], &[1.0, 2.0]),
        Err(OperativeError::DimensionMismatch { .. })
    ));
}

#[test]
fn delta_is_over_a_pair_and_is_still_a_vector() {
    let doc = load();
    let c = case(&doc, "vector_cases", "delta_is_over_a_pair_and_is_still_a_vector");
    let u = operative(&doc, "curator").utility().clone();
    let before = point(&c["before"], &["savings", "free_time"]);
    let after = point(&c["after"], &["savings", "free_time"]);
    assert_eq!(u.delta(&before, &after).unwrap(), vec_of(&c["expect"]["delta"]));
    // Still a vector: m components out, one per objective.
    assert_eq!(u.delta(&before, &after).unwrap().len(), u.m());
}

#[test]
fn a_minimised_objective_reads_the_other_way() {
    let doc = load();
    let c = case(&doc, "vector_cases", "a_minimised_objective_reads_the_other_way");
    let u = Utility::new()
        .with(Objective::new("quiet", "noise", Sense::Minimise))
        .unwrap();
    let loud: StatePoint = [("noise".to_string(), 9.0)].into_iter().collect();
    let quiet: StatePoint = [("noise".to_string(), 1.0)].into_iter().collect();
    let d = dominance(&u.at(&quiet).unwrap(), &u.at(&loud).unwrap()).unwrap();
    assert_eq!(
        d == Dominance::Dominates,
        c["expect"]["quiet_dominates_loud"].as_bool().unwrap()
    );
}

#[test]
fn a_dimension_the_point_does_not_carry_is_refused_not_read_as_zero() {
    let doc = load();
    let _ = case(
        &doc,
        "vector_cases",
        "a_dimension_the_point_does_not_carry_is_refused_not_read_as_zero",
    );
    let u = operative(&doc, "mentor").utility().clone();
    let empty: StatePoint = BTreeMap::new();
    assert!(matches!(
        u.at(&empty),
        Err(OperativeError::DimensionMissing { .. })
    ));
}

// ── Nash ────────────────────────────────────────────────────────────────────

#[test]
fn nash_subtracts_the_disagreement_point() {
    let doc = load();
    let c = case(&doc, "nash_cases", "nash_subtracts_the_disagreement_point");
    let out = nash_product(&vec_of(&c["utilities"]), &vec_of(&c["disagreement"])).unwrap();
    match out {
        NashOutcome::Score(s) => {
            assert!((s - c["expect"]["score"].as_f64().unwrap()).abs() < 1e-12, "got {s}")
        }
        other => panic!("expected a score, got {other:?}"),
    }
}

/// ★ The one place the reference's arithmetic and Nash's part company.
#[test]
fn a_party_that_gains_nothing_zeroes_nash_but_not_a_geometric_mean() {
    let doc = load();
    let c = case(
        &doc,
        "nash_cases",
        "★_a_party_that_gains_nothing_zeroes_nash_but_not_a_geometric_mean",
    );
    let u = [1.0, 0.5];
    let d = [0.5, 0.5];
    let e = &c["expect"];

    match nash_product(&u, &d).unwrap() {
        NashOutcome::NoBargain { refused_by } => {
            assert_eq!(e["nash"].as_str().unwrap(), "NoBargain");
            let want: Vec<usize> = e["refused_by"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect();
            assert_eq!(refused_by, want);
        }
        other => panic!("expected NoBargain, got {other:?}"),
    }

    // The reference computes exp(mean(ln u)) over the RAW utilities.
    let gm = geometric_mean(&u).unwrap();
    assert!(
        gm > e["geometric_mean_above"].as_f64().unwrap(),
        "the geometric mean reads this as a good option: {gm}"
    );
}

#[test]
fn the_degenerate_one_dimensional_case_is_what_the_reference_computes() {
    let doc = load();
    let c = case(
        &doc,
        "nash_cases",
        "the_degenerate_one_dimensional_case_is_what_the_reference_computes",
    );
    let u = vec_of(&c["utilities"]);
    let gm = geometric_mean(&u).unwrap();
    let expected = u.iter().product::<f64>().powf(1.0 / u.len() as f64);
    assert!((gm - expected).abs() < 1e-12);
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ The keystone term is specific about what the reference's scalar IS.
    let u = d["term_by_term"]["★ u_i : S → ℝ^m — utility as a VECTOR"].as_str().unwrap();
    assert!(u.contains("utility: float = 0.5"));
    assert!(u.contains("never a vector"));

    // ★ Nash's degenerate case is credited, not diminished.
    let nash = d["term_by_term"]["★ Nash bargaining — the degenerate case"].as_str().unwrap();
    assert!(nash.contains("AT PARITY"));
    assert!(nash.contains("genuinely tested"));

    // ★ Both Nash nuances are recorded with their evidence.
    let nu = &d["the_two_precise_nuances_about_nash_recorded_rather_than_glossed"];
    let sub = nu["the disagreement point is NAMED in three places and subtracted in none"]
        .as_str()
        .unwrap();
    assert!(sub.contains("never subtracted"));
    assert!(sub.contains("not as a claim about intent"));
    let path = nu["it is defined and tested but not on a production decision path"]
        .as_str()
        .unwrap();
    assert!(path.contains("aggregate_delegated_votes"));
    assert!(path.contains("should not be read as 'in the loop'"));

    // ★ OPV-16's trap is recorded as an ACTIVE collision, with the evidence.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["supp (87 hits, 0 real)"].as_str().unwrap().contains("Not one is `supp(u_i)`"));
    let dom = fp["domain — ★ NOT a false positive, an ACTIVE collision"].as_str().unwrap();
    assert!(dom.contains("homestead.json"));
    assert!(dom.contains("Cynefin axis does not get that key"));

    // The counterweight is fair about what the reference does have.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("not a wrong answer"));
    assert!(cw.contains("never asked it"));

    // The Nash-needs-a-scalar tension is stated, not resolved by fiat.
    let t = d["the_tension_found_while_grounding"].as_str().unwrap();
    assert!(t.contains("ONE SCALAR PER PARTY"));
    assert!(t.contains("frontier first"));

    // Three honest limits, and the unstubbed slots named.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in ["READS declared dimensions", "DENSE SWEEP", "DECLARED TRANSITION RELATION"] {
        assert!(limits.contains(term), "missing limit: {term}");
    }
    let slots = d["declared_slots_deliberately_not_stubbed"].as_str().unwrap();
    assert!(slots.contains("NO placeholder field"));

    let groups = [
        "sharing_constraint_cases",
        "proposition_cases",
        "vector_cases",
        "nash_cases",
    ];
    let mut seen = BTreeSet::new();
    for group in groups {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(
                seen.insert(c["name"].as_str().unwrap().to_string()),
                "duplicate case name"
            );
        }
    }
}
