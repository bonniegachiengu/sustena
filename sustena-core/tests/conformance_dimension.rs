//! Declared dimension kinds, replayed against its conformance vectors
//! (R2 · Events and Time §VII + spec §4J.4 step 3 · EVT-10).
//!
//! **SPEC vectors, Rust-only — and the counterweight is the strongest in the
//! whole set.** §VII's own field note says the LWW-for-snapshot *behaviour*
//! shipped in the reference first, derived empirically after out-of-order SMS
//! clobbered a balance: *"the convergence theorem is not being proposed to VOS;
//! it is being written down about VOS."* This row does not bring a missing
//! behaviour — it turns a hard-won empirical rule into a checked one.
//!
//! ★★ The type-error half is proven by a `compile_fail` doctest on
//! `SnapshotDim`, which `cargo test --doc` executes; the declared-in-data half
//! by `DimensionSchema::check`. Both are needed.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    converges, laws_hold, AccumulatingDim, ContestedDim, Dimension, DimensionError,
    DimensionSchema, DimensionValue, JoinSemilattice, SnapshotDim, Tag, Update, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("dimension.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "dimension.json was written for a different contract version"
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

// ── the fixture: the household's own two quantities, one of each kind ───────

fn snapshot_path(doc: &Value) -> &str {
    doc["fixture"]["snapshot"]["path"].as_str().unwrap()
}

/// The declared readings: `[value, t_event, id]`, the third deliberately stale.
fn readings(doc: &Value) -> Vec<(i64, i64, String)> {
    doc["fixture"]["snapshot"]["readings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let a = r.as_array().unwrap();
            (a[0].as_i64().unwrap(), a[1].as_i64().unwrap(), a[2].as_str().unwrap().to_string())
        })
        .collect()
}

fn dim(v: i64, t: i64, id: &str) -> SnapshotDim {
    SnapshotDim::observed(json!(v), t, id)
}

// ── ★★ §VII: LWW converges under permutation AND repetition ────────────────

#[test]
fn lww_converges_under_any_permutation_and_any_repetition() {
    // ★★ Two phones, offline, seeing the same SMS backlog in different orders,
    // landing on the same balance. `converges` folds EVERY permutation, then
    // every permutation again with each update delivered twice.
    let doc = load();
    let c = case(
        &doc,
        "convergence_cases",
        "★★_LWW_converges_under_ANY_permutation_and_ANY_repetition",
    );
    let updates = vec![dim(10, 100, "a"), dim(30, 300, "b"), dim(20, 200, "c")];
    let report = converges(&updates).unwrap();

    assert_eq!(report.order_independent, c["expect"]["order_independent"].as_bool().unwrap());
    assert_eq!(report.duplication_free, c["expect"]["duplication_free"].as_bool().unwrap());
    assert_eq!(
        report.strongly_eventually_consistent(),
        c["expect"]["strongly_eventually_consistent"].as_bool().unwrap()
    );

    let folded = updates.iter().fold(updates[0].clone(), |a, b| a.join(b));
    assert_eq!(folded.value(), &json!(c["expect"]["value"].as_i64().unwrap()));
}

#[test]
fn the_snapshot_join_obeys_the_three_laws() {
    let doc = load();
    let c = case(&doc, "convergence_cases", "the_snapshot_join_obeys_the_three_laws");
    let r = laws_hold(&dim(10, 100, "a"), &dim(20, 200, "b"), &dim(30, 150, "c"));

    assert_eq!(r.commutative, c["expect"]["commutative"].as_bool().unwrap());
    assert_eq!(r.associative, c["expect"]["associative"].as_bool().unwrap());
    assert_eq!(r.idempotent, c["expect"]["idempotent"].as_bool().unwrap());
}

#[test]
fn a_stale_reading_arriving_late_does_not_clobber_the_newer_one() {
    // ★ The production failure the reference hit and fixed empirically.
    let doc = load();
    let c = case(
        &doc,
        "convergence_cases",
        "★_a_STALE_reading_arriving_late_does_not_clobber_the_newer_one",
    );
    let r = readings(&doc);
    let (v0, t0, id0) = &r[0];
    let mut d = dim(*v0, *t0, id0);
    for (v, t, id) in r.iter().skip(1) {
        d = d.observe(json!(v), *t, id.clone());
    }
    assert_eq!(d.value(), &json!(c["expect"]["after_stale"].as_i64().unwrap()));
}

