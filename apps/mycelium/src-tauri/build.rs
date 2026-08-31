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

    // ★★★ **Rebuild when HEAD moves -- and HEAD moving is not `.git/HEAD`
    //     changing.** This watched only `.git/HEAD`, which holds the text
    //     "ref: refs/heads/main". Committing on the same branch does not touch
    //     that file at all; git rewrites `.git/refs/heads/<branch>` instead. So
    //     the build script did not re-run on a commit, only on a branch SWITCH,
    //     and cargo served a cached stamp.
    //
    //     It shipped: an APK built during the v1.1.2 release carried the hash
    //     `b441fa4`, a commit from hours earlier, and would have rendered it in
    //     Orchie's header as if current. The desktop escaped only because that
    //     day involved enough branch switching to keep invalidating it. A stamp
    //     that silently lies is worse than no stamp, which is the whole reason
    //     this file exists -- so it now watches the ref HEAD actually points at.
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    let git = std::path::Path::new("../../../.git");
    if let Ok(head) = std::fs::read_to_string(git.join("HEAD")) {
        if let Some(reference) = head.trim().strip_prefix("ref: ") {
            // The file git actually rewrites on every commit.
            println!("cargo:rerun-if-changed=../../../.git/{reference}");
        }
        // ★ And the packed form, since a ref that has been packed away has no
        //   loose file to watch. Harmless when absent.
        println!("cargo:rerun-if-changed=../../../.git/packed-refs");
    }

    tauri_build::build()
}
