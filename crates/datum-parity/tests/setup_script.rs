//! `scripts/setup_datum.sh` against a poisoned cache (da-3p8xi).
//!
//! rust-cache restored `target/parity-cache/datum/.git` as a directory git
//! does not recognise, the script took it for a clone, and `git -C` walked up
//! to the enclosing workspace. This builds that shape: a workspace repository
//! holding a copy of the script whose `REPO_URL`, `REPO_BRANCH` and `PINNED_SHA` name a local
//! stand-in for vividn/datum, and a `.git` with no HEAD where the clone goes.

use std::fs;
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn repo_with_commits(dir: &Path, n: usize) -> Vec<String> {
    let name = dir.file_name().unwrap().to_str().unwrap();
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]);
    (0..n)
        .map(|i| {
            git(
                dir,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("{name} {i}"),
                ],
            );
            git(dir, &["rev-parse", "HEAD"])
        })
        .collect()
}

#[test]
fn a_git_dir_that_is_not_a_repository_is_recloned_not_walked_past() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // The stand-in for vividn/datum: pin its first commit, so the script
    // must check it out from the second.
    let upstream = root.join("upstream");
    let shas = repo_with_commits(&upstream, 2);
    let pinned = &shas[0];
    let branch = git(&upstream, &["rev-parse", "--abbrev-ref", "HEAD"]);

    let workspace = root.join("workspace");
    let workspace_head = &repo_with_commits(&workspace, 1)[0];

    let script =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/setup_datum.sh"))
            .unwrap();
    let script: String = script
        .lines()
        .map(|l| {
            if l.starts_with("PINNED_SHA=") {
                format!("PINNED_SHA=\"{pinned}\"")
            } else if l.starts_with("REPO_URL=") {
                format!("REPO_URL=\"{}\"", upstream.display())
            } else if l.starts_with("REPO_BRANCH=") {
                format!("REPO_BRANCH=\"{branch}\"")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let scripts = workspace.join("crates/datum-parity/scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(scripts.join("setup_datum.sh"), script).unwrap();

    // What the cache restored: a `.git` directory with no HEAD in it.
    let datum = workspace.join("target/parity-cache/datum");
    fs::create_dir_all(datum.join(".git/objects")).unwrap();

    // A pnpm that installs nothing.
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("pnpm"), "#!/bin/sh\nexit 0\n").unwrap();
    Command::new("chmod")
        .arg("+x")
        .arg(bin.join("pnpm"))
        .status()
        .unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());

    let out = Command::new("bash")
        .arg(scripts.join("setup_datum.sh"))
        .env("PATH", path)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "setup_datum.sh failed:\n{log}");
    assert!(log.contains("cloning"), "did not re-clone:\n{log}");
    assert!(
        !log.contains(workspace_head.as_str()),
        "read the workspace's HEAD:\n{log}"
    );
    assert_eq!(&git(&datum, &["rev-parse", "HEAD"]), pinned);
    assert_eq!(
        git(&datum, &["rev-parse", "--show-toplevel"]),
        datum.display().to_string()
    );
}
