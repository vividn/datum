//! scripts/lockfile-check.py (da-jks33) against a fixture repo: what a PR adds
//! to or changes in a lockfile is named, what it only removes is not, and a
//! `git` source in Cargo.lock fails.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// A Cargo.lock of a workspace crate and these (name, version, source).
fn cargo_lock(workspace: &str, packages: &[(&str, &str, &str)]) -> String {
    let mut lock =
        format!("version = 4\n\n[[package]]\nname = \"datum-cli\"\nversion = \"{workspace}\"\n");
    for (name, version, source) in packages {
        lock += &format!(
            "\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\nsource = \"{source}\"\nchecksum = \"aa\"\n"
        );
    }
    lock
}

/// A pnpm lockfile holding these (key, integrity), its snapshots and importers
/// beside them as a real one's are, which are no packages.
fn pnpm_lock(packages: &[(&str, &str)]) -> String {
    let mut lock = "lockfileVersion: '9.0'\n\nimporters:\n\n  .:\n    dependencies:\n      in-importers:\n        version: 9.9.9\n\npackages:\n\n".to_string();
    for (key, resolution) in packages {
        lock +=
            &format!("  '{key}':\n    resolution: {resolution}\n    engines: {{node: '>=18'}}\n\n");
    }
    lock += "snapshots:\n\n  in-snapshots@9.9.9: {}\n";
    lock
}

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Repo {
    /// A repo whose first commit, the base, holds the two lockfiles.
    fn new(case: &str) -> Repo {
        let dir =
            std::env::temp_dir().join(format!("lockfile-check-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("web")).unwrap();
        let repo = Repo(dir);
        repo.git(&["init", "-q", "-b", "base"]);
        repo.commit(
            &cargo_lock(
                "0.1.0",
                &[
                    ("kept", "1.0.0", REGISTRY),
                    ("bumped", "1.0.0", REGISTRY),
                    ("gone", "1.0.0", REGISTRY),
                ],
            ),
            &pnpm_lock(&[
                ("@scope/kept@1.0.0", "{integrity: sha512-k}"),
                ("@scope/bumped@1.0.0", "{integrity: sha512-b}"),
                ("gone@1.0.0", "{integrity: sha512-g}"),
            ]),
        );
        repo
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args(["-c", "user.name=A", "-c", "user.email=a@example.com"])
            .args(args)
            .current_dir(&self.0)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(&self, cargo: &str, pnpm: &str) {
        std::fs::write(self.0.join("Cargo.lock"), cargo).unwrap();
        std::fs::write(self.0.join("web/pnpm-lock.yaml"), pnpm).unwrap();
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", "x"]);
    }

    /// The check of HEAD against its parent: its exit, what it named, its stderr.
    fn check(&self) -> (Output, String) {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/lockfile-check.py");
        let out = Command::new("python3")
            .arg(script)
            .args(["HEAD^", "HEAD"])
            .current_dir(&self.0)
            .output()
            .unwrap();
        let named = String::from_utf8_lossy(&out.stdout).into_owned();
        (out, named)
    }
}

#[test]
fn an_added_package_and_a_bumped_one_are_both_named_in_either_lockfile() {
    let repo = Repo::new("added");
    repo.commit(
        &cargo_lock(
            "0.2.0",
            &[
                ("kept", "1.0.0", REGISTRY),
                ("bumped", "1.1.0", REGISTRY),
                ("gone", "1.0.0", REGISTRY),
                ("new", "2.0.0", REGISTRY),
            ],
        ),
        &pnpm_lock(&[
            ("@scope/kept@1.0.0", "{integrity: sha512-k}"),
            ("@scope/bumped@1.1.0", "{integrity: sha512-c}"),
            ("gone@1.0.0", "{integrity: sha512-g}"),
            ("new@2.0.0", "{integrity: sha512-n}"),
        ]),
    );
    let (out, named) = repo.check();
    assert!(out.status.success(), "{named}");
    assert_eq!(
        named.lines().collect::<Vec<_>>(),
        [
            "Cargo.lock: bumped 1.0.0 -> 1.1.0",
            "Cargo.lock: new 2.0.0 added",
            "web/pnpm-lock.yaml: @scope/bumped 1.0.0 -> 1.1.0",
            "web/pnpm-lock.yaml: new 2.0.0 added",
        ],
        "and the workspace crate's own bump, 0.1.0 to 0.2.0, is not named"
    );
}

#[test]
fn a_removal_names_nothing() {
    let repo = Repo::new("removal");
    repo.commit(
        &cargo_lock(
            "0.1.0",
            &[("kept", "1.0.0", REGISTRY), ("bumped", "1.0.0", REGISTRY)],
        ),
        &pnpm_lock(&[
            ("@scope/kept@1.0.0", "{integrity: sha512-k}"),
            ("@scope/bumped@1.0.0", "{integrity: sha512-b}"),
        ]),
    );
    let (out, named) = repo.check();
    assert!(out.status.success());
    assert_eq!(named, "");
}

#[test]
fn a_git_source_in_cargo_lock_fails() {
    let repo = Repo::new("git");
    repo.commit(
        &cargo_lock(
            "0.1.0",
            &[
                ("kept", "1.0.0", REGISTRY),
                ("bumped", "1.0.0", REGISTRY),
                ("gone", "1.0.0", REGISTRY),
                ("forked", "1.0.0", "git+https://example.com/forked#abc"),
            ],
        ),
        &pnpm_lock(&[
            ("@scope/kept@1.0.0", "{integrity: sha512-k}"),
            ("@scope/bumped@1.0.0", "{integrity: sha512-b}"),
            ("gone@1.0.0", "{integrity: sha512-g}"),
        ]),
    );
    let (out, named) = repo.check();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(named, "Cargo.lock: forked 1.0.0 added\n");
    assert!(String::from_utf8_lossy(&out.stderr)
        .contains("git source: forked 1.0.0 from git+https://example.com/forked#abc"));
}

#[test]
fn one_version_pointed_at_another_source_or_tarball_is_named() {
    let repo = Repo::new("swapped");
    repo.commit(
        &cargo_lock(
            "0.1.0",
            &[
                ("kept", "1.0.0", "registry+https://example.com/other"),
                ("bumped", "1.0.0", REGISTRY),
                ("gone", "1.0.0", REGISTRY),
            ],
        ),
        &pnpm_lock(&[
            (
                "@scope/kept@1.0.0",
                "{tarball: https://example.com/kept.tgz}",
            ),
            ("@scope/bumped@1.0.0", "{integrity: sha512-b}"),
            ("gone@1.0.0", "{integrity: sha512-g}"),
        ]),
    );
    let (out, named) = repo.check();
    assert!(out.status.success());
    assert_eq!(
        named.lines().collect::<Vec<_>>(),
        [
            "Cargo.lock: kept 1.0.0 changed its source or checksum",
            "web/pnpm-lock.yaml: @scope/kept 1.0.0 changed its source or checksum",
        ]
    );
}

/// A lockfile whose shape the reader does not know must not read as every
/// package removed, which would ask nothing.
#[test]
fn a_lockfile_it_cannot_read_fails() {
    let repo = Repo::new("unread");
    let pnpm = pnpm_lock(&[("new@2.0.0", "{integrity: sha512-n}")])
        .replace("  'new@2.0.0':", "      'new@2.0.0':");
    repo.commit(&cargo_lock("0.1.0", &[("kept", "1.0.0", REGISTRY)]), &pnpm);
    let (out, _) = repo.check();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr)
        .contains("web/pnpm-lock.yaml at HEAD holds no package this reads"));
}

#[test]
fn a_version_may_hold_an_at_sign() {
    let repo = Repo::new("at");
    repo.commit(
        &cargo_lock(
            "0.1.0",
            &[
                ("kept", "1.0.0", REGISTRY),
                ("bumped", "1.0.0", REGISTRY),
                ("gone", "1.0.0", REGISTRY),
            ],
        ),
        &pnpm_lock(&[
            ("@scope/kept@1.0.0", "{integrity: sha512-k}"),
            (
                "@scope/bumped@1.0.0(@peer/x@2.0.0)",
                "{integrity: sha512-b}",
            ),
            ("gone@1.0.0", "{integrity: sha512-g}"),
        ]),
    );
    let (out, named) = repo.check();
    assert!(out.status.success());
    assert_eq!(
        named,
        "web/pnpm-lock.yaml: @scope/bumped 1.0.0 -> 1.0.0(@peer/x@2.0.0)\n"
    );
}

/// The same package at the same version from the same registry, other bytes:
/// the checksum alone differs, then the integrity alone.
#[test]
fn another_checksum_or_integrity_alone_is_named() {
    let repo = Repo::new("bytes");
    repo.commit(
        &cargo_lock(
            "0.1.0",
            &[
                ("kept", "1.0.0", REGISTRY),
                ("bumped", "1.0.0", REGISTRY),
                ("gone", "1.0.0", REGISTRY),
            ],
        )
        .replacen("checksum = \"aa\"", "checksum = \"bb\"", 1),
        &pnpm_lock(&[
            ("@scope/kept@1.0.0", "{integrity: sha512-swapped}"),
            ("@scope/bumped@1.0.0", "{integrity: sha512-b}"),
            ("gone@1.0.0", "{integrity: sha512-g}"),
        ]),
    );
    let (out, named) = repo.check();
    assert!(out.status.success());
    assert_eq!(
        named.lines().collect::<Vec<_>>(),
        [
            "Cargo.lock: kept 1.0.0 changed its source or checksum",
            "web/pnpm-lock.yaml: @scope/kept 1.0.0 changed its source or checksum",
        ]
    );
}
