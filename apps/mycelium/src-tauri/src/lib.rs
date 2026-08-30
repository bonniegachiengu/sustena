//! **Mycelium** — the Sustena cockpit's Tauri v2 host.
//!
//! ★★★ `sustena-core` is compiled **into** this binary. A command below is a
//! direct call into the engine: no network, no server, no serialization hop
//! beyond the one that carries the answer to the webview. The app works with
//! the machine offline because there is nothing to be offline *from*.

pub mod driver;
pub mod arena;
#[cfg(test)]
mod arena_test;
#[cfg(test)]
mod arena_wire_test;
#[cfg(test)]
mod trust_test;
pub mod commands;
pub mod dto;
pub mod definitions;
pub mod economy;
pub mod identity;
pub mod ingest;
pub mod network;
pub mod orchie;
pub mod peers;
pub mod quorum;
#[cfg(test)]
mod quorum_test;
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
        commands::sms_permission_state,
        commands::sms_request_permission,
        commands::sms_import_page,
        commands::sms_drain_queue,
        commands::sms_queue_depth,
        commands::sms_notify_state,
        commands::sms_pending_classify,
        commands::sms_clear_prompt,
        commands::declare_source,
        commands::resolve_message,
        commands::ignore_message,
        commands::net_reversals,
        commands::get_own_identifiers,
        commands::set_own_identifiers,
        commands::apply_transfers,
        commands::learn_skip,
        commands::reclassify_spend,
        commands::defer_message,
        commands::sustain_hash,
        commands::person_hint,
        commands::link_number,
        commands::learn_rule,
        commands::get_feed,
        commands::orchie_infer,
        commands::orchie_confirm,
        commands::resolve_proposal,
        commands::get_network,
        commands::start_listening,
        commands::set_listen_port,
        commands::set_peer_address,
        commands::remember_unlock,
        commands::forget_unlock,
        commands::reconnect_peers,
        commands::add_peer,
        commands::set_peer_standing,
        commands::share_sustain,
        commands::unshare_sustain,
        commands::sync_with_peer,
        commands::get_library,
        commands::publish_package,
        commands::install_package,
        commands::pay_royalty,
        commands::get_bodies,
        commands::share_ownership,
        commands::get_peer_shelves,
        commands::fetch_package,
        commands::get_orders,
        commands::place_order,
    ])
    // ★★★ The push channel, typed from the same Rust as the commands — so a
    //   listener the UI writes for an event that does not exist will not
    //   compile.
    .events(collect_events![dto::Committed, dto::Refused, dto::RolledUp])
}

/// ★★★ Write `src/bindings.ts` from the Rust types.
///
/// Called on every **desktop debug** start, which is tauri-specta's own
/// documented pattern and has a property a build step would not: the bindings
/// cannot be stale, because running the app regenerates them. Change a field
/// in `dto.rs`, run the app, and the TypeScript build breaks until the UI
/// agrees.
///
/// ★ It is here rather than in an integration test because a test binary that
/// links the Tauri runtime fails to load on Windows with
/// `STATUS_ENTRYPOINT_NOT_FOUND`. Named rather than hidden: the export is real
/// either way, this is only *where* it is triggered from.
///
/// ★★★ **`not(mobile)`, and it is the whole of a real crash.** A debug APK
/// has `debug_assertions` on, so on a phone this ran and tried to write
/// `../src/bindings.ts` relative to the process working directory — which is
/// `/` on Android, and read-only. The `.expect` then aborted the process
/// before the first frame:
///
/// ```text
/// panicked at src\lib.rs: the TypeScript bindings must generate:
///   Io(Os { code: 30, kind: ReadOnlyFilesystem })
/// F libc: Fatal signal 6 (SIGABRT)
/// ```
///
/// ★★ The gate is not a suppressed error. There is no source tree on the
/// phone, so there is no `bindings.ts` to keep honest — the operation is
/// meaningless there rather than failing there. On desktop the hard `.expect`
/// stays exactly as it was, because that is where the staleness it prevents
/// can actually happen.
#[cfg(all(debug_assertions, not(mobile)))]
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
    use tauri::Emitter;

    let builder = specta_builder();

    // ★ Desktop only — see `export_bindings`. On a device this wrote to a
    //   read-only filesystem and aborted the process before the first frame.
    #[cfg(all(debug_assertions, not(mobile)))]
    export_bindings(&builder);

    tauri::Builder::default()
        .plugin(tauri_plugin_sms_capture::init())
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
            // ★★★ The receiver is taken BEFORE the world is handed over, so
            //     no merge can land in the gap between opening and listening.
            // ★★★ Before anything else can want it: a node told to remember
            //     its unlock comes up already able to peer, so a restart never
            //     costs a person a keystroke.
            world.unlock_if_remembered();
            let merges = world.merges();
            app.manage(world);

            // ── the absorber ────────────────────────────────────────────────
            //
            // ★★★ A peer writing into this node is the one change nothing local
            //     initiated, so it is the one change no surface would otherwise
            //     hear about. This thread re-folds what arrived and says so.
            //
            // ★★ It BLOCKS on the channel rather than polling. There is no
            //    interval to tune and no idle wakeups: the listener posts an
            //    id, this wakes, and between merges it costs nothing.
            let absorber = app.handle().clone();
            std::thread::spawn(move || {
                while let Ok(sustain_id) = merges.recv() {
                    let world = absorber.state::<world::World>();
                    world.absorb(&sustain_id);
                    // The surfaces re-read on this. ★ Emitted AFTER the fold,
                    // so anything that reacts reads the new state and not the
                    // one that was wrong.
                    let _ = absorber.emit("sustain:merged", &sustain_id);
                }
            });

            // ── reconnecting ────────────────────────────────────────────────
            //
            // ★★★ Introduced once, reachable thereafter. A peer that was asleep
            //     when this node woke is the normal case, so this sweeps rather
            //     than trying once and giving up.
            //
            // ★★ It waits before the first sweep on purpose: the identity is
            //    still locked at startup, and a locked node cannot peer. The
            //    sweep simply finds nothing until somebody unlocks, and then
            //    finds everything.
            let dialer = app.handle().clone();
            std::thread::spawn(move || loop {
                let every = {
                    let world = dialer.state::<world::World>();
                    let settings = world.network();
                    if settings.auto_reconnect && world.is_unlocked() {
                        for (peer, id, outcome) in world.reconnect_all() {
                            match outcome {
                                Ok(n) if n > 0 => {
                                    println!("[peer] reconnected to {peer}: {n} entrie(s) for {id}");
                                    let _ = dialer.emit("sustain:merged", &id);
                                }
                                Ok(_) => {}
                                // ★ Asleep is not an error a person must act on.
                                Err(e) => eprintln!("[peer] {peer} unreachable for {id}: {e}"),
                            }
                        }
                    }
                    settings.reconnect_every_secs.max(15)
                };
                std::thread::sleep(std::time::Duration::from_secs(every));
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mycelium");
}
