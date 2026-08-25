// The android/ directory beside this file is a real Gradle module. `android_path`
// is what makes the Tauri CLI add it to the generated Android project and merge
// its manifest, which is why the receiver and the SMS permissions live here and
// not in gen/android -- that directory is regenerated and gitignored.
const COMMANDS: &[&str] = &["permission_state", "request_permission", "read_inbox", "drain_queue"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
