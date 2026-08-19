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

    mycelium_lib::specta_builder()
        .export(
            Typescript::default()
                .header("// GENERATED from the Rust types by tauri-specta. Do not edit.\n"),
            "../src/bindings.ts",
        )
        .expect("the TypeScript bindings must generate");

    println!("wrote ../src/bindings.ts");
}