#[test]
fn an_equal_event_time_breaks_on_the_id_so_the_order_stays_total() {
    // ★ Without the tie-break the merge stops being a join. Reused from
    // `Observation`, not re-derived here.
    let doc = load();
    let c = case(
        &doc,
        "convergence_cases",
        "★_an_EQUAL_event_time_breaks_on_the_id_so_the_order_stays_TOTAL",
    );
    let (left, right) = (dim(1, 100, "a"), dim(2, 100, "z"));
    assert_eq!(left.join(&right), right.join(&left));
    assert!(c["expect"]["commutes_at_a_tie"].as_bool().unwrap());
}

#[test]
fn snapshot_metadata_is_constant_however_many_readings_arrive() {
    let doc = load();
    let c = case(
        &doc,
        "convergence_cases",
        "★_snapshot_metadata_is_O(1)_however_many_readings_arrive",
    );
    let mut d = dim(0, 0, "seed");
    for i in 1..500 {
        d = d.observe(json!(i), i, format!("sms-{i}"));
    }
    assert_eq!(d.metadata_len() as u64, c["expect"]["metadata_len_after_500"].as_u64().unwrap());
}

// ── ★★ why "never accumulate deltas" is derived ────────────────────────────

#[test]
fn naive_addition_is_not_idempotent_which_is_the_whole_argument() {
    // ★★ §VII's counter-example, run rather than quoted. Written as plain
    // arithmetic because the shipped type offers no way to do it — which is
    // itself the point of the row.
    let doc = load();
    let c = case(
        &doc,
        "derived_not_decreed_cases",
        "★★_NAIVE_ADDITION_IS_NOT_IDEMPOTENT_which_is_the_whole_argument",
    );
    let (x, delta) = (100i64, 50i64);
    assert_ne!(x + delta + delta, x + delta);
    assert!(c["expect"]["x_plus_2delta_ne_x_plus_delta"].as_bool().unwrap());
}

#[test]
fn carrying_the_applied_ids_makes_it_idempotent_and_that_is_the_cost() {
    // ★★ The other half: made safe, an accumulation IS the grow-only counter —
    // at unbounded metadata, against LWW's constant 1.
    let doc = load();
    let c = case(
        &doc,
        "derived_not_decreed_cases",
        "★★_carrying_the_APPLIED_IDS_makes_it_idempotent_AND_THAT_IS_THE_COST",
    );
    let deltas = doc["fixture"]["accumulating"]["deltas"].as_array().unwrap();
    let a = deltas.iter().fold(AccumulatingDim::new(), |acc, d| {
        let p = d.as_array().unwrap();
        acc.add(p[0].as_str().unwrap(), p[1].as_i64().unwrap())
    });

    let first = deltas[0].as_array().unwrap();
    let redelivered = a.add(first[0].as_str().unwrap(), first[1].as_i64().unwrap());
    assert_eq!(redelivered.total(), c["expect"]["total_after_redelivery"].as_i64().unwrap());
    assert_eq!(a, redelivered, "the STATE is unchanged, not merely equal in total");
    assert!(c["expect"]["state_unchanged"].as_bool().unwrap());

    let big = (0..200).fold(AccumulatingDim::new(), |acc, i| acc.add(format!("tx-{i}"), 1));
    assert_eq!(
        big.metadata_len() as u64,
        c["expect"]["metadata_len_after_200"].as_u64().unwrap(),
        "unbounded — §VII's stated price, measured"
    );
}

#[test]
fn the_accumulating_join_also_obeys_the_three_laws() {
    let doc = load();
    let c = case(&doc, "derived_not_decreed_cases", "the_accumulating_join_also_obeys_the_three_laws");
    let (a, b, cc) = (
        AccumulatingDim::new().add("t1", 10),
        AccumulatingDim::new().add("t2", 20),
        AccumulatingDim::new().add("t3", 5),
    );
    let r = laws_hold(&a, &b, &cc);
    assert!(r.commutative && r.associative && r.idempotent, "{r:?}");
    assert_eq!(a.join(&b).join(&cc).total(), c["expect"]["joined_total"].as_i64().unwrap());
}

// ── ★★ the type error ──────────────────────────────────────────────────────

#[test]
fn the_delta_on_a_snapshot_type_error_is_proven_by_a_compile_fail_doctest() {
    // ★★ The strongest available form, and it is EXECUTED: `SnapshotDim`
    // carries a ```compile_fail``` doctest calling `.add(...)` on a balance,
    // and `cargo test --doc` runs it. This case records that the claim is
    // discharged there rather than restating it — a runtime test cannot assert
    // that something fails to compile.
    let doc = load();
    let c = case(
        &doc,
        "type_error_cases",
        "★★_a_DELTA_on_a_SNAPSHOT_dimension_DOES_NOT_COMPILE",
    );
    assert!(c["expect"]["doctest_is_compile_fail"].as_bool().unwrap());
    assert!(doc["★★_the_type_error_is_proven_by_a_compile_fail_DOCTEST"]
        .as_str()
        .unwrap()
        .contains("cargo test --doc"));
}

