//! **Mycelium** — the Sustena cockpit's Tauri v2 host.
//!
//! ★★★ `sustena-core` is compiled **into** this binary. A command below is a
//! direct call into the engine: no network, no server, no serialization hop
//! beyond the one that carries the answer to the webview. The app works with
//! the machine offline because there is nothing to be offline *from*.

pub mod commands;
pub mod dto;
pub mod engine;

use tauri_specta::{collect_commands, Builder};

/// The typed command surface, defined once.
///
/// ★★ Both the runtime handler and the generated TypeScript come from **this
/// one list**, so a command that exists in Rust and not in TS is not a thing
/// that can happen.
pub fn specta_builder() -> Builder {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::get_sustain,
        commands::run_operator,
        commands::reset_sustain,
    ])
}

/// ★★★ Write `src/bindings.ts` from the Rust types.
///
/// Called on every **debug** start, which is tauri-specta's own documented
/// pattern and has a property a build step would not: the bindings cannot be
/// stale, because running the app regenerates them. Change a field in
/// `dto.rs`, run the app, and the TypeScript build breaks until the UI agrees.
///
/// ★ It is here rather than in an integration test because a test binary that
/// links the Tauri runtime fails to load on Windows with
/// `STATUS_ENTRYPOINT_NOT_FOUND`. Named rather than hidden: the export is real
/// either way, this is only *where* it is triggered from.
#[cfg(debug_assertions)]
fn export_bindings(builder: &Builder) {
    use specta_typescript::Typescript;
    builder
        .export(
            Typescript::default()
                .header("// GENERATED from the Rust types by tauri-specta. Do not edit.
"),
            "../src/bindings.ts",
        )
        .expect("the TypeScript bindings must generate");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    export_bindings(&builder);

    tauri::Builder::default()
        .manage(engine::Engine::default())
        .invoke_handler(builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running mycelium");
}
