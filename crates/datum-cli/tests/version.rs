//! `datum --version` is `datum 3.0.<height>-beta (<sha7>)`, valid semver
//! whose patch is the commit height, read from git at build time; a height
//! git cannot say for certain is `0`, a commit `unknown` (da-5172a, da-cv118).

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
fn the_binary_prints_semver_whose_patch_is_the_height_and_the_sha7() {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let line = String::from_utf8(out.stdout).unwrap();
    let rest = line.strip_prefix("datum ").expect(&line);
    let (semver, commit) = rest.trim_end().split_once(' ').expect(&line);
    let (core, pre) = semver.split_once('-').expect(&line);
    assert_eq!(pre, env!("CARGO_PKG_VERSION_PRE"), "{line:?}");
    let parts: Vec<&str> = core.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()),
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
    // The patch is this checkout's `git rev-list --count`, or 0 where the
    // history is cut short. Dirtiness may have moved since the build.
    let ci_sha = std::env::var("GITHUB_SHA").ok().filter(|s| !s.is_empty());
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (height, commit) = build_id::build_id(dir, ci_sha.as_deref());
    let want = build_id::version(&height, commit.trim_end_matches('+'));
    assert_eq!(line.trim_end().replace("+)", ")"), format!("datum {want}"));
}

#[test]
fn the_version_is_cargos_with_the_height_for_patch() {
    assert_eq!(
        build_id::version("1263", "cdb0266"),
        "3.0.1263-beta (cdb0266)"
    );
    // Semver needs a number: no whole history is patch 0.
    assert_eq!(
        build_id::version("unknown", "cdb0266"),
        "3.0.0-beta (cdb0266)"
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

#[test]
fn an_archived_tree_says_the_commit_git_archive_wrote_into_it() {
    let (src, _) = repo(2);
    std::fs::write(src.path().join(".gitattributes"), "COMMIT export-subst\n").unwrap();
    std::fs::write(src.path().join("COMMIT"), "$Format:%H$\n").unwrap();
    git(src.path(), &["add", "."]);
    git(src.path(), &["commit", "-q", "-m", "stamp"]);
    let head = git(src.path(), &["rev-parse", "HEAD"]);
    let out = tempfile::tempdir().unwrap();
    let tar = format!("{}/t.tar", out.path().display());
    git(src.path(), &["archive", "-o", &tar, "HEAD"]);
    let x = Command::new("tar")
        .args(["-xf", &tar, "-C"])
        .arg(out.path())
        .status()
        .unwrap();
    assert!(x.success());
    assert_eq!(
        build_id::build_id(out.path(), None),
        ("unknown".into(), head[..7].into())
    );
    // Never archived, the placeholder is no commit.
    std::fs::write(out.path().join("COMMIT"), "$Format:%H$\n").unwrap();
    assert_eq!(
        build_id::build_id(out.path(), None),
        ("unknown".into(), "unknown".into())
    );
}

/// This repo's own `.gitattributes` names where `COMMIT` lives: archived,
/// it holds the commit. Skipped in a tree with no git. Archived from the top
/// level, as a release is: git before 2.41 (CI has 2.39) mishandles
/// attributes when `git archive` runs in a subdirectory.
#[test]
fn this_repos_commit_file_is_substituted_by_git_archive() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let head = Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .unwrap();
    if !head.status.success() {
        return;
    }
    let archive = Command::new("sh")
        .current_dir(dir)
        .args([
            "-c",
            r#"p=$(git rev-parse --show-prefix) && cd "$(git rev-parse --show-toplevel)" && git archive HEAD "${p}COMMIT" | tar -xO"#,
        ])
        .output()
        .unwrap();
    assert!(archive.status.success(), "{archive:?}");
    assert_eq!(
        String::from_utf8(archive.stdout).unwrap().trim(),
        String::from_utf8(head.stdout).unwrap().trim()
    );
}
