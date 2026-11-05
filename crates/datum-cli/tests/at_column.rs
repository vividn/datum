//! da-95avu: the column is `at`, its old ops and old SQL still read. The
//! fixtures are a v75 chain made by the v75 binary: `v75_chain.sql` is one
//! copy's file with a kit whose SQL says `occur_time`, a real conflict on the
//! time and a retime, `v75_chain.spool` two writes it queued, and
//! `v75_chain_backup.jsonl` its `db backup`.

use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use std::io::Write;
use tempfile::TempDir;

const DUMP: &str = include_str!("fixtures/v75_chain.sql");
const SPOOL: &str = include_str!("fixtures/v75_chain.spool");
const BACKUP: &str = include_str!("fixtures/v75_chain_backup.jsonl");
const V2: &str = include_str!("fixtures/datum_backup_synthetic.json");

struct Copy(TempDir);

impl Copy {
    fn empty() -> Self {
        Copy(tempfile::tempdir().unwrap())
    }

    /// The fixture, in a file at this copy's path, as the v75 binary left it.
    fn old() -> Self {
        let copy = Self::empty();
        let conn = Connection::open(copy.db()).unwrap();
        conn.execute_batch(DUMP).unwrap();
        // Its own client, so it keeps its endpoint key and is the member it was.
        let client: String = conn
            .query_row("SELECT client_id FROM sync_local", [], |r| r.get(0))
            .unwrap();
        std::fs::create_dir_all(copy.dir().join("home")).unwrap();
        std::fs::write(copy.dir().join("home/client_id"), client).unwrap();
        copy
    }

    fn dir(&self) -> &Path {
        self.0.path()
    }

    fn db(&self) -> PathBuf {
        self.dir().join("data.db")
    }

