fn main() {
    // ★★★ **The build stamps itself, so a stale app cannot look current.**
    //
    // A whole night's work sat in git while the app on screen was from
    // yesterday, and nobody could tell by looking. A version rendered from a
    // hardcoded string is worse than none -- it lies with confidence. These
    // come from the crate version (which `sync-version.ps1` drives from the
    // VERSION file) and from git at COMPILE time, so the number on screen is a
    // property of the binary rather than a claim about it.
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        // ★ A packaged build with no .git says so rather than inventing one.
        .unwrap_or_else(|| "nogit".to_string());
    println!("cargo:rustc-env=SUSTENA_BUILD_HASH={hash}");

    // ★ Rebuild when HEAD moves, or the stamp would go stale silently -- the
    //   exact failure this exists to prevent.
    println!("cargo:rerun-if-changed=../../../.git/HEAD");

    tauri_build::build()
}
