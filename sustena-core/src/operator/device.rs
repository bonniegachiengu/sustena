//! The sensor as a Sustain — `device.heartbeat` (INGEST §IX).
//!
//! ★★★ §IX's whole point is that a sensor needs **no bespoke alerting
//! subsystem**. A device is an ordinary Sustain with its own dimensions and its
//! own viable region, so a flat battery escalates by the same path an overdrawn
//! pocket does, and the connection panel is a view over its state rather than a
//! separate thing to build.
//!
//! ```text
//! S_dev = (queue_depth, t_last_ack, app_version, ...)
//! V_dev = { queue_depth <= Q_max AND now − t_last_ack <= theta(c) }
//! ```
//!
//! ★★ `queue_depth` is the leading indicator, and §IX says why: it rises before
//! anything else visibly breaks, because a device that cannot deliver keeps
//! accepting. A health model built only on last-contact lags by construction.
//!
//! ★★★ **The liveness half cannot be self-enforced.** A phone that has stopped
//! cannot write its own `t_last_ack`, so `now − t_last_ack <= theta` has to be
//! evaluated by whoever is watching, not by the device. That is the same reason
//! a whole folds a part's state rather than trusting the part's self-report —
//! and it is why this operator only ever RECORDS, and never judges.

use serde_json::{json, Map, Value};

use super::meta::{OperatorMeta, OperatorResult, ParamDecl, Protocol, Registry};
use super::EmittedEvent;
use crate::flow::Movement;
use crate::state::State;

fn num(params: &Map<String, Value>, key: &str) -> f64 {
    params.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(params: &Map<String, Value>, key: &str) -> String {
    params.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Record that the device is alive, and how far behind it is.
///
/// ★ The clock is the HOST's. A core whose promise is reproducibility cannot
/// read a clock: the same inputs would produce different state on every run and
/// no conformance vector could pin it. Time comes in as a parameter, exactly as
/// `budget.record_income` takes `received_at`.
fn heartbeat(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let depth = num(params, "queue_depth");
    if depth < 0.0 {
        return OperatorResult::fail(
            "A queue cannot be shorter than empty.",
            "queue_depth_non_negative",
        );
    }
    let at = num(params, "at_ms");
    if at <= 0.0 {
        return OperatorResult::fail(
            "A heartbeat needs the moment it was taken.",
            "heartbeat_has_a_time",
        );
    }

    let _ = state.set("device.queue_depth", json!(depth));
    let _ = state.set("device.last_ack_ms", json!(at));
    let version = text(params, "app_version");
    if !version.is_empty() {
        let _ = state.set("device.app_version", json!(version));
    }

    events.push(EmittedEvent {
        name: "event.device.heartbeat".into(),
        payload: json!({"queue_depth": depth, "at_ms": at}),
    });

    OperatorResult::ok(json!({"queue_depth": depth, "at_ms": at}))
}

pub fn register(registry: &mut Registry) {
    registry.register(OperatorMeta {
        name: "device.heartbeat",
        description: "Record that a capture device is alive, and how far behind it is.",
        params: vec![
            ParamDecl::number("queue_depth"),
            ParamDecl::number("at_ms"),
            ParamDecl::text("app_version").optional(),
        ],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.device.heartbeat"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: heartbeat,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::{execute, Enforcement};

    fn blank() -> Value {
        json!({"device": {"queue_depth": 0.0, "last_ack_ms": 0.0, "app_version": ""}})
    }

    fn call(state: &Value, ps: &[(&str, Value)]) -> crate::operator::Execution {
        let reg = Registry::default();
        let params: Map<String, Value> =
            ps.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        execute(&reg, &reg.names(), &Enforcement::default(), state, "device.heartbeat", &params)
    }

    #[test]
    fn a_heartbeat_records_the_depth_and_the_moment() {
        let ex = call(&blank(), &[("queue_depth", json!(3.0)), ("at_ms", json!(1_700_000.0))]);
        assert!(ex.committed());
        assert_eq!(ex.state.pointer("/device/queue_depth").and_then(Value::as_f64), Some(3.0));
        assert_eq!(ex.state.pointer("/device/last_ack_ms").and_then(Value::as_f64), Some(1_700_000.0));
    }

    #[test]
    fn a_heartbeat_with_no_time_is_refused() {
        // ★★★ A heartbeat's whole value is WHEN it was. One without a time
        //     would look like contact and prove nothing.
        let ex = call(&blank(), &[("queue_depth", json!(0.0))]);
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("heartbeat_has_a_time"));
    }

    #[test]
    fn a_negative_queue_is_refused_rather_than_stored() {
        let ex = call(&blank(), &[("queue_depth", json!(-1.0)), ("at_ms", json!(1.0))]);
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("queue_depth_non_negative"));
    }

    #[test]
    fn the_device_never_judges_its_own_liveness() {
        // ★★★ §IX: a dead phone cannot write its own t_last_ack, so nothing
        //     here may decide whether it is late. It records; the watcher
        //     judges. If this operator ever gained a "stale" field it would be
        //     a self-report from exactly the party that cannot be trusted to
        //     make it.
        let ex = call(&blank(), &[("queue_depth", json!(0.0)), ("at_ms", json!(1.0))]);
        let dev = ex.state.pointer("/device").and_then(Value::as_object).unwrap();
        for forbidden in ["stale", "healthy", "alive", "late"] {
            assert!(!dev.contains_key(forbidden), "the device judged itself: {forbidden}");
        }
    }

    #[test]
    fn a_later_heartbeat_replaces_the_earlier_one() {
        let first = call(&blank(), &[("queue_depth", json!(9.0)), ("at_ms", json!(100.0))]);
        let second = call(&first.state, &[("queue_depth", json!(0.0)), ("at_ms", json!(200.0))]);
        assert_eq!(second.state.pointer("/device/queue_depth").and_then(Value::as_f64), Some(0.0));
        assert_eq!(second.state.pointer("/device/last_ack_ms").and_then(Value::as_f64), Some(200.0));
    }
}
