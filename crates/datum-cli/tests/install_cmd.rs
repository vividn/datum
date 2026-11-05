//! `datum install | uninstall | upgrade` and `datum check`'s install section
//! (da-uphgn), in a throwaway home: the binary is a copy of its own,
//! `systemctl` is faked on PATH, and the release is a directory read by
//! `file://`, its SHA256SUMS signed by a throwaway key the test build
//! trusts in place of the release key. Linux only: the daemon's unit is
//! systemd's.
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use blake2::Blake2b512;
use data_encoding::BASE64;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::sync_net::claim_port;

/// The key the test build trusts, and one it does not.
const TEST_KEY: [u8; 32] = [7; 32];
const OTHER_KEY: [u8; 32] = [8; 32];

/// `seed`'s key as minisign writes it: `Ed`, its id (here the public key's
/// first 8 bytes), the public key.
fn minisign_key(seed: &[u8; 32]) -> String {
    let public = SigningKey::from_bytes(seed).verifying_key().to_bytes();
    BASE64.encode(&[b"Ed", &public[..8], &public[..]].concat())
}

/// `msg` signed by `seed` as `minisign -S` signs: `ED`, Ed25519 over its
/// BLAKE2b-512, and a global signature over that and the trusted comment.
fn minisign(seed: &[u8; 32], msg: &[u8]) -> String {
    let key = SigningKey::from_bytes(seed);
    let public = key.verifying_key().to_bytes();
    let sig = key.sign(&Blake2b512::digest(msg)).to_bytes();
    let comment = "datum test release";
    let global = key
        .sign(&[&sig[..], comment.as_bytes()].concat())
        .to_bytes();
    format!(
        "untrusted comment: test\n{}\ntrusted comment: {comment}\n{}\n",
        BASE64.encode(&[b"ED", &public[..8], &sig[..]].concat()),
        BASE64.encode(&global)
    )
}

struct Home {
    tmp: TempDir,
    port: u16,
    _claim: std::net::TcpListener,
}

impl Home {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let fake = tmp.path().join("fake");
        std::fs::create_dir(&fake).unwrap();
        let log = tmp.path().join("ctl.log").display().to_string();
        for (name, body) in [
            ("systemctl", format!("echo \"systemctl $*\" >> {log}")),
            (
                "cargo",
                format!(
                    "echo \"cargo $*\" >> {log}; rm \"$3/bin/datum\"; echo '[v1]' > \"$3/.crates.toml\""
                ),
            ),
        ] {
            let path = fake.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let exe = tmp.path().join("bin/datum");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_datum"), &exe).unwrap();
        let (port, _claim) = claim_port();
        let home = Self { tmp, port, _claim };
        home.ok(&["occur", "coffee"]);
        home
    }

    fn path(&self, p: &str) -> PathBuf {
        self.tmp.path().join(p)
    }

    fn exe(&self) -> PathBuf {
        self.path("bin/datum")
    }

    fn completion(&self) -> PathBuf {
        self.path("xdgdata/bash-completion/completions/datum")
    }