    fn run(&self, args: &[&str], stdin: &str) -> Output {
        let d = self.dir();
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(args)
            .env("HOME", d)
            .env("DATUM_HOME", d.join("home"))
            .env("DATUM_CONFIG_HOME", d.join("config"))
            .env("XDG_CONFIG_HOME", d.join("xc"))
            .env("XDG_DATA_HOME", d.join("xd"))
            .env("XDG_STATE_HOME", d.join("xs"))
            .env("DATUM_DB", self.db())
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .env("NO_COLOR", "1")
            .env("TZ", "UTC")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn ok_in(&self, args: &[&str], stdin: &str) -> String {
        let out = self.run(args, stdin);
        assert!(
            out.status.success(),
            "`datum {}`: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        self.ok_in(args, "")
    }

    /// Send `other` every op it lacks.
    fn send(&self, other: &Copy) {
        let vector = other.ok(&["sync", "pipe", "vector"]);
        let ops = self.ok(&["sync", "pipe", "ops", "--since", vector.trim()]);
        other.ok_in(&["sync", "pipe", "apply"], &ops);
    }

    fn hash(&self) -> String {
        self.ok(&["sync", "pipe", "verify"]).trim().to_string()
    }

    /// Every row's whole content by hid, each column by the name `time` has
    /// in this schema, so two schemas compare.
    fn rows(&self, time: &str) -> BTreeMap<String, Vec<Option<String>>> {
        let conn = Connection::open(self.db()).unwrap();
        let sql = format!(
            "SELECT hid, {time}, {time}_offset, {time}_tz, {time}_uncertainty, slug, field, state,
                    dur, following_state, data, verb, content_hash, id_structure, deleted_at,
                    superseded_by, create_time, modify_time
             FROM entries"
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        let n = stmt.column_count();
        let rows = stmt
            .query_map([], |r| {
                let cell = |i| {
                    Ok(match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => None,
                        v => Some(match v {
                            rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                            rusqlite::types::ValueRef::Text(t) => {
                                String::from_utf8_lossy(t).into_owned()
                            }
                            other => format!("{other:?}"),
                        }),
                    })
                };
                Ok((
                    r.get::<_, String>(0)?,
                    (1..n).map(cell).collect::<rusqlite::Result<Vec<_>>>()?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()
            .unwrap();
        rows
    }

    /// `check` finds no error, of sync's or a row's.
    fn clean(&self) {
        let text = self.ok(&["check"]);
        assert!(text.contains("Summary: 0 errors"), "{text}");
    }

    fn query(&self, sql: &str) -> String {
        Connection::open(self.db())
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }
}

/// An old file migrates: every row as it was, under the new names, `check`
/// finds nothing wrong with the sync or the rows, and the hash holds.
#[test]
fn an_old_chain_migrates_with_every_time_unchanged() {
    let copy = Copy::old();
    let before = copy.rows("occur_time");
    let timed = before.values().filter(|r| r[0].is_some()).count();
    assert!(timed >= 15, "the fixture has its times: {timed}");

    let hash = copy.hash();
    assert_eq!(copy.rows("at"), before);
    assert_eq!(copy.rows("occur_time"), before, "the old names read true");
    assert_eq!(copy.hash(), hash, "the hash holds across an open");

    copy.clean();

    // The conflict on the time reads as `at`, and as before.
    let conflicts = copy.query("SELECT conflicts FROM entries WHERE conflicts IS NOT NULL");
    let conflicts: serde_json::Value = serde_json::from_str(&conflicts).unwrap();
    assert_eq!(conflicts["at"].as_array().unwrap().len(), 2, "{conflicts}");
    assert!(conflicts.get("occur_time").is_none(), "{conflicts}");
    assert!(copy.ok(&["conflicts", "list"]).contains("\n    at\n"));

    // What the migration keeps is the file as it was.
    let kept = std::fs::read_dir(copy.dir())
        .unwrap()
        .filter_map(|e| Some(e.ok()?.path()))
        .find(|p| p.to_string_lossy().contains(".pre-v75-to-"))
        .expect("a pre-migration copy");
    let conn = Connection::open(kept).unwrap();
    let rows: i64 = conn
        .query_row("SELECT COUNT(occur_time) FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows as usize, timed);
}

/// A copy joining fresh folds the old ops to the rows the old file holds, in
/// the new names, and to its hash; and ops signed since say `at`, and hash the
/// same on both.
#[test]
fn old_ops_replay_to_the_same_rows_on_a_fresh_copy() {
    let (old, fresh) = (Copy::old(), Copy::empty());
    let before = old.rows("occur_time");
    old.send(&fresh);
    assert_eq!(old.hash(), fresh.hash());
    let cols = |c: &Copy| -> BTreeMap<String, Vec<Option<String>>> {
        c.rows("at")
            .into_iter()
            .map(|(hid, mut row)| {
                // Which row a merge leaves pointing where is the same; the
                // rest of the row is all that a fold is asked for.
                row.truncate(14);
                (hid, row)
            })
            .collect()
    };
    let want: BTreeMap<_, _> = before
        .into_iter()
        .map(|(hid, mut row)| {
            row.truncate(14);
            (hid, row)
        })
        .collect();
    assert_eq!(cols(&fresh), want);
    assert_eq!(
        fresh.query("SELECT conflicts FROM entries WHERE conflicts IS NOT NULL"),
        old.query("SELECT conflicts FROM entries WHERE conflicts IS NOT NULL")
    );
    fresh.clean();

    // Written now: the op says `at`, an edit of an old row's time too.
    old.ok(&["occur", "later", "-d", "2026-10-07", "-t", "09:00"]);
    // 16:00 in its own zone, two hours ahead of UTC that day.
    old.ok(&["retime", "zoned", "-d", "2026-10-03", "-t", "16:00"]);
    let newest = old.query("SELECT body FROM ops ORDER BY rowid DESC LIMIT 1");
    assert!(newest.contains("\"at\":{"), "{newest}");
    assert!(!newest.contains("occur_time"), "{newest}");
    old.send(&fresh);
    assert_eq!(old.hash(), fresh.hash());
    assert_eq!(
        fresh.query("SELECT at FROM entries WHERE field = 'zoned'"),
        "2026-10-03T14:00:00+00:00"
    );
}

/// SQL stored before the rename answers after it, on the copy that held it and
/// on one that took it from the ops; a shipped view's `AS at` still sorts and
/// names; and the kit's index on an old name still builds.
#[test]
fn stored_sql_with_the_old_names_still_answers() {
    let (old, fresh) = (Copy::old(), Copy::empty());
    let times = |c: &Copy| c.ok(&["view", "oldsql"]);
    let first = times(&old);
    assert!(
        first.contains("hold") && first.contains("2026-10-04T10:00:00"),
        "{first}"
    );
    assert!(first.contains("Europe/Berlin"), "{first}");
    let aliased = old.ok(&["view", "aliased"]);
    assert!(aliased.contains("2026-10-01"), "{aliased}");
    assert!(
        !aliased.contains("2026-10-01T"),
        "the alias, not the column: {aliased}"
    );

    old.send(&fresh);
    assert_eq!(times(&fresh), first);
    assert_eq!(fresh.ok(&["view", "aliased"]), aliased);

    // Written since, an old view reads it too.
    old.ok(&["occur", "later", "-d", "2026-10-07", "-t", "09:00"]);
    assert!(times(&old).contains("2026-10-07T09:00:00"));
    let index = old.query("SELECT sql FROM sqlite_master WHERE name = 'kit_oldkit__oldkit_time'");
    assert!(index.contains("field"), "{index}");
}

/// A row written before and one written after land on one slug, default
/// template or a stored `%occur_time%` of a field's own.
#[test]
fn stored_id_templates_keep_their_slugs() {
    let copy = Copy::old();
    let holds = "SELECT COUNT(*) || ' ' || MIN(slug) FROM entries
                 WHERE field = 'hold' AND deleted_at IS NULL";
    let one = "1 hold:7:2026-10-04T10:00:00.000Z";
    copy.ok(&["tail", "-n", "1"]);
    assert_eq!(copy.query(holds), one);
    // Written again, it meets the row before on that slug and writes nothing.
    let again = copy.run(
        &["occur", "hold", "-d", "2026-10-04", "-t", "10:00", "pr=7"],
        "",
    );
    assert!(String::from_utf8_lossy(&again.stdout).contains("EXISTS"));
    assert_eq!(copy.query(holds), one, "the stored template, after");

    // The default: an old row's `%occur_time%`, a new row's `%at%`.
    let old_row = copy.query("SELECT slug FROM entries WHERE field = 'hasoldkey'");
    copy.ok(&["delete", "hasoldkey"]);
    copy.ok(&[
        "occur",
        "hasoldkey",
        "-d",
        "2026-10-04",
        "-t",
        "10:05",
        "occur_time_note=kept",
    ]);
    assert_eq!(
        copy.query("SELECT slug FROM entries WHERE field = 'hasoldkey' AND deleted_at IS NULL"),
        old_row
    );
    copy.ok(&["occur", "brandnew", "-d", "2026-10-04", "-t", "10:05"]);
    assert_eq!(
        copy.query("SELECT id_structure FROM entries WHERE field = 'brandnew'"),
        "%at%"
    );
    // What was stored stays as stored, and reads the same.
    assert_ne!(
        copy.query("SELECT COUNT(*) || '' FROM entries WHERE id_structure = '%occur_time%'"),
        "0"
    );
}

/// What an old binary queued lands on its next open in the new names.
#[test]
fn a_line_an_old_binary_spooled_lands_with_its_time() {
    let copy = Copy::old();
    std::fs::write(copy.dir().join("data.db.spool"), SPOOL).unwrap();
    let hash_of_field = |f: &str| format!("SELECT at FROM entries WHERE field = '{f}'");
    copy.ok(&["tail", "-n", "1"]);
    assert_eq!(
        copy.query(&hash_of_field("queued")),
        "2026-10-06T07:00:00+00:00"
    );
    assert_eq!(
        copy.query(&hash_of_field("water")),
        "2026-10-02T08:50:00+00:00"
    );
    let last = copy.query("SELECT body FROM ops ORDER BY rowid DESC LIMIT 1");
    assert!(
        last.contains("\"at\":{") && !last.contains("occur_time"),
        "{last}"
    );
    copy.clean();
}

/// The undo journal and a backup hold rows in the old name: neither loses a time.
#[test]
fn an_old_snapshot_and_an_old_backup_keep_their_times() {
    let copy = Copy::old();
    let water = "SELECT at FROM entries WHERE field = 'water'";
    copy.ok(&["tail", "-n", "1"]);
    assert_eq!(copy.query(water), "2026-10-02T08:45:00+00:00");
    copy.ok(&["history", "water"]);
    let shown = copy.ok(&["history", "water", "--version", "1"]);
    assert!(
        shown.contains("\"at\": {") && shown.contains("08:30:00"),
        "{shown}"
    );

    let restored = Copy::empty();
    restored.ok_in(&["db", "restore", "-", "--yes"], BACKUP);
    assert_eq!(restored.query(water), "2026-10-02T08:50:00+00:00");
    let timed =
        "SELECT COUNT(at) || '' FROM entries WHERE field = 'zoned' AND at_tz = 'Europe/Berlin'";
    assert_eq!(restored.query(timed), "1");
    restored.clean();
}

/// `db v2import` into the new schema, then a sync to a second new copy.
#[test]
fn a_v2_import_syncs_to_a_second_copy() {
    let (a, b) = (Copy::empty(), Copy::empty());
    let file = a.dir().join("v2.json");
    std::fs::write(&file, V2).unwrap();
    a.ok(&["db", "v2import", "--from", file.to_str().unwrap()]);
    let timed = a.query("SELECT COUNT(at) || '' FROM entries");
    assert_ne!(timed, "0");
    a.send(&b);
    assert_eq!(b.query("SELECT COUNT(at) || '' FROM entries"), timed);
    assert_eq!(a.hash(), b.hash());
}
