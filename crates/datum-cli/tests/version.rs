//! `datum --version` is `datum 3.beta.<height> (<sha7>)`, the height and the
//! commit read from git at build time, and `unknown` wherever git cannot say
//! for certain (da-5172a).

use std::path::Path;
use std::process::Command;

#[path = "../build_id.rs"]
mod build_id;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
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
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A repo of `n` commits, and its HEAD.
fn repo(n: usize) -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    git(tmp.path(), &["init", "-q"]);
    for i in 0..n {
        git(
            tmp.path(),
            &["commit", "-q", "--allow-empty", "-m", &i.to_string()],
        );
    }
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    (tmp, head)
}

#[test]
fn the_binary_prints_3_beta_height_and_sha7() {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let line = String::from_utf8(out.stdout).unwrap();
    let rest = line.strip_prefix("datum 3.beta.").expect(&line);
    let (height, commit) = rest.trim_end().split_once(' ').expect(&line);
    assert!(
        height == "unknown" || height.parse::<u32>().is_ok(),
        "{line:?}"
    );
    let sha = commit
        .strip_prefix('(')
        .and_then(|c| c.strip_suffix(')'))
        .expect(&line);
    let sha = sha.strip_suffix('+').unwrap_or(sha);
    assert!(
        sha.len() == 7 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "{line:?}"
    );
}

#[test]
fn the_height_is_every_commit_reachable_and_a_dirty_tree_says_so() {
    let (tmp, head) = repo(5);
    assert_eq!(
        build_id::build_id(tmp.path(), None),
        ("5".into(), head[..7].into())
    );
    std::fs::write(tmp.path().join("new"), "").unwrap();
    assert_eq!(
        build_id::build_id(tmp.path(), None).1,
        format!("{}+", &head[..7])
    );
    // CI names its commit, and a checkout it made is not dirty for us.
    let first = git(tmp.path(), &["rev-list", "--max-parents=0", "HEAD"]);
    assert_eq!(
        build_id::build_id(tmp.path(), Some(&first)),
        ("1".into(), first[..7].into())
    );
}

#[test]
fn a_shallow_clone_says_unknown_unless_it_holds_the_root() {
    let (src, head) = repo(3);
    let url = format!("file://{}", src.path().display());
    for (depth, height) in [("1", "unknown"), ("2", "unknown"), ("3", "3")] {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["clone", "-q", "--depth", depth, &url, "c"]);
        let got = build_id::build_id(&tmp.path().join("c"), None);
        assert_eq!(got, (height.into(), head[..7].into()), "--depth {depth}");
    }
}

#[test]
fn no_git_says_unknown() {
    let tmp = tempfile::tempdir().unwrap();
    let got = build_id::build_id(tmp.path(), None);
    assert_eq!(got, ("unknown".into(), "unknown".into()));
}
