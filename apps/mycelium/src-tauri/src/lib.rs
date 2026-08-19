//! **Mycelium** — the Sustena cockpit's Tauri v2 host.
//!
//! ★★★ `sustena-core` is compiled **into** this binary. A command below is a
//! direct call into the engine: no network, no server, no serialization hop
//! beyond the one that carries the answer to the webview. The app works with
//! the machine offline because there is nothing to be offline *from*.

pub mod arena;
#[cfg(test)]
mod arena_test;
pub mod commands;
pub mod dto;
pub mod definitions;
pub mod economy;
pub mod identity;
pub mod ingest;
pub mod orchie;
pub mod peers;
#[cfg(test)]
mod peers_test;
pub mod store;
pub mod templates;
pub mod widgets;
pub mod wire;
pub mod world;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder};

/// The typed command surface, defined once.
///
/// ★★ Both the runtime handler and the generated TypeScript come from **this
/// one list**, so a command that exists in Rust and not in TS is not a thing
/// that can happen.
pub fn specta_builder() -> Builder {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::get_world,
        commands::get_sustain,
        commands::select_sustain,
        commands::create_sustain,
        commands::run_operator,
        commands::get_constraints,
        commands::get_log,
        commands::get_operators,
        commands::simulate,
        commands::get_economy,
        commands::set_parameter,
        commands::get_definitions,
        commands::author_definition,
        commands::create_from_definition,
        commands::get_access,
        commands::get_rollup,
        commands::transfer,
        commands::get_identity,
        commands::unlock_identity,
        commands::enrol_identity,
        commands::lock_identity,
        commands::get_ingest,
        commands::capture_message,
        commands::declare_source,
        commands::resolve_message,
        commands::learn_rule,
        commands::get_feed,
        commands::orchie_infer,
        commands::orchie_confirm,
        commands::resolve_proposal,
        commands::get_network,
        commands::start_listening,
        commands::add_peer,
        commands::set_peer_standing,
        commands::share_sustain,
        commands::unshare_sustain,
        commands::sync_with_peer,
        commands::get_library,
        commands::publish_package,
        commands::install_package,
        commands::pay_royalty,
    ])
    // ★★★ The push channel, typed from the same Rust as the commands — so a
    //   listener the UI writes for an event that does not exist will not
    //   compile.
    .events(collect_events![dto::Committed, dto::Refused, dto::RolledUp])
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
        .invoke_handler(builder.invoke_handler())
        // ★ ONE setup. `tauri::Builder::setup` replaces rather than chains, so
        //   a second call would silently discard the first — and the one it
        //   discarded would have been the household.
        .setup(move |app| {
            // Registers the typed events, so `Committed::emit` reaches the
            // webview under the name the generated TS listens for.
            builder.mount_events(app);

            // ★★★ The household is opened from disk BEFORE the window can ask
            //   for it, and every log is folded on the way in. If the store is
            //   empty this is also where Bonnie's household is seeded — once,
            //   and only into emptiness.
            let dir = app.path().app_data_dir().map_err(|e| format!("no app data dir: {e}"))?;
            let store = store::Store::at(&dir).map_err(|e| e.to_string())?;
            let world = world::World::open(store).map_err(|e| e.to_string())?;
            let seeded = world.seed_if_empty().map_err(|e| e.to_string())?;
            println!(
                "[store] {} · {} sustain(s){}",
                dir.display(),
                world.with(|i| i.order().len()),
                if seeded { " · seeded" } else { " · loaded from log" }
            );
            app.manage(world);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mycelium");
}
