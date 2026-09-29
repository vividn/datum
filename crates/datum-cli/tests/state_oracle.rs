//! da-u5l1o: one oracle decides a field's state at an instant, so the
//! `!` repeat mark, `datum check`'s `repeated_state` and `--if-changed`
//! cannot disagree. vividn's report: after a timed switch with no
//! `following_state` ended, the field was back in the state it had
//! interrupted — but the render and the check still read it as the
//! block's state, so switching to that state again printed `!` and warned.
//! Subprocess, so the printed action is the real one.

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    /// Run and return stdout, asserting exit 0.
    fn ok(&self, args: &[&str]) -> String {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.tmp.path().join("t.db"))
            .env("DATUM_HOME", self.tmp.path())
            .env("DATUM_CONFIG_HOME", self.tmp.path())
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(
            out.status.code(),
            Some(0),
            "`datum {}`: stdout={stdout} stderr={}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        stdout
    }
}

/// state1, then a five-minute block of state2 that ended four minutes
/// ago, then state2 again: state1 was back in effect, so it is a change.
fn nates_history(env: &Env) {
    env.ok(&["switch", "field", "state1", "-qqq"]);
    env.ok(&["switch", "field", "state2", "-q", "5"]);
}

#[test]
fn a_switch_after_a_finished_block_is_not_a_repeat() {
    let env = Env::new();
    nates_history(&env);
    let out = env.ok(&["switch", "field", "state2", "-t", "-1"]);
    assert!(out.contains("CREATE"), "{out}");
    assert!(!out.contains("!state2"), "state1 had resumed:\n{out}");
    let check = env.ok(&["check"]);
    assert!(!check.contains("repeated_state"), "{check}");
}

#[test]
fn if_changed_writes_it_and_marks_no_repeat() {
    let env = Env::new();
    nates_history(&env);
    let out = env.ok(&["switch", "field", "state2", "-t", "-1", "--if-changed"]);
    assert!(
        out.contains("CREATE"),
        "the state was state1, so it writes:\n{out}"
    );
    assert!(
        !out.contains("!state2"),
        "and what it wrote is no repeat:\n{out}"
    );
}

/// vividn, 2026-09-29: «an occur should not modify the state». So an occur
/// inside `drinking` leaves it `drinking`, and switching to it again after
/// the occur is a repeat.
#[test]
fn an_occur_leaves_the_state_so_a_switch_back_is_a_repeat() {
    let env = Env::new();
    env.ok(&["switch", "coffee", "drinking", "-qqq"]);
    env.ok(&["occur", "coffee", "-qq"]);
    let out = env.ok(&["switch", "coffee", "drinking", "-q"]);
    assert!(out.contains("!drinking"), "{out}");
    let check = env.ok(&["check"]);
    assert!(check.contains("repeated_state"), "{check}");
}

#[test]
fn a_genuine_repeat_is_still_marked_and_flagged() {
    let env = Env::new();
    env.ok(&["switch", "field", "state2", "-qq"]);
    let out = env.ok(&["switch", "field", "state2", "-q"]);
    assert!(out.contains("!state2"), "{out}");
    let check = env.ok(&["check"]);
    assert!(check.contains("repeated_state"), "{check}");
}
