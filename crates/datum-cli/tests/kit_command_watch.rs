//! `chorelist -w` on a real process (da-3td5c): a write redraws, time passing
//! redraws with no write, and a wake that changed nothing prints nothing. And
//! what an installed older chores kit runs to take the newer one.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tempfile::TempDir;

const CHORES: &str = include_str!("../../../kits/chores.toml");
/// `chorelist` prints this when nothing is due or done today.
const NOTHING: &str = "COMPLETED!";

struct Env(TempDir);

impl Env {
    fn new() -> Self {
        let env = Self(tempfile::tempdir().unwrap());
        env.install(CHORES, &[]);
        env
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", self.0.path().join("data.db"))
            .env("DATUM_HOME", self.0.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.0.path().join("config"))
            .env("NO_COLOR", "1")
            .args(args);
        c
    }

    fn ok(&self, args: &[&str]) -> String {
        let out: Output = self.cmd(args).output().unwrap();
        assert!(
            out.status.success(),
            "datum {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn install(&self, kit: &str, flags: &[&str]) -> String {
        let path: PathBuf = self.0.path().join("kit.toml");
        std::fs::write(&path, kit).unwrap();
        let mut args = vec!["kit", "install", "--yes", path.to_str().unwrap()];
        args.extend(flags);
        self.ok(&args)
    }

    fn watch(&self, args: &[&str], tick_ms: &str) -> Watch {
        let mut child = self
            .cmd(args)
            .env("DATUM_TEST_WATCH_TICK_MS", tick_ms)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let seen = Arc::new(Mutex::new(String::new()));
        let into = seen.clone();
        std::thread::spawn(move || {
            let mut buf = [0; 4096];
            while let Ok(n @ 1..) = stdout.read(&mut buf) {
                into.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        });
        Watch { child, seen }
    }
}

struct Watch {
    child: Child,
    seen: Arc<Mutex<String>>,
}

impl Watch {
    fn seen(&self) -> String {
        self.seen.lock().unwrap().clone()
    }

    fn wait_for(&self, what: &str, within: Duration) {
        let start = Instant::now();
        while !self.seen().contains(what) {
            assert!(
                start.elapsed() < within,
                "no {what:?} within {within:?}: {:?}",
                self.seen()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_write_redraws_and_a_wake_that_changed_nothing_prints_nothing() {
    let env = Env::new();
    // The poll is 500 ms, so every poll wakes on the tick.
    let watch = env.watch(&["chorelist", "-w"], "1");
    watch.wait_for(NOTHING, Duration::from_secs(10));
    std::thread::sleep(Duration::from_millis(1600));
    assert_eq!(watch.seen().matches(NOTHING).count(), 1, "{}", watch.seen());

    env.ok(&["occur", "vacuum", "next_date=+7"]);
    watch.wait_for("(1 row)", Duration::from_secs(10));
    std::thread::sleep(Duration::from_millis(1600));
    let seen = watch.seen();
    assert_eq!(seen.matches("(1 row)").count(), 1, "{seen}");
}

/// With no write: a shirk moves the due time and marks no done, so the chore
/// is off the list until its instant passes, and a watch's tick alone shows it.
#[test]
fn a_chore_falling_due_redraws_with_no_write() {
    let env = Env::new();
    let due = chrono::Utc::now() + chrono::Duration::seconds(5);
    let due = format!("next_time={}", due.to_rfc3339());
    env.ok(&["occur", "mop", "-T", &due]);
    let watch = env.watch(&["chorelist", "-w"], "100");
    watch.wait_for(NOTHING, Duration::from_secs(5));
    watch.wait_for("mop", Duration::from_secs(10));
}

/// da-sd25: a view gone shows its error in place, and its return redraws.
#[test]
fn an_error_shows_in_place_and_the_watch_goes_on() {
    let env = Env::new();
    let watch = env.watch(&["chorelist", "-w"], "100");
    watch.wait_for(NOTHING, Duration::from_secs(10));
    env.ok(&["kit", "uninstall", "chores"]);
    watch.wait_for("references missing view", Duration::from_secs(10));
    env.install(CHORES, &[]);
    let before = watch.seen().matches(NOTHING).count();
    let start = Instant::now();
    while watch.seen().matches(NOTHING).count() == before {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{}",
            watch.seen()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn w_is_refused_where_the_command_does_not_declare_watch() {
    let env = Env::new();
    let kit = r#"
[kit]
name = "still"
[[view]]
name = "one"
sql = "SELECT 1 AS n"
[[command]]
name = "one"
view = "one"
"#;
    env.install(kit, &[]);
    for flag in ["-w", "--watch"] {
        let out = env.cmd(&["one", flag]).output().unwrap();
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("--watch is not enabled"), "{err}");
    }
}

/// The chores kit as a copy installed it before this: `status` shown, its
/// colors foreground.
fn older_kit() -> String {
    let mut old = CHORES.to_string();
    for (new, was) in [
        (
            r#"columns = ["field", "iti_days", "due", "last_occur"]"#,
            r#"columns = ["field", "iti_days", "due", "last_occur", "status"]"#,
        ),
        (
            "background = \"red\"\ncolor = \"white\"\nbold = true",
            r#"color = "red""#,
        ),
        (r#"background = "yellow""#, r#"color = "yellow""#),
    ] {
        assert!(old.contains(new), "{new}");
        old = old.replace(new, was);
    }
    old
}

#[test]
fn an_older_install_takes_the_kit_with_force_and_keeps_its_chores() {
    let env = Env(tempfile::tempdir().unwrap());
    env.install(&older_kit(), &[]);
    env.ok(&["occur", "dishes", "-d", "-3", "-t", "00:00", "next_date=+1"]);
    let row = |env: &Env| -> serde_json::Value {
        let rows: serde_json::Value =
            serde_json::from_str(&env.ok(&["chorelist", "--json"])).unwrap();
        rows[0].clone()
    };
    assert_eq!(row(&env)["row_color"], "#e01b24", "on the older kit");
    assert!(row(&env).get("status").is_some());

    // A plain install keeps what is there, and says so.
    let kept = env.install(CHORES, &[]);
    assert!(kept.contains("--force"), "{kept}");
    assert!(row(&env).get("row_background").is_none());

    env.install(CHORES, &["--force"]);
    let row = row(&env);
    assert_eq!(row["field"], "dishes");
    assert_eq!(row["row_background"], "#e01b24");
    assert_eq!(row["row_color"], "#ffffff");
    assert_eq!(row["row_bold"], true);
    assert!(row.get("status").is_none(), "{row}");
}
