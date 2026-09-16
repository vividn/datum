//! A kit carries layouts into the DATA database (da-gg0v).
//!
//! vividn, 2026-09-15 18:53: "layouts have to go inside the db proper, not
//! the _internal db". So a `[[layout]]` block does not land in a table of
//! its own the way `[[field]]` and `[[view]]` do — it lands as an ordinary
//! entry of field `layout`, slug `layout:<name>`, in the database the kit
//! installs into, carrying `kit_id` in its data. That row syncs with the
//! data, so one install reaches every device of that database, and it is
//! the same row the web's layout store writes one database over
//! (`web/src/lib/layout/store.ts`) — which is what lets the web list a
//! kit's layouts beside the person's own.
//!
//! The real binary, because what is asserted is the DATA the entry path
//! writes and the JSON `tail` prints; an in-process dispatch would leave
//! the stdout half unread.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value as Json;
use tempfile::TempDir;

/// The tree a kit ships here — a whole, if minimal, `Layout.root`.
const ROOT: &str = r#"{"id":"root","tabs":[{"id":"root-tab","content":{"id":"w","kind":"tail","settings":{}}}],"active":0,"bar":{"edge":"top","align":"start"}}"#;

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
    dir: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            db: tmp.path().join("data.db"),
            dir: tmp.path().to_path_buf(),
            _tmp: tmp,
            home,
            config_home,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    /// Run and return stdout, failing the test with both streams on a
    /// non-zero exit — a kit that did not install is worth reading.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}` exited {:?}\n  stdout: {}\n  stderr: {}",
            args.join(" "),
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).expect("stdout is utf-8")
    }

    /// Write a kit TOML and install it; the path, for a second install.
    fn install(&self, name: &str, toml: &str, extra: &[&str]) -> String {
        let path = self.dir.join(format!("{name}.toml"));
        std::fs::write(&path, toml).unwrap();
        let path = path.to_string_lossy().to_string();
        let mut args = vec!["kit", "install", path.as_str()];
        args.extend_from_slice(extra);
        self.ok(&args)
    }

    /// The `layout` entries, as `tail --json` lists them.
    /// The rows of the `layout` field that ARE layouts. da-c8mh's TRACK
    /// document shares the field — it is the row that declared it — and
    /// carries no data of its own.
    fn layouts(&self) -> Vec<Json> {
        let listed: Json =
            serde_json::from_str(&self.ok(&["tail", "layout", "--json", "-n", "50"])).unwrap();
        listed["rows"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter(|row| !row["data"].is_null())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A kit shipping one layout, with a field beside it so the layout is not
/// the only thing the install has to carry.
fn kit_toml(name: &str, layout: &str, designed_for: &str) -> String {
    format!(
        r#"[kit]
name = "{name}"
version = "1.0"

[[field]]
name = "coffee"
verb = "occur"

[[layout]]
name = "{layout}"
designed_for = "{designed_for}"
root = '{ROOT}'
"#
    )
}

#[test]
fn install_writes_the_layout_into_the_data_db() {
    let env = Env::new();
    let printed = env.install("focus", &kit_toml("focus", "focus-day", "desktop"), &[]);
    assert!(
        printed.contains("Installed 1 layout(s):") && printed.contains("+ focus-day"),
        "install did not report the layout: {printed}"
    );

    let rows = env.layouts();
    assert_eq!(rows.len(), 1, "expected one layout entry, got {rows:#?}");
    let row = &rows[0];
    // The slug is the NAME's, not the instant's: it is the identity both
    // the kit and the web address a layout by.
    assert_eq!(row["slug"], "layout:focus-day");
    assert_eq!(row["field"], "layout");
    let data = &row["data"];
    assert_eq!(data["kit_id"], "focus", "the row must name its kit");
    assert_eq!(data["name"], "focus-day");
    assert_eq!(data["designedFor"], "desktop");
    // The web's spelling, because the web reads this very row.
    assert_eq!(data["version"], 1);
    assert_eq!(
        data["root"],
        serde_json::from_str::<Json>(ROOT).unwrap(),
        "the stored tree must be the kit's, whole"
    );
}

#[test]
fn export_reproduces_the_layout_block_and_list_counts_it() {
    let env = Env::new();
    env.install("focus", &kit_toml("focus", "focus-day", "mobile"), &[]);

    let listed = env.ok(&["kit", "list"]);
    let row = listed
        .lines()
        .find(|l| l.starts_with("focus"))
        .unwrap_or_else(|| panic!("no `focus` row in:\n{listed}"));
    assert!(
        listed.contains("layouts"),
        "`kit list` must have a layouts column:\n{listed}"
    );
    assert_eq!(
        row.split_whitespace().last(),
        Some("1"),
        "`kit list` must count the layout:\n{listed}"
    );

    let exported = env.ok(&["kit", "export", "focus"]);
    let kit = datum_core::kit::KitToml::from_str(&exported).unwrap();
    assert_eq!(kit.layouts.len(), 1, "exported:\n{exported}");
    assert_eq!(kit.layouts[0].name, "focus-day");
    assert_eq!(kit.layouts[0].designed_for.as_deref(), Some("mobile"));
    assert_eq!(
        serde_json::from_str::<Json>(&kit.layouts[0].root).unwrap(),
        serde_json::from_str::<Json>(ROOT).unwrap()
    );

    // And the export is installable again: re-install → export is the same
    // bytes, which is the round-trip the other kit blocks are held to.
    let path = env.dir.join("again.toml");
    std::fs::write(&path, &exported).unwrap();
    env.ok(&["kit", "install", &path.to_string_lossy(), "--force"]);
    assert_eq!(env.ok(&["kit", "export", "focus"]), exported);
}

#[test]
fn a_persons_own_layout_of_the_same_name_is_not_overwritten() {
    let env = Env::new();
    // What the web writes when someone saves a layout (store.ts): the same
    // field, the same slug, no kit.
    env.ok(&[
        "occur",
        "layout",
        "--id",
        "%name%",
        "-k",
        r#"name="focus-day""#,
        "-k",
        "version=1",
        "-k",
        r#"root={"id":"mine","tabs":[],"active":0,"bar":{"edge":"top","align":"start"}}"#,
    ]);

    let printed = env.install("focus", &kit_toml("focus", "focus-day", "desktop"), &[]);
    assert!(
        printed.contains("Skipped 1 layout(s)"),
        "the kit must not take a name someone already saved: {printed}"
    );
    let rows = env.layouts();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["data"]["root"]["id"], "mine");
    assert!(
        rows[0]["data"].get("kit_id").is_none(),
        "the person's own layout must stay unowned: {:#?}",
        rows[0]["data"]
    );

    // `--force` is the way through, and then uninstall takes it back.
    env.install(
        "focus",
        &kit_toml("focus", "focus-day", "desktop"),
        &["--force"],
    );
    assert_eq!(env.layouts()[0]["data"]["kit_id"], "focus");
    let removed = env.ok(&["kit", "uninstall", "focus"]);
    assert!(
        removed.contains("1 layout(s)"),
        "uninstall must report the layout: {removed}"
    );
    assert!(env.layouts().is_empty(), "the layout entry must be gone");
}

#[test]
fn a_kit_whose_layout_is_not_a_tree_installs_nothing() {
    let env = Env::new();
    let path = env.dir.join("bad.toml");
    std::fs::write(
        &path,
        r#"[kit]
name = "bad"

[[field]]
name = "coffee"
verb = "occur"

[[layout]]
name = "broken"
root = "not json at all"
"#,
    )
    .unwrap();
    let out = env.run(&["kit", "install", &path.to_string_lossy()]);
    assert!(
        !out.status.success(),
        "a malformed layout must fail install"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("root is not JSON"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Screened before any write, as the index DDL is: the field beside it
    // did not land either.
    assert!(env.ok(&["kit", "list"]).contains("No kits installed"));
}
