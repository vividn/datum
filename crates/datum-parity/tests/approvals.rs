//! vividn's consent to the goldens, enforced rather than remembered (da-iuyyw).
//!
//! «it would be absolutel imperative that these tests not change without my
//! explicit consent» (vividn, 2026-10-05). A golden is any file under
//! `goldens/` but the few in [`NOT_GOLDENS`]; `goldens/APPROVED` holds the
//! sha256 of each one vividn approved. An approved golden whose bytes changed
//! fails here, by name. One absent from `APPROVED` is PENDING — listed, not
//! failed — until vividn walks the set. `PARITY_REGEN=1` still rewrites a
//! golden; it cannot approve one.
//!
//! What keeps a can from editing `APPROVED` to match is the second half:
//! `scripts/check_approvals_signed.sh`, run on every PR by
//! `.forgejo/workflows/approvals.yml`, fails any PR commit touching it that is
//! not signed by a key in `goldens/allowed_signers`. Its tests are below.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Files under `goldens/` that are not snapshots: the lock, the keys that may
/// change it, and the fixture definitions the snapshots are taken from.
const NOT_GOLDENS: &[&str] = &["APPROVED", "allowed_signers", "text/fixtures.toml"];

#[derive(Default)]
struct Report {
    /// Lines ready to paste into `APPROVED`.
    pending: Vec<String>,
    failures: Vec<String>,
}

fn goldens(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).expect("read goldens dir") {
        let path = e.expect("dir entry").path();
        if path.is_dir() {
            goldens(root, &path, out);
            continue;
        }
        let rel = path.strip_prefix(root).unwrap().to_string_lossy();
        let rel = rel.replace('\\', "/");
        if !NOT_GOLDENS.contains(&rel.as_str()) {
            out.push(rel);
        }
    }
}

fn check(root: &Path) -> Report {
    let text = std::fs::read_to_string(root.join("APPROVED")).expect("read APPROVED");
    let mut approved = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let [path, sha, _date] = line.split_whitespace().collect::<Vec<_>>()[..] else {
            panic!("APPROVED: want `<path> <sha256> <date>`, got {line:?}");
        };
        assert!(
            approved.insert(path, sha).is_none(),
            "APPROVED names {path} twice"
        );
    }

    let mut files = Vec::new();
    goldens(root, root, &mut files);
    files.sort();
    let today = chrono::Utc::now().date_naive();
    let mut r = Report::default();
    for rel in &files {
        let sha = format!(
            "{:x}",
            Sha256::digest(std::fs::read(root.join(rel)).unwrap())
        );
        match approved.remove(rel.as_str()) {
            None => r.pending.push(format!("{rel} {sha} {today}")),
            Some(want) if want != sha => r.failures.push(format!(
                "{rel} changed since vividn approved it (sha256 {want}, now {sha}); \
                 the change needs vividn's approval"
            )),
            Some(_) => {}
        }
    }
    for gone in approved.keys() {
        r.failures.push(format!(
            "{gone} is approved but gone; removing it needs vividn's approval"
        ));
    }
    r
}

#[test]
fn approved_goldens_are_unchanged() {
    let r = check(&Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens"));
    if !r.pending.is_empty() {
        println!(
            "{} golden(s) pending vividn's approval:\n  {}",
            r.pending.len(),
            r.pending.join("\n  ")
        );
    }
    assert!(r.failures.is_empty(), "{}", r.failures.join("\n"));
}

fn scratch_goldens(approve: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("text/expected")).unwrap();
    std::fs::write(dir.path().join("text/expected/a.txt"), "golden\n").unwrap();
    std::fs::write(dir.path().join("text/fixtures.toml"), "").unwrap();
    let line = if approve {
        let sha = format!("{:x}", Sha256::digest(b"golden\n"));
        format!("text/expected/a.txt {sha} 2026-10-05\n")
    } else {
        String::new()
    };
    std::fs::write(dir.path().join("APPROVED"), format!("# header\n{line}")).unwrap();
    dir
}

#[test]
fn a_changed_byte_of_an_approved_golden_fails_naming_it() {
    let dir = scratch_goldens(true);
    let r = check(dir.path());
    assert!(r.failures.is_empty() && r.pending.is_empty());

    std::fs::write(dir.path().join("text/expected/a.txt"), "golden!").unwrap();
    let r = check(dir.path());
    assert_eq!(r.failures.len(), 1, "{:?}", r.failures);
    assert!(r.failures[0].starts_with("text/expected/a.txt changed"));
    assert!(r.failures[0].contains("needs vividn's approval"));
}

#[test]
fn an_unapproved_golden_is_pending_not_failed() {
    let dir = scratch_goldens(false);
    let r = check(dir.path());
    assert!(r.failures.is_empty(), "{:?}", r.failures);
    assert_eq!(r.pending.len(), 1, "fixtures.toml is not a golden");
    assert!(r.pending[0].starts_with("text/expected/a.txt "));
}

#[test]
fn an_approved_golden_that_is_gone_fails() {
    let dir = scratch_goldens(true);
    std::fs::remove_file(dir.path().join("text/expected/a.txt")).unwrap();
    let r = check(dir.path());
    assert_eq!(r.failures.len(), 1);
    assert!(r.failures[0].starts_with("text/expected/a.txt is approved but gone"));
}

