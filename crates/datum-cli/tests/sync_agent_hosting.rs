//! The service unit's exec line must be a command `datum` actually runs.
//!
//! `datum sync install-daemon` renders `ExecStart="<datum>" sync agent`
//! (in `datum-commands`), and `datum-cli` is what routes `sync agent` to
//! the daemon runtime. Those two halves live in different crates, so
//! nothing but a test holds them together — and the failure mode when
//! they drift is exactly da-ge3e: a unit that starts something which
//! does no syncing, with the user left running `datum sync` by hand.
//!
//! So: render the unit, take the argv out of it, and run that argv
//! through the same `datum_cli::run` the binary's `main` calls.

use std::path::PathBuf;

use datum_commands::commands::sync::install_daemon::{render_unit, Platform};
use datum_test_util::{env_lock, ScopedEnv};

/// Pull the argv out of a rendered systemd `ExecStart=`, honouring the
/// quoting that lets a `datum` under a path with a space still start.
fn exec_argv(unit: &str) -> Vec<String> {
    let line = unit
        .lines()
        .find_map(|l| l.strip_prefix("ExecStart="))
        .expect("unit has an ExecStart");
    let (exec, rest) = match line.strip_prefix('"') {
        Some(quoted) => {
            let (path, rest) = quoted
                .split_once('"')
                .expect("unbalanced quote in ExecStart");
            (path.to_string(), rest)
        }
        None => {
            let (path, rest) = line.split_once(' ').unwrap_or((line, ""));
            (path.to_string(), rest)
        }
    };
    std::iter::once(exec)
        .chain(rest.split_whitespace().map(str::to_string))
        .collect()
}

#[test]
fn unit_exec_line_is_argv_the_cli_accepts() {
    let _guard = env_lock();
    let tmp = tempfile::tempdir().unwrap();
    // No config under here → the daemon enumerates zero targets, warns,
    // and returns. That is all this test needs: reaching the daemon at
    // all is the assertion.
    let _cfg = ScopedEnv::set("DATUM_CONFIG_HOME", tmp.path());
    let _home = ScopedEnv::set("HOME", tmp.path());

    let unit = render_unit(
        Platform::SystemdUser,
        &PathBuf::from("/usr/local/bin/datum"),
    );
    let mut argv = exec_argv(&unit);
    assert_eq!(
        &argv[1..],
        &["sync", "agent"],
        "the unit must exec the datum binary's own agent subcommand: {unit}"
    );

    // `--one-shot` so the daemon returns instead of running forever, and
    // an explicit pid-file so a real agent on this box isn't disturbed.
    argv.push("--one-shot".to_string());
    argv.push("--pid-file".to_string());
    argv.push(tmp.path().join("agent.pid").display().to_string());

    datum_cli::run(argv).expect("`datum sync agent` must reach the daemon runtime");
}

/// The daemon's flags survive the CLI's own arg parsing, rather than
/// being swallowed as sync's `--server`-style flags.
#[test]
fn agent_rejects_flags_it_does_not_own() {
    let err = datum_cli::run(
        ["datum", "sync", "agent", "--nope"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("nope"),
        "unexpected error: {err:?}"
    );
}