#[test]
fn a_snapshot_hands_out_no_handle_that_could_add() {
    // ★ The value-level mirror: there is no route from a declared snapshot to
    // a delta.
    let doc = load();
    let c = case(&doc, "type_error_cases", "★_a_snapshot_hands_out_NO_HANDLE_that_could_add");
    let v = DimensionValue::Snapshot(dim(1200, 100, "sms-1"));

    assert!(v.as_accumulating().is_none());
    assert!(c["expect"]["as_accumulating"].is_null());
    assert!(v.as_snapshot().is_some());
    assert_eq!(v.kind(), Dimension::Snapshot);
}

#[test]
fn a_delta_declared_against_a_snapshot_dimension_is_refused_from_data() {
    // ★★ The half a type system cannot reach — and the REASON travels with the
    // refusal, because a caller told only "no" learns nothing about the rule.
    let doc = load();
    let c = case(
        &doc,
        "type_error_cases",
        "★★_a_delta_declared_against_a_snapshot_dimension_is_REFUSED_from_DATA",
    );
    let mut s = DimensionSchema::new();
    s.declare(snapshot_path(&doc), Dimension::Snapshot).unwrap();

    let by = c["expect"]["by"].as_i64().unwrap();
    let err = s
        .check(snapshot_path(&doc), &Update::Delta { by, id: "tx-1".into() })
        .unwrap_err();

    assert!(matches!(err, DimensionError::DeltaOnSnapshot { .. }));
    assert!(err.to_string().contains(c["expect"]["reason_mentions"].as_str().unwrap()));
}

#[test]
fn an_observation_against_an_accumulating_dimension_is_refused_too() {
    // The rule is not "observations are always safe": replacing a running total
    // with a reading discards every contribution already counted.
    let doc = load();
    let mut s = DimensionSchema::new();
    let path = doc["fixture"]["accumulating"]["path"].as_str().unwrap();
    s.declare(path, Dimension::Accumulating).unwrap();

    assert!(matches!(
        s.check(path, &Update::Observe { value: json!(5), t_event: 1, id: "x".into() }),
        Err(DimensionError::ObservationOnAccumulating { .. })
    ));
}

#[test]
fn an_undeclared_dimension_is_an_error_not_a_default() {
    // Defaulting to `snapshot` in particular would silently make every
    // unconsidered dimension LWW — the remembered rule in a different hat.
    let s = DimensionSchema::new();
    assert!(matches!(
        s.check("nowhere", &Update::Delta { by: 1, id: "x".into() }),
        Err(DimensionError::Undeclared(_))
    ));
}

#[test]
fn a_dimension_cannot_be_redeclared_as_a_different_kind() {
    let doc = load();
    let mut s = DimensionSchema::new();
    let p = snapshot_path(&doc);
    s.declare(p, Dimension::Snapshot).unwrap();
    s.declare(p, Dimension::Snapshot).unwrap(); // same kind is a no-op
    assert!(matches!(
        s.declare(p, Dimension::Accumulating),
        Err(DimensionError::AlreadyDeclared { .. })
    ));
}

// ── ★ the attachment, and the scoping §VII is emphatic about ───────────────

#[test]
fn declaring_the_kind_is_enough_the_apply_attaches_itself() {
    // ★ The caller declares `snapshot` and never chooses LWW. The guard the
    // reference discovered empirically is here a consequence of one declaration.
    let doc = load();
    let c = case(
        &doc,
        "scoping_cases",
        "★_declaring_the_kind_is_ENOUGH_the_apply_ATTACHES_ITSELF",
    );
    let mut s = DimensionSchema::new();
    let p = snapshot_path(&doc);
    s.declare(p, Dimension::Snapshot).unwrap();

    let mut cur: Option<DimensionValue> = None;
    for (v, t, id) in readings(&doc) {
        cur = Some(
            s.apply(p, cur.as_ref(), Update::Observe { value: json!(v), t_event: t, id })
                .unwrap(),
        );
    }
    assert_eq!(
        cur.unwrap().as_snapshot().unwrap().value(),
        &json!(c["expect"]["after_stale"].as_i64().unwrap()),
        "the stale reading arrived last and did not clobber"
    );
}