    fn service(&self) -> PathBuf {
        self.path("config/systemd/user/datum-sync.service")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.path("ctl.log")).unwrap_or_default()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let path = format!(
            "{}:{}",
            self.path("fake").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(self.exe())
            .args(args)
            .env("PATH", path)
            .env("HOME", self.path("home"))
            .env("SHELL", "/bin/bash")
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("xdgdata"))
            .env("DATUM_HOME", self.path("data"))
            .env("DATUM_CONFIG_HOME", self.path("dconf"))
            .env("DATUM_DB", self.path("data/data.db"))
            .env(
                "DATUM_RELEASE_URL",
                format!("file://{}", self.path("release").display()),
            )
            .env("DATUM_TEST_RELEASE_KEY", minisign_key(&TEST_KEY))
            .env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", self.port))
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .env("NO_COLOR", "1")
            .env_remove("DATUM_CONFIG")
            .env_remove("ZDOTDIR")
            .envs(env.iter().copied())
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        self.ok_with(args, &[])
    }

    fn ok_with(&self, args: &[&str], env: &[(&str, &str)]) -> String {
        let out = self.run_with(args, env);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    /// Makes this datum one cargo installed: its root's `.crates.toml`
    /// lists it.
    fn cargo_built(&self) {
        let listed =
            "[v1]\n\"datum-cli 3.0.0 (path+file:///checkout/crates/datum-cli)\" = [\"datum\"]\n";
        std::fs::write(self.path(".crates.toml"), listed).unwrap();
    }

    fn err(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stderr).unwrap()
    }

    /// The install section's categories, or `None` when it did not run.
    fn install_findings(&self) -> Option<Vec<String>> {
        let report: serde_json::Value =
            serde_json::from_str(&self.ok(&["check", "--json"])).unwrap();
        let ran = report["examined"]["checkers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "install");
        ran.then(|| {
            report["issues"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["domain"] == "install")
                .map(|i| i["category"].as_str().unwrap().to_string())
                .collect()
        })
    }

    /// A release holding `binary` as this platform's build, with `sum` its
    /// line in SHA256SUMS (`None`: its real one), signed by the test key.
    /// The fixed name a datum from before build names reads (da-mzhau) is
    /// listed first, with a sum nothing matches, so an upgrade that read it
    /// would refuse.
    fn release(&self, binary: &[u8], sum: Option<&str>) {
        self.release_in("release", binary, sum);
        self.sign(&TEST_KEY);
    }

    /// The same, as the release in `dir` beside the latest, unsigned: a
    /// kept one is `v<version>` (da-ec8y8).
    fn release_in(&self, dir: &str, binary: &[u8], sum: Option<&str>) {
        let dir = self.path(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixed = format!("datum-{}-{}", std::env::consts::ARCH, std::env::consts::OS);
        let real = format!("{:x}", Sha256::digest(binary));
        let asset = format!("{fixed}-{}", &real[..7]);
        std::fs::write(dir.join(&asset), binary).unwrap();
        let sum = sum.unwrap_or(&real);
        let zero = "0".repeat(64);
        std::fs::write(
            dir.join("SHA256SUMS"),
            format!("{zero}  {fixed}\n{sum}  {asset}\n"),
        )
        .unwrap();
    }

    /// Signs the release's SHA256SUMS with `seed`'s key.
    fn sign(&self, seed: &[u8; 32]) {
        self.sign_in("release", seed);
    }

    fn sign_in(&self, dir: &str, seed: &[u8; 32]) {
        let sums = std::fs::read(self.path(&format!("{dir}/SHA256SUMS"))).unwrap();
        std::fs::write(self.sig_in(dir), minisign(seed, &sums)).unwrap();
    }

    /// The signature an upgrade reads: the build's, `SHA256SUMS-<sha7>.minisig`,
    /// where SHA256SUMS names one, and `SHA256SUMS.minisig` where it does not.
    fn sig(&self) -> PathBuf {
        self.sig_in("release")
    }

    fn sig_in(&self, dir: &str) -> PathBuf {
        let sums = std::fs::read_to_string(self.path(&format!("{dir}/SHA256SUMS"))).unwrap();
        let fixed = format!("datum-{}-{}-", std::env::consts::ARCH, std::env::consts::OS);
        let name = sums
            .split_whitespace()
            .find_map(|w| w.strip_prefix(&fixed))
            .map_or("SHA256SUMS.minisig".into(), |b| {
                format!("SHA256SUMS-{b}.minisig")
            });
        self.path(&format!("{dir}/{name}"))
    }
}

/// `upgrade --to` installs the kept release beside the latest, `v<version>`
/// (da-ec8y8), verified as the latest is, and only a binary that says it is
/// that version.
#[test]
fn upgrade_to_installs_a_kept_release_verified() {
    let h = Home::new();
    h.ok(&["install", "--no-completions"]);
    let before = std::fs::read(h.exe()).unwrap();
    h.release(b"#!/bin/sh\necho datum 9.9.9\n", None);
    let unchanged = |said: String, why: &str| {
        assert!(said.contains(why), "{said}");
        assert_eq!(std::fs::read(h.exe()).unwrap(), before);
        assert!(!h.path("bin/.datum-upgrade").exists());
    };

    unchanged(
        h.err(&["upgrade", "--to", "3.0.5"]),
        "datum 3.0.5 has no kept release to install",
    );

    h.release_in("v3.0.5", &before, None);
    h.sign_in("v3.0.5", &TEST_KEY);
    assert!(h
        .ok(&["upgrade", "--to", "3.0.5"])
        .contains("datum is 3.0.5 already"));

    let kept = b"#!/bin/sh\necho 'datum 3.0.5-beta (abcdef1)'\n";
    h.release_in("v3.0.5", kept, None);
    h.sign_in("v3.0.5", &OTHER_KEY);
    unchanged(
        h.err(&["upgrade", "--to", "3.0.5"]),
        "does not verify with the key this datum was built with",
    );

    h.release_in(
        "v3.0.5",
        b"#!/bin/sh\necho 'datum 3.0.6-beta (abcdef1)'\n",
        None,
    );
    h.sign_in("v3.0.5", &TEST_KEY);
    unchanged(
        h.err(&["upgrade", "--to", "3.0.5"]),
        "says it is datum 3.0.6-beta (abcdef1), not datum 3.0.5",
    );

    h.release_in("v3.0.5", kept, None);
    h.sign_in("v3.0.5", &TEST_KEY);
    let said = h.ok(&["upgrade", "--check", "--to", "v3.0.5"]);
    assert!(
        said.contains("`datum upgrade --to 3.0.5` installs it"),
        "{said}"
    );
    assert_eq!(std::fs::read(h.exe()).unwrap(), before);
    let said = h.ok(&["upgrade", "--to", "3.0.5"]);
    assert!(said.contains("to datum 3.0.5-beta (abcdef1)"), "{said}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), kept);
}

#[test]
fn install_then_uninstall_keeps_the_record() {
    let h = Home::new();
    let said = h.ok(&["install"]);
    assert!(h.completion().exists(), "{said}");
    assert!(!h.service().exists(), "no database syncs: {said}");
    assert!(
        said.contains("datum alias install bash >> ~/.bashrc"),
        "{said}"
    );
    assert!(h.path("data/installed.toml").exists());

    h.ok(&["install", "--daemon"]);
    let unit = std::fs::read_to_string(h.service()).unwrap();
    assert!(
        unit.contains(&format!("ExecStart=\"{}\"", h.exe().display())),
        "{unit}"
    );

    // Every alias loop goes: another database's, and the first build's one
    // line (6627637cf) too.
    let rc = h.path("home/.zshrc");
    let one_line = "for __datum_a in asleep find; do command -v \"$__datum_a\" >/dev/null 2>&1 || \
         alias \"$__datum_a=\"'datum '\"$__datum_a\"; done; unset __datum_a\n";
    let loops = h.ok(&["alias", "install", "zsh"])
        + one_line
        + &h.ok(&["--db", "other", "alias", "install", "zsh"]);
    std::fs::create_dir_all(rc.parent().unwrap()).unwrap();
    // The last loop's newline trimmed, as an editor may.
    std::fs::write(&rc, format!("export KEPT=1\n{}", loops.trim_end())).unwrap();

    let said = h.ok(&["uninstall"]);
    assert_eq!(std::fs::read_to_string(&rc).unwrap(), "export KEPT=1\n");
    assert!(!h.completion().exists(), "{said}");
    assert!(!h.service().exists(), "{said}");
    assert!(!h.exe().exists(), "{said}");
    assert!(!h.path("data/installed.toml").exists(), "{said}");
    assert!(h.path("data/data.db").exists(), "{said}");
    assert!(
        said.contains(&format!("data    {}", h.path("data").display())),
        "{said}"
    );
}

/// A datum a package manager put in place is its to remove.
#[test]
fn uninstall_leaves_a_binary_install_did_not_set_up() {
    let h = Home::new();
    // And another binary's record is that binary's: it stays.
    let record = h.path("data/installed.toml");
    std::fs::write(&record, "exe = \"/opt/other/datum\"\n").unwrap();
    let said = h.ok(&["uninstall"]);
    assert!(h.exe().exists(), "{said}");
    assert!(said.contains("did not set it up"), "{said}");
    assert!(record.exists(), "{said}");
}

/// The daemon comes with `datum install` once a database syncs, unless
/// DATUM_SYNC_NO_DAEMON, and installing it again restarts it onto the
/// binary, as install.sh used to.
#[test]
fn install_sets_up_the_daemon_when_a_database_syncs() {
    let h = Home::new();
    h.run(&["sync", "chain", "invite", "--expires", "1s"]);
    h.ok(&["install", "--no-completions"]);
    assert!(!h.service().exists(), "opted out");
    h.ok_with(
        &["install", "--no-completions"],
        &[("DATUM_SYNC_NO_DAEMON", "0")],
    );
    assert!(h.service().exists());
    assert!(!h.completion().exists());
    assert!(
        h.log()
            .contains("systemctl --user restart datum-sync.service"),
        "{}",
        h.log()
    );
}

#[test]
fn check_says_what_install_set_up_and_is_gone() {
    let h = Home::new();
    assert_eq!(h.install_findings(), None, "not installed: no section");
    h.ok(&["install", "--daemon"]);
    assert_eq!(h.install_findings(), Some(vec![]));

    std::fs::remove_file(h.completion()).unwrap();
    let unit = std::fs::read_to_string(h.service()).unwrap();
    let other = unit.replace(&h.exe().display().to_string(), "/opt/old/datum");
    std::fs::write(h.service(), other).unwrap();
    assert_eq!(
        h.install_findings(),
        Some(vec![
            "install_completion_missing".to_string(),
            "install_daemon_binary".to_string()
        ])
    );

    // Opted out of completion, none is missed.
    h.ok(&["install", "--no-completions"]);
    assert_eq!(h.install_findings(), Some(vec![]));

    // The shell it was installed for is the one checked, not $SHELL's.
    h.ok(&["install", "--shell", "fish"]);
    assert_eq!(h.install_findings(), Some(vec![]));
    std::fs::remove_file(h.path("config/fish/completions/datum.fish")).unwrap();
    assert_eq!(
        h.install_findings(),
        Some(vec!["install_completion_missing".to_string()])
    );
}

#[test]
fn update_checks_are_opt_in() {
    let h = Home::new();
    h.ok(&["install", "--no-completions"]);
    h.release(b"#!/bin/sh\necho datum 9.9.9\n", None);
    assert_eq!(h.install_findings(), Some(vec![]), "off by default");
    std::fs::create_dir_all(h.path("dconf")).unwrap();
    std::fs::write(h.path("dconf/datumrc.toml"), "update_check = true\n").unwrap();
    assert_eq!(
        h.install_findings(),
        Some(vec!["install_update".to_string()])
    );
    h.release(&std::fs::read(h.exe()).unwrap(), None);
    assert_eq!(h.install_findings(), Some(vec![]), "up to date");
}

#[test]
fn upgrade_refuses_a_mismatch_and_swaps_in_a_good_release() {
    let h = Home::new();
    let stub = b"#!/bin/sh\necho datum 9.9.9\n";
    h.release(stub, None);
    let refused = h.err(&["upgrade"]);
    assert!(
        refused.contains("not set up by `datum install`"),
        "{refused}"
    );

    h.ok(&["install", "--daemon"]);
    let before = std::fs::read(h.exe()).unwrap();
    h.release(&before, None);
    assert!(h.ok(&["upgrade"]).contains("up to date"));

    h.release(stub, Some(&"0".repeat(64)));
    let refused = h.err(&["upgrade"]);
    assert!(refused.contains("does not match SHA256SUMS"), "{refused}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), before);
    assert!(!h.path("bin/.datum-upgrade").exists());

    h.release(b"not a program", None);
    let refused = h.err(&["upgrade"]);
    assert!(refused.contains("does not run here"), "{refused}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), before);
    assert!(!h.path("bin/.datum-upgrade").exists());

    h.release(stub, None);
    assert!(h.ok(&["upgrade", "--check"]).contains("a newer datum"));
    assert_eq!(std::fs::read(h.exe()).unwrap(), before);

    let said = h.ok(&["upgrade"]);
    assert!(said.contains("datum 9.9.9"), "{said}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), stub);
    assert_eq!(
        h.log().matches("restart datum-sync.service").count(),
        2,
        "install, then upgrade: {}",
        h.log()
    );
}

/// `upgrade --to <major>.<minor>` is that minor's newest build (da-ec8y8):
/// its kept `v<major>.<minor>` where one is, else the latest where the latest
/// is of it, and nothing otherwise. `status --to` downloads it to say so.
#[test]
fn upgrade_to_a_minor_installs_its_newest_build() {
    let h = Home::new();
    h.ok(&["install", "--no-completions"]);
    let before = std::fs::read(h.exe()).unwrap();
    let unchanged = |said: String, why: &str| {
        assert!(said.contains(why), "{said}");
        assert_eq!(std::fs::read(h.exe()).unwrap(), before);
        assert!(!h.path("bin/.datum-upgrade").exists());
    };
    // The latest is of 3.5, a minor no release is kept for yet.
    h.release(b"#!/bin/sh\necho 'datum 3.5.40-beta (abcdef1)'\n", None);

    unchanged(
        h.err(&["upgrade", "--to", "3.6"]),
        "says it is datum 3.5.40-beta (abcdef1), not datum 3.6",
    );
    unchanged(
        h.err(&["upgrade", "status", "--to", "3.6"]),
        "not datum 3.6",
    );
    unchanged(
        h.ok(&["upgrade", "status", "--to", "3.5"]),
        "datum 3.5.40-beta (abcdef1) is released: `datum upgrade --to 3.5` installs it",
    );

    let kept = b"#!/bin/sh\necho 'datum 3.4.30-beta (abcdef1)'\n";
    h.release_in("v3.4", kept, None);
    h.sign_in("v3.4", &TEST_KEY);
    unchanged(
        h.ok(&["upgrade", "status", "--to", "3.4"]),
        "datum 3.4.30-beta (abcdef1) is released",
    );
    let said = h.ok(&["upgrade", "--to", "v3.4"]);
    assert!(said.contains("to datum 3.4.30-beta (abcdef1)"), "{said}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), kept);
}

/// A datum cargo installed uninstalls by cargo, so cargo's own record of
/// it goes too.
#[test]
fn a_cargo_install_uninstalls_by_cargo() {
    let h = Home::new();
    h.cargo_built();
    h.ok(&["install", "--no-completions"]);
    h.ok(&["uninstall"]);
    assert!(!h.exe().exists());
    let cargo = format!(
        "cargo uninstall --root {} datum-cli",
        h.tmp.path().display()
    );
    assert!(h.log().contains(&cargo), "{}", h.log());
}

/// A datum cargo built from a checkout upgrades onto the release, verified
/// with the key its build carries, saying it leaves the checkout's builds
/// (da-1yc69): cargo uninstalls it, the release takes its place, and it is
/// recorded as `datum install` records, so the next upgrade is ordinary.
/// One `datum install` never set up moves too.
#[test]
fn a_cargo_install_moves_onto_the_release() {
    let h = Home::new();
    h.cargo_built();
    let before = std::fs::read(h.exe()).unwrap();
    // A real datum, so the release runs: an ELF with bytes after it.
    let release = |tail: &[u8]| [&before[..], tail].concat();
    let warned = |out: &Output| {
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(said.contains("built by cargo from a checkout"), "{said}");
        assert!(said.contains("scripts/install.sh"), "{said}");
    };
    let unchanged = || {
        assert_eq!(std::fs::read(h.exe()).unwrap(), before);
        assert!(!h.path("bin/.datum-upgrade").exists());
        assert_eq!(h.log(), "", "cargo uninstalled nothing");
        assert!(!h.path("data/installed.toml").exists());
    };

    h.release(&release(b"one"), None);
    h.sign(&OTHER_KEY);
    let out = h.run(&["upgrade"]);
    assert!(!out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("does not verify with the key this datum was built with"),
        "{said}"
    );
    unchanged();

    h.sign(&TEST_KEY);
    let out = h.run(&["upgrade", "--check"]);
    assert!(out.status.success(), "{out:?}");
    warned(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("a newer datum"));
    unchanged();

    let out = h.run(&["upgrade"]);
    assert!(out.status.success(), "{out:?}");
    warned(&out);
    assert_eq!(std::fs::read(h.exe()).unwrap(), release(b"one"));
    let cargo = format!(
        "cargo uninstall --root {} datum-cli",
        h.tmp.path().display()
    );
    assert_eq!(h.log().matches(&cargo).count(), 1, "{}", h.log());
    let record = std::fs::read_to_string(h.path("data/installed.toml")).unwrap();
    assert!(record.contains(&h.exe().display().to_string()), "{record}");

    h.release(&release(b"two"), None);
    let out = h.run(&["upgrade"]);
    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        !said.contains("built by cargo"),
        "an ordinary upgrade: {said}"
    );
    assert_eq!(std::fs::read(h.exe()).unwrap(), release(b"two"));
    assert_eq!(h.log().matches(&cargo).count(), 1, "{}", h.log());
}

/// Once cargo has uninstalled the checkout's datum, the release is the only
/// one left: a rename that then fails keeps it and says how to finish.
#[test]
fn a_failed_swap_after_cargo_uninstall_keeps_the_release() {
    let h = Home::new();
    h.cargo_built();
    // An uninstall that leaves a directory where the binary was, which no
    // file can be renamed onto.
    std::fs::write(
        h.path("fake/cargo"),
        "#!/bin/sh\nrm \"$3/bin/datum\"; mkdir -p \"$3/bin/datum/in-the-way\"\n",
    )
    .unwrap();
    let release = [&std::fs::read(h.exe()).unwrap()[..], b"one"].concat();
    h.release(&release, None);
    let said = h.err(&["upgrade"]);
    let part = h.path("bin/.datum-upgrade");
    assert!(
        said.contains(&format!("mv '{}' '{}'", part.display(), h.exe().display())),
        "{said}"
    );
    assert_eq!(std::fs::read(&part).unwrap(), release);
}

/// A release from before build names lists only the fixed name, which an
/// upgrade reads where SHA256SUMS lists no build name (da-mzhau).
#[test]
fn upgrade_reads_a_release_from_before_build_names() {
    let h = Home::new();
    h.ok(&["install", "--no-completions"]);
    let stub = b"#!/bin/sh\necho datum 9.9.9\n";
    let fixed = format!("datum-{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    std::fs::create_dir_all(h.path("release")).unwrap();
    std::fs::write(h.path(&format!("release/{fixed}")), stub).unwrap();
    std::fs::write(
        h.path("release/SHA256SUMS"),
        format!("{:x}  {fixed}\n", Sha256::digest(stub)),
    )
    .unwrap();
    h.sign(&TEST_KEY);
    assert!(h.ok(&["upgrade"]).contains("datum 9.9.9"));
    assert_eq!(std::fs::read(h.exe()).unwrap(), stub);
}

/// An upgrade trusts no line of SHA256SUMS its signature by the key this
/// datum was built with does not cover (da-0b55k), and refuses, changing
/// nothing, when it is missing, by another key, or over other bytes.
#[test]
fn upgrade_refuses_sums_the_release_key_did_not_sign() {
    let h = Home::new();
    h.ok(&["install", "--no-completions"]);
    let before = std::fs::read(h.exe()).unwrap();
    let stub = b"#!/bin/sh\necho datum 9.9.9\n";
    let refused = |why: &str| {
        let said = h.err(&["upgrade"]);
        assert!(said.contains(why), "{said}");
        assert_eq!(std::fs::read(h.exe()).unwrap(), before);
        assert!(!h.path("bin/.datum-upgrade").exists());
    };

    h.release(stub, None);
    std::fs::remove_file(h.sig()).unwrap();
    refused("SHA256SUMS is not signed: the release has no SHA256SUMS-");

    h.sign(&OTHER_KEY);
    refused("does not verify with the key this datum was built with: The signature was created with a different key");

    // The binary swapped and SHA256SUMS rewritten to match, the old
    // signature kept.
    h.release(stub, None);
    let kept = std::fs::read(h.sig()).unwrap();
    h.release(b"#!/bin/sh\necho datum 6.6.6\n", None);
    std::fs::write(h.sig(), kept).unwrap();
    refused(
        "does not verify with the key this datum was built with: The signature verification failed",
    );

    h.release(stub, None);
    assert!(h.ok(&["upgrade"]).contains("datum 9.9.9"));
    assert_eq!(std::fs::read(h.exe()).unwrap(), stub);
}

impl Home {
    /// The daily upgrade's unit of `kind`, `timer` or `service`.
    fn auto(&self, kind: &str) -> PathBuf {
        self.path(&format!("config/systemd/user/datum-auto-upgrade.{kind}"))
    }

    /// The backup timer installed, if any.
    fn backup_timer(&self) -> Option<String> {
        let dir = std::fs::read_dir(self.path("config/systemd/user")).ok()?;
        (dir.flatten())
            .filter_map(|e| e.file_name().into_string().ok())
            .find(|n| n.starts_with("datum-backup-") && n.ends_with(".timer"))
    }

    /// `datum install` with stdin a terminal, on which `typed` is typed.
    fn install_on_a_terminal(&self, typed: &[u8]) -> Output {
        use std::io::Write;
        let (mut master, slave) = crate::ambiguous_time::pty();
        let path = format!(
            "{}:{}",
            self.path("fake").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let child = Command::new(self.exe())
            .args(["install", "--no-completions"])
            .env("PATH", path)
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("DATUM_HOME", self.path("data"))
            .env("DATUM_CONFIG_HOME", self.path("dconf"))
            .env("DATUM_DB", self.path("data/data.db"))
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .stdin(slave)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        master.write_all(typed).unwrap();
        child.wait_with_output().unwrap()
    }
}

/// `upgrade --auto` installs a daily timer running `datum upgrade` on this
/// binary, `--check` says whether it is on, and `--no-auto` and `uninstall`
/// remove it (da-f7ckm).
#[test]
fn upgrade_auto_installs_a_daily_timer_and_no_auto_removes_it() {
    let h = Home::new();
    h.ok(&["install", "--no-completions", "--no"]);
    assert!(!h.auto("timer").exists(), "--no answers no");
    h.release(&std::fs::read(h.exe()).unwrap(), None);
    let said = h.ok(&["upgrade", "--check"]);
    assert!(said.contains("daily upgrades: off"), "{said}");

    let said = h.ok(&["upgrade", "--auto"]);
    assert!(said.contains("datum-auto-upgrade.timer"), "{said}");
    let service = std::fs::read_to_string(h.auto("service")).unwrap();
    assert!(
        service.contains(&format!("\nExecStart=\"{}\" upgrade\n", h.exe().display())),
        "{service}"
    );
    assert!(
        service.contains(&format!(
            "Environment=\"DATUM_HOME={}\"",
            h.path("data").display()
        )),
        "{service}"
    );
    let timer = std::fs::read_to_string(h.auto("timer")).unwrap();
    assert!(timer.contains("\nOnCalendar=daily\n"), "{timer}");
    assert!(
        h.log()
            .contains("systemctl --user enable --now datum-auto-upgrade.timer"),
        "{}",
        h.log()
    );
    let said = h.ok(&["upgrade", "--check"]);
    assert!(said.contains("daily upgrades: on"), "{said}");
    assert!(said.contains("datum is up to date"), "{said}");
    // `status` is the one form, `--check` its other name (da-tsna7).
    assert_eq!(h.ok(&["upgrade", "status"]), said);

    let said = h.ok(&["upgrade", "--no-auto"]);
    assert!(said.contains("removed the systemd user timer"), "{said}");
    assert!(!h.auto("timer").exists() && !h.auto("service").exists());
    assert!(h
        .ok(&["upgrade", "--no-auto"])
        .contains("no daily upgrade timer installed"));

    h.ok(&["upgrade", "--auto"]);
    h.ok(&["uninstall"]);
    assert!(!h.auto("timer").exists() && !h.auto("service").exists());
}

/// `datum install` offers each `--auto` in turn (da-tsna7): backups of every
/// database every 30m, then the daily upgrade. A terminal is asked each,
/// yes the default; with none, backups are no and upgrades yes, unasked;
/// `--yes` / `--no` answer both.
#[test]
fn install_offers_backups_and_daily_upgrades() {
    let h = Home::new();
    let out = h.install_on_a_terminal(b"n\nn\n");
    assert!(out.status.success(), "{out:?}");
    let asked = String::from_utf8_lossy(&out.stderr);
    let backups = asked
        .find("Back up every database every 30m")
        .expect(&asked);
    let upgrades = asked.find("Upgrade datum daily").expect(&asked);
    assert!(backups < upgrades, "{asked}");
    assert!(!h.auto("timer").exists(), "answered no");
    assert!(h.backup_timer().is_none(), "answered no");

    let out = h.install_on_a_terminal(b"\n\n");
    assert!(out.status.success(), "{out:?}");
    assert!(h.auto("timer").exists(), "Enter takes yes");
    let timer = h.backup_timer().expect("Enter takes yes");
    assert!(timer.starts_with("datum-backup-all-"), "{timer}");
    // One there already is said, and not asked again.
    let out = h.install_on_a_terminal(b"\n");
    assert!(out.status.success(), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("Back up every"));

    h.ok(&["upgrade", "--no-auto"]);
    h.ok(&["backup", "--no-auto", "--all"]);
    h.ok(&["install", "--no-completions", "--no"]);
    assert!(!h.auto("timer").exists());
    assert!(h.backup_timer().is_none());
    h.ok(&["install", "--no-completions"]);
    assert!(h.auto("timer").exists(), "no terminal: yes");
    assert!(h.backup_timer().is_none(), "no terminal: no");
    h.ok(&["upgrade", "--no-auto"]);
    h.ok(&["install", "--no-completions", "--yes"]);
    assert!(h.auto("timer").exists());
    assert!(h.backup_timer().is_some());
}

/// A datum cargo built is refused the timer, saying why, and offered none.
#[test]
fn a_cargo_build_gets_no_daily_upgrade() {
    let h = Home::new();
    h.cargo_built();
    h.ok(&["install", "--no-completions", "--yes"]);
    assert!(!h.auto("timer").exists());
    let said = h.err(&["upgrade", "--auto"]);
    assert!(
        said.contains("built by cargo") && said.contains("`datum upgrade` moves it"),
        "{said}"
    );
    assert!(!h.auto("timer").exists());
}

/// A datum this user cannot replace, its directory not theirs to write (a
/// link to a binary root keeps current, say), is not upgraded, gets no
/// timer, and is offered none (da-f7ckm).
#[test]
fn a_datum_not_yours_to_replace_is_left_alone() {
    let h = Home::new();
    h.ok(&["install", "--no-completions", "--no"]);
    h.release(b"#!/bin/sh\necho datum 9.9.9\n", None);
    let bin = h.path("bin");
    let mode = |m| std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(m)).unwrap();
    mode(0o555);
    let upgrade = h.run(&["upgrade"]);
    let check = h.run(&["upgrade", "--check"]);
    let auto = h.run(&["upgrade", "--auto"]);
    let offered = h.run(&["install", "--no-completions", "--yes"]);
    mode(0o755);
    // root writes anywhere: there is nothing to refuse, so nothing to test.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("skipped: run as root, which may write any directory");
        return;
    }
    for out in [&upgrade, &check, &auto] {
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && said.contains("is not yours to replace"),
            "{out:?}"
        );
    }
    assert!(offered.status.success(), "{offered:?}");
    assert!(!h.auto("timer").exists());
    assert_ne!(
        std::fs::read(h.exe()).unwrap(),
        b"#!/bin/sh\necho datum 9.9.9\n"
    );
}
