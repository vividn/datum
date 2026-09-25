//! `DATUM_BUILD` — `3.beta.<height> (<sha7>)` — for `datum --version`
//! (da-5172a). The `3.beta` is Cargo's `3.0.0-beta`, the rest is git's; see
//! `build_id.rs`.

use std::path::Path;

mod build_id;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ci_sha = std::env::var("GITHUB_SHA").ok().filter(|s| !s.is_empty());
    let (height, commit) = build_id::build_id(dir, ci_sha.as_deref());
    let major = env!("CARGO_PKG_VERSION_MAJOR");
    let pre = env!("CARGO_PKG_VERSION_PRE");
    if height == "unknown" {
        println!("cargo:warning=no whole git history (a shallow clone, or no .git): the version says {major}.{pre}.unknown");
    }
    println!("cargo:rustc-env=DATUM_BUILD={major}.{pre}.{height} ({commit})");

    // Rerun when HEAD moves, the index or a ref changes, or a crate is edited
    // (the `+`). A path that does not exist would rerun every build.
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-changed=build_id.rs");
    let rev_parse = |flag: &str| {
        std::process::Command::new("git")
            .current_dir(dir)
            .args(["rev-parse", "--path-format=absolute", flag])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let (Some(git_dir), Some(common)) = (rev_parse("--git-dir"), rev_parse("--git-common-dir"))
    else {
        return;
    };
    let watched = [
        format!("{git_dir}/HEAD"),
        format!("{git_dir}/index"),
        format!("{common}/refs"),
        format!("{common}/packed-refs"),
        format!("{}/..", dir.display()),
    ];
    for path in watched.iter().filter(|p| Path::new(p).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
}