#[test]
fn a_contested_dimension_keeps_both_edits_where_lww_would_keep_one() {
    // ★★ §VII's emphatic limit, side by side: the same two concurrent changes,
    // two kinds, two outcomes — and only one is right for a shared plan.
    let doc = load();
    let c = case(
        &doc,
        "scoping_cases",
        "★★_a_CONTESTED_dimension_keeps_BOTH_edits_where_LWW_would_keep_ONE",
    );
    let edits = doc["fixture"]["contested"]["edits"].as_array().unwrap();

    let mut contested = ContestedDim::new();
    for (i, e) in edits.iter().enumerate() {
        contested = contested
            .edit(e.as_str().unwrap(), Tag::new(&format!("node-{i}"), 1))
            .unwrap();
    }
    assert_eq!(contested.live().len() as u64, c["expect"]["contested_live"].as_u64().unwrap());

    // The same pair under LWW keeps exactly one.
    let as_snapshot = dim(4, 100, "phone").join(&dim(6, 100, "laptop"));
    assert_eq!(
        as_snapshot.metadata_len() as u64,
        c["expect"]["snapshot_metadata_len"].as_u64().unwrap()
    );
}

#[test]
fn a_contested_dimension_refuses_automatic_apply_entirely() {
    // ★ Any automatic apply is a machine deciding which human was right.
    let doc = load();
    let mut s = DimensionSchema::new();
    let p = doc["fixture"]["contested"]["path"].as_str().unwrap();
    s.declare(p, Dimension::Contested).unwrap();

    for u in [
        Update::Observe { value: json!("4pm"), t_event: 1, id: "a".into() },
        Update::Delta { by: 1, id: "b".into() },
    ] {
        assert!(matches!(
            s.check(p, &u),
            Err(DimensionError::AutomaticApplyOnContested { .. })
        ));
    }
}

#[test]
fn the_contested_join_also_obeys_the_three_laws() {
    // Replicas agree on what is OUTSTANDING without anyone's edit being chosen
    // for them — convergence and correctness kept apart, exactly as §VII asks.
    let a = ContestedDim::new().edit("x", Tag::new("n1", 1)).unwrap();
    let b = ContestedDim::new().edit("y", Tag::new("n2", 1)).unwrap();
    let c = ContestedDim::new().edit("z", Tag::new("n3", 1)).unwrap();
    let r = laws_hold(&a, &b, &c);
    assert!(r.commutative && r.associative && r.idempotent, "{r:?}");
    assert_eq!(a.join(&b).join(&c).live().len(), 3);
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The counterweight is the field note: the BEHAVIOUR shipped there first.
    let key = "★★_the_counterweight_and_it_is_the_STRONGEST_in_the_whole_set_the_BEHAVIOUR_is_at_parity_and_was_derived_FIRST_in_production";
    let cw = d[key].as_str().unwrap();
    assert!(cw.contains("before it had a name"));
    assert!(
        cw.contains("written down about VOS"),
        "§VII's own closing line has to survive, because it is the whole framing"
    );

    // ★ And half the declaration already existed in this core.
    assert!(d["★_and_the_snapshot_vs_contested_distinction_ALREADY_EXISTED_in_this_core"]
        .as_str()
        .unwrap()
        .contains("vclock.rs"));

    // The reconciliation done before building is recorded, not just done.
    let r = &doc["reconciliation_before_building"];
    assert!(r["★_vclock::Dimension_was_EXTENDED_not_forked"]
        .as_str()
        .unwrap()
        .contains("rather than a parallel one"));
    assert!(r["★★_and_resolve_became_a_TYPED_outcome_because_of_it"]
        .as_str()
        .unwrap()
        .contains("corrected in place"));
    assert!(r["★_the_total_order_was_REUSED_not_re_derived"].as_str().unwrap().contains("Observation"));

    // The false positive that is genuinely the right idea, wrongly used.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("balance_after")));
    assert!(fp
        .values()
        .any(|v| v.as_str().is_some_and(|s| s.contains("only the declaration is missing"))));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE DSL AUTHORING SURFACE IS NOT BUILT",
        "NO STATE-TREE INTEGRATION",
        "THE CONTESTED KIND REFUSES AUTOMATIC APPLY ENTIRELY",
        "METADATA IS GENUINELY UNBOUNDED",
        "A DECLARED KIND CANNOT BE CHANGED",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups =
        ["convergence_cases", "derived_not_decreed_cases", "type_error_cases", "scoping_cases"];
    let mut seen = BTreeSet::new();
    for group in groups {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