// ── scripts/check_approvals_signed.sh, with throwaway keys ──────────────────

struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn keygen(&self, name: &str) -> PathBuf {
        let key = self.dir.path().join(format!("{name}.key"));
        let ok = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", name, "-f"])
            .arg(&key)
            .status()
            .expect("ssh-keygen is needed to test the signature check");
        assert!(ok.success());
        key
    }

    /// Commits a change to `path`, signed with `key` if given.
    fn commit(&self, path: &str, key: Option<&Path>) -> String {
        let file = self.dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let old = std::fs::read_to_string(&file).unwrap_or_default();
        std::fs::write(&file, format!("{old}x\n")).unwrap();
        self.git(&["add", path]);
        let mut args = vec!["-c", "gpg.format=ssh"];
        let signing;
        if let Some(key) = key {
            signing = format!("user.signingkey={}", key.display());
            args.extend(["-c", &signing, "commit", "-S"]);
        } else {
            args.push("commit");
        }
        args.extend(["-q", "-m", path]);
        self.git(&args);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Runs the check on `base..HEAD`: its exit code and stdout.
    fn check(&self, base: &str) -> (Option<i32>, String) {
        let script = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check_approvals_signed.sh"
        );
        let signers = self.dir.path().join("signers");
        let out = Command::new("bash")
            .arg(script)
            .arg(&signers)
            .args([base, "HEAD"])
            .current_dir(self.dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        println!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
        (out.status.code(), stdout)
    }

    /// The check REFUSED, with a `FAIL` line holding `why`: exit 1 alone is
    /// also what a crashed or missing script looks like.
    fn assert_red(&self, base: &str, why: &str) {
        let (code, out) = self.check(base);
        assert_eq!(code, Some(1), "{out}");
        assert!(
            out.lines()
                .any(|l| l.starts_with("FAIL  ") && l.contains(why)),
            "no FAIL line naming {why}:\n{out}"
        );
    }
}

const APPROVED: &str = "crates/datum-parity/goldens/APPROVED";

/// A repo whose base commit trusts `vividn`'s key, plus `vividn` and `mallory`.
fn signed_repo() -> (Repo, String, PathBuf, PathBuf) {
    let repo = Repo {
        dir: tempfile::tempdir().unwrap(),
    };
    repo.git(&["init", "-q", "-b", "main"]);
    repo.git(&["config", "user.name", "vividn"]);
    repo.git(&["config", "user.email", "vividn@example.com"]);
    let vividn = repo.keygen("vividn");
    let mallory = repo.keygen("mallory");
    let public = std::fs::read_to_string(vividn.with_extension("key.pub")).unwrap();
    std::fs::write(
        repo.dir.path().join("signers"),
        format!("# a comment\nnate@example.com namespaces=\"git\" {public}"),
    )
    .unwrap();
    let base = repo.commit(APPROVED, None);
    (repo, base, vividn, mallory)
}

#[test]
fn an_unsigned_commit_touching_approved_is_red() {
    let (repo, base, _, _) = signed_repo();
    let c = repo.commit(APPROVED, None);
    repo.assert_red(&base, &c);
}

#[test]
fn a_commit_signed_by_a_key_not_allowed_is_red() {
    let (repo, base, _, mallory) = signed_repo();
    let c = repo.commit(APPROVED, Some(&mallory));
    repo.assert_red(&base, &c);
}

#[test]
fn a_commit_signed_by_the_allowed_key_is_green() {
    let (repo, base, vividn, _) = signed_repo();
    repo.commit("unrelated.txt", None);
    let c = repo.commit(APPROVED, Some(&vividn));
    let (code, out) = repo.check(&base);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains(&format!("ok    {c}")), "{out}");
}

#[test]
fn the_check_guards_itself() {
    let (repo, base, _, _) = signed_repo();
    let c = repo.commit("crates/datum-parity/goldens/allowed_signers", None);
    repo.assert_red(&base, &c);
    let (repo, base, _, _) = signed_repo();
    let c = repo.commit(".forgejo/workflows/approvals.yml", None);
    repo.assert_red(&base, &c);
}

/// vremya's hole in #441: `diff-tree --cc` leaves out a path a merge took
/// whole from one parent, so a merge with an OLD main commit brings back its
/// `APPROVED` while no commit in `base..head` touches it.
#[test]
fn a_merge_bringing_back_an_older_approved_is_red() {
    let (repo, old, _, _) = signed_repo();
    let base = repo.commit(APPROVED, None);
    let p1 = repo.commit("unrelated.txt", None);
    repo.git(&["checkout", "-q", &old, "--", APPROVED]);
    let tree = repo.git(&["write-tree"]);
    let merge = repo.git(&["commit-tree", &tree, "-p", &p1, "-p", &old, "-m", "merge"]);
    repo.git(&["reset", "-q", "--hard", &merge]);
    assert_ne!(repo.git(&["diff", "--name-only", &base, "HEAD"]), "");
    repo.assert_red(&base, "merge commit");
}
