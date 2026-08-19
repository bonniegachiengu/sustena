//! `cargo run --bin smoke` — drive the host's engine exactly as the Tauri
//! commands do, and print the real JSON that crosses the IPC boundary.
//!
//! ★★ This is **not** a substitute for running the app; it is the leg of the
//! chain that can be evidenced as text. It calls the same `Engine` the commands
//! call and builds the same `GateResult` DTO they return, so what it prints is
//! byte-for-byte what the webview receives — the UI's job is then only to
//! render it.
//!
//! ★ What it deliberately does NOT prove: the click, the IPC hop, and the
//! rendering. Those need the window, and are shown there.

use serde_json::{json, Map, Value};

use mycelium_lib::dto::{GateResult, SustainDto};
use mycelium_lib::engine::{Engine, SUSTAIN_ID, SUSTAIN_LABEL};

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn show(label: &str, v: &impl serde::Serialize) {
    println!("\n── {label} {}", "─".repeat(60usize.saturating_sub(label.len())));
    println!("{}", serde_json::to_string_pretty(v).expect("serializable"));
}

fn main() {
    let engine = Engine::default();

    let sustain = SustainDto::of(
        SUSTAIN_ID,
        SUSTAIN_LABEL,
        &engine.definition,
        engine.enforcement.enabled,
        &engine.snapshot(),
    );
    show("get_sustain  (opening)", &sustain);

    for (op, p) in [
        ("budget.record_income", params(&[("amount", json!(4500.0)), ("source", json!("salary"))])),
        ("budget.allocate", params(&[("pocket_name", json!("food")), ("amount", json!(1200.0))])),
        ("budget.spend", params(&[("pocket_name", json!("food")), ("amount", json!(340.0))])),
        // ★ The one that must come back REFUSED, with the engine's own words.
        ("budget.spend", params(&[("pocket_name", json!("food")), ("amount", json!(9000.0))])),
    ] {
        let x = engine.call(op, &p);
        let result = GateResult::of(op, &x);
        show(&format!("run_operator  {op}"), &result);
    }

    let after = SustainDto::of(
        SUSTAIN_ID,
        SUSTAIN_LABEL,
        &engine.definition,
        engine.enforcement.enabled,
        &engine.snapshot(),
    );
    show("get_sustain  (after)", &after.state);
}
