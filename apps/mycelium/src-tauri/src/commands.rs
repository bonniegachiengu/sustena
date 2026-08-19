//! The command surface — three, and every one of them calls the real engine.
//!
//! ★ A walking skeleton earns the right to be small. What is NOT here is named
//! in the app's README rather than stubbed into existence: no simulation, no
//! event log, no economy, no definition editor. A command that returned
//! plausible-looking nothing would be worse than an absent one.

use serde_json::{Map, Value};
use tauri::State;

use crate::dto::{GateResult, SustainDto};
use crate::engine::{Engine, SUSTAIN_ID, SUSTAIN_LABEL};

/// ★ Every command logs what it was asked and what the engine answered.
///
/// Not decoration: it is how a real IPC call from the window is *observable*
/// from outside the webview. A dev run that prints these is a dev run where the
/// UI genuinely reached the engine.
macro_rules! trace {
    ($($t:tt)*) => { println!("[ipc] {}", format!($($t)*)) };
}

/// `Σ` and its current state.
#[tauri::command]
#[specta::specta]
pub fn get_sustain(engine: State<'_, Engine>) -> SustainDto {
    trace!("get_sustain -> gate armed={} operators={}", engine.enforcement.enabled, engine.definition.operators.len());
    SustainDto::of(
        SUSTAIN_ID,
        SUSTAIN_LABEL,
        &engine.definition,
        engine.enforcement.enabled,
        &engine.snapshot(),
    )
}

/// ★★★ Ask the engine to do something, and report what the gate decided.
///
/// Returns `GateResult` — **not** `Result<_, String>`. A refusal is a correct
/// answer, and typing it as an error would make the UI render the system
/// working as the system failing.
#[tauri::command]
#[specta::specta]
pub fn run_operator(
    engine: State<'_, Engine>,
    operator: String,
    params: Value,
) -> GateResult {
    let params: Map<String, Value> = match params {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    trace!("run_operator  {operator}  {}", serde_json::to_string(&params).unwrap_or_default());
    let x = engine.call(&operator, &params);
    let out = GateResult::of(&operator, &x);
    trace!(
        "  -> {:?}  mutations={} events={} reason={}",
        out.verdict,
        out.mutations,
        out.events.len(),
        out.reason.clone().unwrap_or_else(|| "-".into())
    );
    out
}

/// Start over from the opening state.
#[tauri::command]
#[specta::specta]
pub fn reset_sustain(engine: State<'_, Engine>) -> SustainDto {
    engine.reset();
    get_sustain(engine)
}
