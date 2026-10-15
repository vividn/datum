//! `--colour` is `--color` (da-fdu1l): the British spelling of any color
//! flag is read as the American one, and nothing datum prints says it —
//! not help, not completion, not a refusal.

use std::process::{Command, Output};

use tempfile::TempDir;

fn datum(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("c.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR")
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let out = datum(dir, args);
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// Each line run with `--color` on one database and `--colour` on another
/// leaves the two specs alike, and `tail` is painted as `--color` asks.
#[test]
fn colour_is_color_on_track_spec_set_and_tail() {
    let lines: &[&[&str]] = &[
        &[
            "track", "mood", "--verb", "switch", "--state", "happy", "--color", "red",
        ],
        &["spec", "set", "mood2", "--verb", "switch", "--color=blue"],
        &["spec", "set", "mood", "--state-color", "happy=green"],
        &["spec", "set", "mood2", "--state-color=calm=pink"],
        &["switch", "mood", "happy"],
    ];
    let (us, uk) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for line in lines {
        ok(&us, line);
        let british: Vec<String> = line.iter().map(|w| w.replace("color", "colour")).collect();
        ok(&uk, &british.iter().map(String::as_str).collect::<Vec<_>>());
    }
    for field in ["mood", "mood2"] {
        let show = |dir| ok(dir, &["spec", "show", field]);
        assert_eq!(show(&us), show(&uk));
    }
    let mood = ok(&uk, &["spec", "show", "mood"]);
    assert!(
        mood.contains("\"color\": \"#") && mood.contains("\"happy\": \"#"),
        "{mood}"
    );

    for (args, painted) in [
        (&["--colour", "always", "tail"][..], true),
        (&["--colour=always", "tail"], true),
        (&["--colour=never", "tail"], false),
    ] {
        let out = ok(&uk, args);
        assert_eq!(out.contains('\x1b'), painted, "{args:?}: {out:?}");
    }
}

/// A refusal names the flag `--color`, and completion offers only it.
#[test]
fn only_color_is_ever_shown() {
    let dir = tempfile::tempdir().unwrap();
    let out = datum(
        &dir,
        &["track", "mood", "--verb", "switch", "--colour", "beige"],
    );
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("--color") && !err.contains("colour"), "{err}");
    // `tail` takes it only in front of the command, and refuses either
    // spelling after it in the same words.
    let refusal = |flag| datum(&dir, &["tail", flag, "always"]).stderr;
    assert_eq!(refusal("--colour"), refusal("--color"));

    for line in ["spec set mood --", "track mood --", "tail --"] {
        let mut words: Vec<&str> = line.split(' ').collect();
        words.insert(0, "datum");
        let cword = (words.len() - 1).to_string();
        let mut args = vec!["__complete", &cword, "--"];
        args.extend(&words);
        let found = ok(&dir, &args);
        assert!(!found.contains("colour"), "{line}: {found}");
    }
    let found = ok(
        &dir,
        &["__complete", "3", "--", "datum", "spec", "set", "--"],
    );
    assert!(found.lines().any(|l| l == "--color"), "{found}");
}
