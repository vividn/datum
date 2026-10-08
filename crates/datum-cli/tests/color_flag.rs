//! Colour by the conventions other tools read (da-r15mq): `--color
//! auto|always|never` in front of the command, then `NO_COLOR`, then
//! `CLICOLOR_FORCE` / `FORCE_COLOR`, then the terminal. Piped here, so the
//! terminal always says no.

use std::process::Command;

use tempfile::TempDir;

fn tail(dir: &TempDir, args: &[&str], vars: &[(&str, &str)]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("c.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR")
        .envs(vars.iter().copied())
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?} {vars:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// argv, environment, whether the output is coloured.
type Case = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
    bool,
);

#[test]
fn color_follows_flag_then_no_color_then_force_then_tty() {
    let dir = tempfile::tempdir().unwrap();
    tail(&dir, &["switch", "mood", "happy"], &[]);
    tail(&dir, &["alias", "set", "t", "tail"], &[]);
    let cases: &[Case] = &[
        (&["tail"], &[], false),
        (&["tail"], &[("CLICOLOR_FORCE", "1")], true),
        (&["tail"], &[("FORCE_COLOR", "1")], true),
        (&["tail"], &[("CLICOLOR_FORCE", "0")], false),
        (&["tail"], &[("FORCE_COLOR", "false")], false),
        (&["tail"], &[("NO_COLOR", "1"), ("FORCE_COLOR", "1")], false),
        (&["tail"], &[("NO_COLOR", ""), ("FORCE_COLOR", "1")], true),
        (&["--color", "always", "tail"], &[], true),
        (&["--color=always", "tail"], &[], true),
        (&["--color", "always", "tail"], &[("NO_COLOR", "1")], true),
        (
            &["--color", "never", "tail"],
            &[("FORCE_COLOR", "1")],
            false,
        ),
        (&["--color", "auto", "tail"], &[("FORCE_COLOR", "1")], true),
        (&["--color=always", "t"], &[], true),
        (&["--color", "always", "--db", "c", "tail"], &[], true),
        (&["--color", "always", "--", "tail"], &[], true),
    ];
    for (args, vars, coloured) in cases {
        let out = tail(&dir, args, vars);
        assert_eq!(
            out.contains('\x1b'),
            *coloured,
            "{args:?} {vars:?}: {out:?}"
        );
    }
    // Nothing after `--` is the global flag.
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("c.db"))
        .args(["--", "--color", "always", "tail"])
        .output()
        .unwrap();
    assert!(!out.stdout.contains(&0x1b), "{out:?}");
}

#[test]
fn color_takes_auto_always_or_never() {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .args(["--color", "yes", "tail"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        err.contains("--color takes auto, always or never, not 'yes'"),
        "{err}"
    );
}
