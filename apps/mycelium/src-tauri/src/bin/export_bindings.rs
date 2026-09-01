//! `cargo run --bin export_bindings` — write `src/bindings.ts` without starting
//! the app.
//!
//! ★ A plain binary rather than an integration test: a test harness linking the
//! Tauri runtime fails to load on Windows (`STATUS_ENTRYPOINT_NOT_FOUND`),
//! whereas an ordinary binary is exactly what the app itself is. The app also
//! regenerates on every debug start, so this exists for CI and for regenerating
//! without opening a window.
fn main() {
    use specta_typescript::Typescript;

    // ★★ Anchored to THIS crate, not to whatever directory the caller happens
    //    to be in. A plain "../src/bindings.ts" is correct when the app starts
    //    itself from `src-tauri` and silently wrong when npm runs it from the
    //    web root -- which is exactly what happened: a freshly generated file
    //    landed in `apps/src/bindings.ts`, the real one was never touched, and
    //    a new command looked like it had failed to export.
    let out = format!("{}/../src/bindings.ts", env!("CARGO_MANIFEST_DIR"));

    mycelium_lib::specta_builder()
        .export(
            Typescript::default()
                .header("// GENERATED from the Rust types by tauri-specta. Do not edit.\n"),
            &out,
        )
        .expect("the TypeScript bindings must generate");

    println!("wrote {out}");
}
