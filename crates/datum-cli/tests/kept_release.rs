//! The kept releases (da-ec8y8): `release/kept-tags.sh` names each commit
//! that raises the sync floor and the one before it, tagged for the version
//! its build prints, and the one before as the last of its minor, in a repo
//! whose migrations file is this tree's with a floor appended and the minor
//! raised, as a floor is.

use std::path::Path;
use std::process::Command;

use datum_core::migrations::SYNC_FLOOR;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// `kept-tags.sh <before> <after>` in `dir`: its lines, or its stderr.
fn kept(dir: &Path, before: &str, after: &str) -> Result<Vec<String>, String> {
    let out = Command::new("bash")
        .arg(root().join("release/kept-tags.sh"))
        .args([before, after])
        .current_dir(dir)
        .output()
        .unwrap();
    match out.status.success() {
        true => Ok(String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(String::from)
            .collect()),
        false => Err(String::from_utf8(out.stderr).unwrap()),
    }
}

#[test]
fn a_floor_raising_commit_and_the_one_before_it_are_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mods = "crates/datum-core/src/migrations/mod.rs";
    let real = std::fs::read_to_string(root().join(mods)).unwrap();
    git(dir, &["init", "-q"]);
    std::fs::create_dir_all(dir.join("crates/datum-core/src/migrations")).unwrap();
    std::fs::copy(root().join("Cargo.toml"), dir.join("Cargo.toml")).unwrap();
    let commit = |text: &str, msg: &str| {
        std::fs::write(dir.join(mods), text).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", msg]);
        git(dir, &["rev-parse", "HEAD"])
    };
    let before = commit(&real, "this tree's floors");
    let (head, tail) = real.split_at(real.find("pub const SYNC_FLOORS").unwrap());
    let end = tail.find("\n];").unwrap();
    let raised = format!(
        "{head}{}\n    // a new floor\n    999,{}",
        &tail[..end],
        &tail[end..]
    );
    // The floor raises the minor with it, as the_minor_version_counts_the_sync_floors holds.
    let (major, minor) = (
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR").parse::<u32>().unwrap(),
    );
    let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let bumped = cargo.replacen(
        &format!("version = \"{major}.{minor}."),
        &format!("version = \"{major}.{}.", minor + 1),
        1,
    );
    assert_ne!(bumped, cargo, "the workspace version line");
    std::fs::write(dir.join("Cargo.toml"), bumped).unwrap();
    let floor = commit(&raised, "a floor");
    let after = commit(&format!("{raised}\n// unrelated\n"), "after");

    // Both builds by version, and the one before as its minor's last too.
    let want = vec![
        format!("v{major}.{minor}.1 {before} {SYNC_FLOOR}"),
        format!("v{major}.{}.2 {floor} 999", minor + 1),
        format!("v{major}.{minor} {before} {SYNC_FLOOR}"),
    ];
    assert_eq!(kept(dir, &before, &after).unwrap(), want);
    assert_eq!(kept(dir, "", &after).unwrap(), want, "the whole history");
    assert_eq!(kept(dir, &floor, &after).unwrap(), Vec::<String>::new());

    // A push whose old tip is not under the new one tags nothing.
    let said = kept(dir, &after, &before).unwrap_err();
    assert!(said.contains("is not an ancestor"), "{said}");
}
