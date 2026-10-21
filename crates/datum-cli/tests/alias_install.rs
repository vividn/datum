//! `datum alias install zsh|bash` (da-gc9dm): a loop for the rc file, and
//! a new shell runs an alias by its bare name without shadowing a command.
//! The loop is printed and the person adds it, and it reads only names
//! from datum, `alias list --names` (da-a0l6s).

use std::path::Path;
use std::process::Command;

/// `install` prints the loop on stdout, exactly, and writes no file: so
/// `datum alias install zsh >> ~/.zshrc` is right as written.
#[test]
fn install_prints_the_loop_and_writes_nothing() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    for (db, datum) in [(None, "datum"), (Some("my db"), "datum --db 'my db'")] {
        for shell in ["zsh", "bash"] {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
            if let Some(db) = db {
                cmd.args(["--db", db]);
            }
            let out = cmd
                .args(["alias", "install", shell])
                .env("HOME", &home)
                .env_remove("ZDOTDIR")
                .env("DATUM_HOME", tmp.path().join("data"))
                .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
                .output()
                .expect("spawn datum");
            assert!(out.status.success(), "{shell}: {out:?}");
            assert_eq!(
                String::from_utf8(out.stdout).unwrap(),
                format!(
                    "for __datum_a in $({datum} alias list --names 2>/dev/null) occur start stop switch; do\n  \
                     command -v \"$__datum_a\" >/dev/null 2>&1 || alias \"$__datum_a={datum} $__datum_a\"\n\
                     done\n\
                     unset __datum_a\n"
                ),
            );
            let stderr = String::from_utf8(out.stderr).unwrap();
            assert!(stderr.contains(&format!("/.{shell}rc`")), "{stderr}");
        }
    }
    // A --db name the loop's quotes cannot hold is refused, a newline too:
    // in the alias's text it would end the command.
    for name in ["a\nb", "a$b"] {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(["--db", name, "alias", "install", "zsh"])
            .env("HOME", &home)
            .output()
            .expect("spawn datum");
        assert!(!out.status.success() && out.stdout.is_empty(), "{out:?}");
    }
    let left: Vec<_> = std::fs::read_dir(&home).unwrap().collect();
    assert!(left.is_empty(), "install wrote {left:?}");
}

/// Install as the README says (`>> ~/.zshrc`), run an alias by name in a
/// fresh interactive shell (which reads the rc), then uninstall: for each
/// shell that is installed here.
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
        let block = datum(&["alias", "install", shell]);
        // Added twice by mistake: `uninstall` takes out both.
        std::fs::write(&rc, format!("{mine}{block}{block}")).unwrap();

        let mut sh = with_env(Command::new(shell));
        let Ok(out) = sh
            .args([
                "-ic",
                "asleep; occur coffee; type find; echo \"[${__datum_a-unset}]\"; datum_no_such_word; true",
            ])
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
        assert!(stdout.contains("[unset]"), "{shell}: {stdout}");
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
    // Each shell that ran ran `asleep` and the verb `occur` once, and
    // nothing logged under `found`.
    let tail = datum(&["tail", "-n", "20"]);
    assert_eq!(tail.matches("sleep").count(), ran, "{tail}");
    assert_eq!(tail.matches("coffee").count(), ran, "{tail}");
    assert!(!tail.contains("found"), "{tail}");
}

/// `alias list --names` is one name a line, nothing else, and leaves out
/// a name no shell alias can take.
#[test]
fn list_names_leaves_out_unsayable_names() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let datum = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .arg("--db")
            .arg(tmp.path().join("t.db"))
            .args(args)
            .env("DATUM_HOME", tmp.path().join("data"))
            .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
            .output()
            .expect("spawn datum")
    };
    for name in ["tea1", "two words", "x;touch pwned", "chai"] {
        assert!(datum(&["alias", "set", name, "occur tea"]).status.success());
    }
    let out = datum(&["alias", "list", "--names"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "chai\ntea1\n");
    // A typed `--show` it has no rendering for is refused, not ignored.
    let out = datum(&["alias", "list", "--names", "--show", "json"]);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        !out.status.success() && stderr.contains("alias list --names: --show json"),
        "{stderr}"
    );
}

/// `alias shell`, which printed code for a shell to eval, is gone.
#[test]
fn alias_shell_is_gone() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .arg("--db")
        .arg(tmp.path().join("t.db"))
        .args(["alias", "shell", "zsh"])
        .output()
        .expect("spawn datum");
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("unknown alias subcommand: 'shell'"),
        "{stderr}"
    );
}

/// A shell that starts before there is a database says nothing of it: the
/// loop's `alias list --names` is a read, and refuses a missing database.
#[test]
fn a_shell_before_the_first_write_starts_quietly() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let p = tmp.path();
    let bin = Path::new(env!("CARGO_BIN_EXE_datum")).parent().unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    for shell in ["zsh", "bash"] {
        let block = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(["alias", "install", shell])
            .env("HOME", p)
            .output()
            .expect("spawn datum")
            .stdout;
        std::fs::write(p.join(format!(".{shell}rc")), block).unwrap();
        let Ok(out) = Command::new(shell)
            .args(["-ic", "true"])
            .env("HOME", p)
            .env_remove("ZDOTDIR")
            .env("PATH", &path)
            .env("DATUM_DB", p.join("none.db"))
            .env("DATUM_HOME", p.join("data"))
            .env("DATUM_CONFIG_HOME", p.join("config"))
            .output()
        else {
            eprintln!("{shell} not installed: skipped");
            continue;
        };
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("database"), "{shell}: {stderr}");
        assert!(!p.join("none.db").exists(), "{shell}");
    }
}
