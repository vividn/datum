//! A number written through `-k` comes back as the number written (da-uru7).
//!
//! `0.23633156966490299` is the shortest round-trip spelling of a double.
//! It used to come back as `0.236331569664903` — one ULP away, a different
//! double — because serde_json's default parser takes a fast path that is
//! not correctly rounded on long mantissas. The writer (ryu) was always
//! exact; the workspace now turns on serde_json's `float_roundtrip` so the
//! reader is too, and these tests hold both ends to `f64::to_bits`.
//!
//! Real binary on purpose: the loss was on the argv → store → JSON path,
//! and only the process sees all of it.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

/// A double whose shortest spelling needs 17 significant digits — the
/// witness from the bead.
const SHARP: &str = "0.23633156966490299";
/// 16 digits; this one always survived, and must keep surviving.
const TWO_THIRDS: &str = "0.6666666666666666";

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let db = tmp.path().join("prec.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
        cmd.env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("NO_COLOR", "1");
        cmd
    }

    fn run(&self, args: &[&str]) -> String {
        let out = self.command().args(args).output().expect("spawn datum");
        ok(&out, args)
    }

    fn run_with_stdin(&self, args: &[&str], input: &str) -> String {
        use std::io::Write;
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn datum");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write stdin");
        let out = child.wait_with_output().expect("wait datum");
        ok(&out, args)
    }
}

fn ok(out: &Output, args: &[&str]) -> String {
    assert!(
        out.status.success(),
        "`datum {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).expect("utf-8 stdout")
}

/// The newest row's `data`, as the CLI's own JSON says it is.
fn newest_data(env: &Env, field: &str) -> serde_json::Value {
    let json = env.run(&["tail", field, "--json", "-n", "1"]);
    let doc: serde_json::Value = serde_json::from_str(&json).expect("tail --json is JSON");
    doc["rows"][0]["data"].clone()
}

/// The one assertion that matters: the same double, not a near one.
#[track_caller]
fn assert_same_double(what: &str, got: &serde_json::Value, spelling: &str) {
    let want: f64 = spelling.parse().expect("test constant parses");
    let got = got
        .as_f64()
        .unwrap_or_else(|| panic!("{what}: expected a number, got {got}"));
    assert_eq!(
        got.to_bits(),
        want.to_bits(),
        "{what}: wrote {spelling}, read back {got} — a different double"
    );
}

/// `occur -k` → `tail --json` and `get --show json` keep every bit.
#[test]
fn a_number_written_through_k_reads_back_bit_identical() {
    let env = Env::new();
    let arr = format!("[{SHARP}, {TWO_THIRDS}]");
    env.run(&[
        "occur",
        "prec",
        "-k",
        &format!("v={SHARP}"),
        "-k",
        &format!("arr={arr}"),
    ]);

    let data = newest_data(&env, "prec");
    assert_same_double("tail --json v", &data["v"], SHARP);
    assert_same_double("tail --json arr[0]", &data["arr"][0], SHARP);
    assert_same_double("tail --json arr[1]", &data["arr"][1], TWO_THIRDS);

    // The reader's other door: `get --show json`, the action echo the web
    // reads instead of parsing printed lines.
    let hid = env.run(&["tail", "prec", "--json", "-n", "1"]);
    let hid: serde_json::Value = serde_json::from_str(&hid).unwrap();
    let hid = hid["rows"][0]["hid"].as_str().expect("hid").to_string();

    let echo = env.run(&["get", &hid, "--show", "json"]);
    assert!(
        echo.contains(SHARP),
        "get --show json must echo the double as written ({SHARP}), got:\n{echo}"
    );

    // And the entry document itself.
    let entry: serde_json::Value =
        serde_json::from_str(&env.run(&["get", &hid, "--pretty"])).expect("get --pretty is JSON");
    assert_same_double("get --pretty v", &entry["data"]["v"], SHARP);
}

/// `update -k` — the layout store's save path — keeps every bit too.
#[test]
fn a_number_written_through_update_reads_back_bit_identical() {
    let env = Env::new();
    env.run(&["occur", "prec"]);
    let tail = env.run(&["tail", "prec", "--json", "-n", "1"]);
    let tail: serde_json::Value = serde_json::from_str(&tail).unwrap();
    let hid = tail["rows"][0]["hid"].as_str().expect("hid").to_string();

    env.run(&["update", &hid, "-k", &format!("root={{\"ratio\":{SHARP}}}")]);

    let data = newest_data(&env, "prec");
    assert_same_double("update root.ratio", &data["root"]["ratio"], SHARP);
}

/// `db backup -` → `db restore -` is a copy, not a rounding.
#[test]
fn a_number_survives_backup_and_restore() {
    let from = Env::new();
    from.run(&["occur", "prec", "-k", &format!("v={SHARP}")]);
    let dump = from.run(&["db", "backup", "-"]);
    assert!(
        dump.contains(SHARP),
        "the backup document must carry the double as written, got:\n{dump}"
    );

    let to = Env::new();
    to.run_with_stdin(&["db", "restore", "-"], &dump);
    assert_same_double("after restore", &newest_data(&to, "prec")["v"], SHARP);
}

/// 1000 draws in (0, 1): write them all, read them all, every bit equal.
#[test]
fn a_thousand_random_doubles_round_trip() {
    // splitmix64, seeded — a failure is reproducible without a dep.
    let mut state: u64 = 0xDA07_2026_0001_0DA7;
    let mut next = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };

    // A double in (0, 1) with a random 52-bit mantissa and an exponent
    // spread over ~7 decades — the long spellings live here.
    let draws: Vec<f64> = (0..1000)
        .map(|_| {
            let r = next();
            let exponent = 1023 - 1 - (r >> 58) % 24;
            let mantissa = r & ((1u64 << 52) - 1);
            f64::from_bits((exponent << 52) | mantissa)
        })
        .collect();

    let arr = serde_json::to_string(&draws).expect("serialize draws");
    let env = Env::new();
    env.run(&["occur", "prec", "-k", &format!("arr={arr}")]);

    let read = newest_data(&env, "prec");
    let read = read["arr"].as_array().expect("arr is an array");
    assert_eq!(read.len(), draws.len(), "all 1000 draws come back");
    for (i, (want, got)) in draws.iter().zip(read).enumerate() {
        let got = got
            .as_f64()
            .unwrap_or_else(|| panic!("draw {i}: not a number"));
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "draw {i}: wrote {want:?}, read back {got:?}"
        );
    }
}
