//! `datum alias install zsh|bash` (da-gc9dm): one line in the rc file, and
//! a new shell runs an alias by its bare name without shadowing a command.

use std::path::Path;
use std::process::Command;

/// Install, run an alias by name in a fresh interactive shell (which reads
/// the rc), then uninstall: for each shell that is installed here.
#[test]
fn an_installed_shell_runs_aliases_by_name() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let p = tmp.path();
    let bin = Path::new(env!("CARGO_BIN_EXE_datum")).parent().unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let with_env = |mut cmd: Command| {
        cmd.env("HOME", p)
            .env_remove("ZDOTDIR")
            .env("PATH", &path)
            .env("DATUM_DB", p.join("t.db"))
            .env("DATUM_HOME", p.join("data"))
            .env("DATUM_CONFIG_HOME", p.join("config"))
            .env("NO_COLOR", "1");
        cmd
    };
    let datum = |args: &[&str]| {
        let out = with_env(Command::new(env!("CARGO_BIN_EXE_datum")))
            .args(args)
            .output()
            .expect("spawn datum");
        assert!(out.status.success(), "datum {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    datum(&["alias", "set", "asleep", "start sleep"]);
    // An alias by a command's name: the command keeps it.
    datum(&["alias", "set", "find", "occur found"]);

    let mut ran = 0;
    for shell in ["zsh", "bash"] {
        let rc = p.join(format!(".{shell}rc"));
        let mine = "# mine\nexport X=1\n";
        std::fs::write(&rc, mine).unwrap();
        datum(&["alias", "install", shell]);
        assert!(datum(&["alias", "install", shell]).starts_with("already in"));
        let text = std::fs::read_to_string(&rc).unwrap();
        assert_eq!(text.lines().count(), 3, "{shell}: one line added: {text}");

        let mut sh = with_env(Command::new(shell));
        let Ok(out) = sh
            .args(["-ic", "asleep; type find; datum_no_such_word; true"])
            .output()
        else {
            eprintln!("{shell} not installed: skipped");
            continue;
        };
        ran += 1;
        let (stdout, stderr) = (
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(stdout.contains("CREATE"), "{shell}: {stdout} {stderr}");
        assert!(
            stdout.contains("find is /") && !stdout.contains("alias"),
            "{shell}: {stdout}"
        );
        assert!(
            stderr.contains("datum_no_such_word") && stderr.contains("not found"),
            "{shell}: {stderr}"
        );

        datum(&["alias", "uninstall", shell]);
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), mine, "{shell}");
    }
    // Each shell that ran ran `asleep` once, and nothing logged under
    // `found`.
    let tail = datum(&["tail", "-n", "20"]);
    assert_eq!(tail.matches("sleep").count(), ran, "{tail}");
    assert!(!tail.contains("found"), "{tail}");
}

/// `alias shell` leaves out a name no shell alias can take, and carries
/// `--db` into each alias it makes.
#[test]
fn the_snippet_carries_the_db_and_skips_unsayable_names() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let db = tmp.path().join("my db.db");
    let datum = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .arg("--db")
            .arg(&db)
            .args(args)
            .env("DATUM_HOME", tmp.path().join("data"))
            .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
            .output()
            .expect("spawn datum");
        assert!(out.status.success(), "datum {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    datum(&["alias", "set", "tea1", "occur tea"]);
    datum(&["alias", "set", "two words", "occur tea"]);
    let snippet = datum(&["alias", "shell", "bash"]);
    assert!(snippet.contains(" tea1;"), "{snippet}");
    assert!(!snippet.contains("two"), "{snippet}");
    assert!(
        snippet.contains(&format!("'datum --db '\\''{}'\\'' '", db.display())),
        "{snippet}"
    );
}
