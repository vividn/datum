//! A named checker lists every finding of every category it reports
//! (da-80t1i). For each category in [`CATEGORIES`], two are planted in a
//! database of their own, the checker is named, and both must be in the lines
//! printed under the category. A category with no way to plant two says why
//! here, and a new one fails until it is given either.

use datum_core::action::Action;
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::field_spec::{Alias, FieldSpec};
use datum_core::time::DatumTime;
use serde_json::json;

use super::{render_human, report, CheckArgs, CATEGORIES};

/// How two of a category are planted, each named by a word printed under it;
/// or why two cannot be.
enum Plant {
    Two(fn(&Db) -> [String; 2]),
    Cannot(&'static str),
    /// A category this test has not been given either for.
    Unknown,
}

fn plant(category: &str) -> Plant {
    use Plant::{Cannot, Two};
    match category {
        "time_end_before_start" => {
            Cannot("nothing can: a row is `at` and `dur`, and the query finds none (da-5ns)")
        }
        "reserved_data_key" => Two(|db| {
            two(|field| {
                let mut e = log(field, "x");
                e.data = Some(json!({ "at": "noon" }));
                db.insert_entry(&e).unwrap();
            })
        }),
        "content_hash_mismatch" => Two(|db| {
            let both = two(|field| {
                db.insert_entry(&log(field, "x")).unwrap();
            });
            let sql = "UPDATE entries SET content_hash = 'deadbeef'";
            db.conn().execute(sql, []).unwrap();
            both
        }),
        "unknown_field" => Two(|db| {
            two(|field| {
                db.insert_entry(&log(field, "x")).unwrap();
            })
        }),
        "orphaned_extra_data_key" => Two(|db| {
            two(|field| {
                let mut spec = FieldSpec::new(field, Action::Switch);
                spec.only_listed_keys = true;
                db.save_field_spec(&spec).unwrap();
                let mut e = log(field, "x");
                e.data = Some(json!({ "weather": "sunny" }));
                db.insert_entry(&e).unwrap();
            })
        }),
        "mistyped_data_value" => Two(|db| {
            two(|field| {
                let mut e = log(field, "x");
                e.data = Some(json!({ "wake_time": "banana" }));
                db.insert_entry(&e).unwrap();
            })
        }),
        "kit_index_unbuilt" => Two(|db| {
            let both = two(|_| {});
            let indices: Vec<datum_core::kit::IndexEntry> = (both.iter())
                .map(|name| datum_core::kit::IndexEntry {
                    name: name.clone(),
                    on: "entries".into(),
                    expression: "field".into(),
                    where_clause: None,
                    unique: false,
                })
                .collect();
            db.install_kit("k", &[], &indices, false).unwrap();
            // No kit may declare a unique one any more (da-9lo90).
            let sql = "UPDATE entries SET data = json_set(data, '$.unique', json('true'))
                       WHERE field = 'index'";
            db.conn().execute(sql, []).unwrap();
            both
        }),
        "verb_corruption" => Two(|db| {
            two(|field| {
                db.insert_entry(&log(field, "x")).unwrap();
                let sql = "UPDATE entries SET dur = '-PT5M' WHERE field = ?1";
                db.conn().execute(sql, [field]).unwrap();
            })
        }),
        "verb_drift" => Two(|db| {
            two(|field| {
                let e = log(field, "x");
                db.insert_entry(&e).unwrap();
                db.set_entry_verb(e.hid.as_str(), "block").unwrap();
            })
        }),
        "slug_drift" => Two(|db| {
            two(|field| {
                let mut e = log(field, "x");
                e.id_structure = Some("%at%".to_string());
                e.slug = datum_core::ids::Slug(format!("{field}:{}", e.short_uid()));
                db.insert_entry(&e).unwrap();
            })
        }),
        "alias_state_collision" => Two(|db| {
            for field in ["owner", "loser"] {
                let mut spec = FieldSpec::new(field, Action::Switch);
                spec.allowed_states = Some(vec!["sa".into(), "sb".into()]);
                spec.regenerate_derived_aliases();
                db.save_field_spec(&spec).unwrap();
            }
            ["'sa'".into(), "'sb'".into()]
        }),
        "alias_legacy_double_dash" => Two(|db| two(|name| alias(db, name, "note x -- y"))),
        "spec_alias_names_stale" => Two(|db| {
            let mut spec = FieldSpec::new("mood", Action::Switch);
            spec.allowed_states = Some(vec!["sa".into(), "sb".into()]);
            spec.regenerate_derived_aliases();
            db.save_field_spec(&spec).unwrap();
            let sql = "DELETE FROM spec_alias_names WHERE name IN ('sa', 'sb')";
            db.conn().execute(sql, []).unwrap();
            ["'sa'".into(), "'sb'".into()]
        }),
        "alias_name_not_plain" => Two(|db| {
            for name in ["x;rm", "x;touch"] {
                alias(db, name, "occur coffee");
            }
            ["'x;rm'".into(), "'x;touch'".into()]
        }),
        "alias_shadowed_by_command" => Two(|db| {
            for name in ["stats", "tail"] {
                alias(db, name, "occur coffee");
            }
            ["'stats'".into(), "'tail'".into()]
        }),
        "kit_command_shadowed_by_field" => Two(|db| {
            let both = two(|_| {});
            let commands: Vec<datum_core::kit::CommandEntry> = (both.iter())
                .map(|n| toml::from_str(&format!("name = '{n}'\nexpands_to = 'tail'")).unwrap())
                .collect();
            db.install_kit_commands("k", &commands, false).unwrap();
            for name in &both {
                let mut spec = FieldSpec::new(name, Action::Occur);
                spec.regenerate_derived_aliases();
                db.save_field_spec(&spec).unwrap();
            }
            both
        }),
        "alias_orphan" => Two(|db| two(|name| alias(db, name, &format!("{name}_gone")))),
        "alias_cycle" => Two(|db| two(|name| alias(db, name, name))),
        "repeated_state" => Two(|db| {
            two(|field| {
                for h in [1, 2] {
                    let mut e = log(field, "same");
                    e.at = Some(DatumTime::from_utc_and_tz(hour(h), "UTC").unwrap());
                    db.insert_entry(&e).unwrap();
                }
            })
        }),
        // A FINISHED written before an entry already after it, as a sync brings one.
        "integrity_after_finished" => Two(|db| {
            two(|field| {
                for (h, state) in [(2, "later"), (1, datum_core::shape::FINISHED_STATE_VALUE)] {
                    let mut e = log(field, state);
                    e.at = Some(DatumTime::from_utc_and_tz(hour(h), "UTC").unwrap());
                    db.insert_entry(&e).unwrap();
                }
            })
        }),
        "integrity_warning_unreadable" => {
            Two(|db| two(|id| warn(db, id, &log("x", "x"), "not json", "double_open")))
        }
        "integrity_double_open" => Two(|db| stored(db, "double_open")),
        "integrity_orphan_end" => Two(|db| stored(db, "orphan_end")),
        "integrity_overlap" => Two(|db| stored(db, "overlap")),
        "integrity_last_state_mismatch" => Two(|db| stored(db, "last_state_mismatch")),
        "sync_conflict" => Two(|db| {
            two(|field| {
                db.insert_entry(&log(field, "x")).unwrap();
                let sql = r#"UPDATE entries SET conflicts = '{"state":[]}' WHERE field = ?1"#;
                db.conn().execute(sql, [field]).unwrap();
            })
        }),
        "key_drift" => Two(|db| {
            two(|field| {
                for (min, has, base) in [
                    (0, json!(["a"]), json!([])),
                    (20, json!(["a", "b"]), json!(["a"])),
                    (10, json!(["a", "c"]), json!(["a"])),
                ] {
                    let at =
                        DatumTime::from_utc_and_tz(hour(0) + chrono::Duration::minutes(min), "UTC")
                            .unwrap();
                    let moment = Some(datum_core::dur::Dur::moment());
                    let mut e = Entry::new(
                        field,
                        Verb::Point,
                        None,
                        Some(json!({ "k": has })),
                        Some(at),
                        moment,
                        "t",
                    );
                    e.extra_metadata = Some(json!({ "asserted_base": { "k": base } }));
                    db.insert_entry(&e).unwrap();
                }
            })
        }),
        // Two rows of one field referring to one slug are one issue.
        "dangling_state_ref" => Two(|db| refer(db, "book:gone", None)),
        "dangling_data_ref" => {
            Two(|db| refer(db, "x", Some(json!({ "needs": "ref://issue:gone" }))))
        }
        "stats_cache_drift" => Two(|db| {
            let _now = datum_core::test_util::freeze_time("2026-06-15T12:00:00Z");
            for (t, state) in [
                ("2026-06-10T09:00:00Z", "coding"),
                ("2026-06-11T09:00:00Z", "meeting"),
                ("2026-06-12T09:00:00Z", "coding"),
            ] {
                datum_core::test_util::entry_builder("activity")
                    .verb(Verb::Switch)
                    .state(json!(state))
                    .time_start_utc(t.parse().unwrap())
                    .insert(db);
            }
            let args = crate::args::StatsArgs::default();
            crate::commands::stats::compute_all_fields(&args, db).unwrap();
            let sql = "UPDATE stats_cache SET seconds = seconds + 60 WHERE field = 'activity'
                       AND day IN ('2026-06-10', '2026-06-11')";
            db.conn().execute(sql, []).unwrap();
            ["2026-06-10".into(), "2026-06-11".into()]
        }),
        "sync_orphan_ops" => Two(|db| {
            two(|field| {
                db.insert_entry(&log(field, "x")).unwrap();
                db.conn()
                    .execute("DELETE FROM entries WHERE field = ?1", [field])
                    .unwrap();
            })
        }),
        "sync_stale_row" => Two(|db| {
            two(|field| {
                db.insert_entry(&log(field, "x")).unwrap();
                let sql = r#"UPDATE entries SET data = '{"cups":5}' WHERE field = ?1"#;
                db.conn().execute(sql, [field]).unwrap();
            })
        }),
        "sync_unfoldable" => Two(|db| {
            two(|field| {
                let e = log(field, "x");
                db.insert_entry(&e).unwrap();
                let sql = "UPDATE ops SET body = json_set(body, '$.effect.create.verb', 'bogus') WHERE hid = ?1";
                db.conn().execute(sql, [e.hid.as_str()]).unwrap();
            })
        }),
        "sync_no_op" => Two(|db| {
            let genesis = json!({"type": "genesis", "chain": "c1", "admins": [], "label": "a"});
            db.sync_chain_write(genesis, true).unwrap();
            two(|field| {
                let e = log(field, "x");
                db.insert_entry(&e).unwrap();
                db.conn()
                    .execute("DELETE FROM ops WHERE hid = ?1", [e.hid.as_str()])
                    .unwrap();
            })
        }),
        "sync_refused" => Two(|db| notices(db, "refused")),
        "sync_joined" => {
            Cannot("a copy joins one chain: every `member` notice is said as one line, its joining")
        }
        "sync_unlocked" => Two(|db| notices(db, "unlock")),
        "sync_wiped" => Two(|db| notices(db, "wipe")),
        "sync_new_member" => Two(|db| notices(db, "endpoint")),
        "sync_upgrade" | "sync_unsent" => Cannot(
            "each finding is an issue of its own, printed whole: a member's newer schema, \
             and this copy's changes no other member holds",
        ),
        "flagged_entry" => Two(|db| {
            two(|field| {
                let mut e = log(field, "x");
                e.extra_metadata = Some(json!({ "flag": true }));
                db.insert_entry(&e).unwrap();
            })
        }),
        "timezone_state_not_a_zone" => Two(|db| {
            two(|state| {
                db.insert_entry(&log("timezone", state)).unwrap();
            })
        }),
        "tz_offset_mismatch" => Two(|db| {
            let both = two(|field| zoned(db, field, 0, "America/Chicago"));
            db.conn()
                .execute("UPDATE entries SET at_offset = 0", [])
                .unwrap();
            both
        }),
        "unknown_timezone" => Two(|db| {
            let both = two(|field| zoned(db, field, 0, "Europe/Berlin"));
            let sql = "UPDATE entries SET at_tz = 'Mars/Olympus_Mons'";
            db.conn().execute(sql, []).unwrap();
            both
        }),
        "tz_off_neighbours" => Two(|db| {
            for (h, z) in "BBBTBBBBTBBB".chars().enumerate() {
                match (z, h) {
                    ('B', _) => zoned(db, "work", h, "Europe/Berlin"),
                    (_, 3) => zoned(db, "firstone", h, "Asia/Tokyo"),
                    _ => zoned(db, "secondone", h, "Asia/Tokyo"),
                }
            }
            ["firstone".into(), "secondone".into()]
        }),
        "tz_off_declared" => Two(|db| {
            let at = DatumTime::from_utc_and_tz(hour(0), "Europe/Berlin").unwrap();
            let mut declared = log("timezone", "Europe/Berlin");
            declared.at = Some(at);
            db.insert_entry(&declared).unwrap();
            let mut h = 0;
            two(|field| {
                h += 1;
                zoned(db, field, h, "Asia/Tokyo");
            })
        }),
        "spool_stuck" => {
            Cannot("one queued change is stuck, the first, which it names: the rest wait behind it")
        }
        "spool_waiting" => Cannot(
            "one finding, the queue waiting for the lock, counted: the next command lands it",
        ),
        "spool_set_aside" => Two(|db| {
            let both = [".spool.torn", ".spool.rejected"].map(String::from);
            for ext in &both {
                let mut file = db.path().unwrap().as_os_str().to_owned();
                file.push(ext);
                std::fs::write(file, "{}\n").unwrap();
            }
            both
        }),
        "install_completion_missing"
        | "install_update"
        | "install_update_check"
        | "install_daemon_missing"
        | "install_daemon_binary"
        | "install_daemon_stale"
        | "backup_daemon_missing"
        | "backup_daemon_failed" => Cannot(
            "what `datum install` set up, or a systemd unit, is no database's: each finding \
             is an issue of its own, printed whole",
        ),
        _ => Plant::Unknown,
    }
}

/// `f` run with each of two names, which are returned.
fn two(mut f: impl FnMut(&str)) -> [String; 2] {
    ["firstone", "secondone"].map(|name| {
        f(name);
        name.to_string()
    })
}

fn log(field: &str, state: &str) -> Entry {
    let at = Some(DatumTime::now());
    Entry::new_legacy(
        field,
        Verb::Switch,
        Some(json!(state)),
        None,
        at,
        None,
        "d1",
    )
}

/// The hour `h` of a day in 2024.
fn hour(h: usize) -> chrono::DateTime<chrono::Utc> {
    let day: chrono::DateTime<chrono::Utc> = "2024-01-15T00:00:00Z".parse().unwrap();
    day + chrono::Duration::hours(h as i64)
}

/// A moment of `field` at the hour `h`, in `zone`.
fn zoned(db: &Db, field: &str, h: usize, zone: &str) {
    let at = DatumTime::from_utc_and_tz(hour(h), zone).unwrap();
    let e = Entry::new_legacy(field, Verb::Point, None, None, Some(at), None, "d1");
    db.insert_entry(&e).unwrap();
}

/// Two `media` entries an hour apart, of `state` and `data`, by their hids
/// as a check shows them.
fn refer(db: &Db, state: &str, data: Option<serde_json::Value>) -> [String; 2] {
    [1, 2].map(|h| {
        let mut e = log("media", state);
        e.at = Some(DatumTime::from_utc_and_tz(hour(h), "UTC").unwrap());
        e.data = data.clone();
        db.insert_entry(&e).unwrap();
        super::short(e.hid.as_str())
    })
}

/// A stored integrity warning of `kind` on `e`, its `local_entry` as given.
fn warn(db: &Db, id: &str, e: &Entry, local: &str, kind: &str) {
    let sql = "INSERT INTO integrity_warnings
        (id, hid, entry_slug, local_entry, detected_at, detected_by, kind)
        VALUES (?1, ?2, ?3, ?4, '2026-10-01T00:00:00Z', 'local_validator', ?5)";
    let row = [id, e.hid.as_str(), e.slug.as_str(), local, kind];
    db.conn().execute(sql, row).unwrap();
}

/// Two entries, each with a stored integrity warning of `kind`.
fn stored(db: &Db, kind: &str) -> [String; 2] {
    two(|field| {
        let e = log(field, "x");
        db.insert_entry(&e).unwrap();
        warn(db, field, &e, &serde_json::to_string(&e).unwrap(), kind);
    })
}

/// Two sync notices of `kind`, labeled by their names.
fn notices(db: &Db, kind: &str) -> [String; 2] {
    two(|label| {
        let sql = "INSERT INTO sync_notices (subject, kind, label, at, via)
            VALUES (?1, ?2, ?1, '2026-10-01T09:00:00Z', '/tmp/x')";
        db.conn().execute(sql, [label, kind]).unwrap();
    })
}

fn alias(db: &Db, name: &str, to: &str) {
    let alias = Alias {
        name: name.to_string(),
        expands_to: to.to_string(),
        description: None,
    };
    db.save_alias(&alias).unwrap();
}

/// The lines a named run prints under each issue of `category`: its own
/// line, the message's lines after it and the block.
fn listed(shown: &str, category: &str) -> String {
    let mut inside = false;
    let mut out = String::new();
    for line in shown.lines().filter(|l| !l.is_empty()) {
        if !line.starts_with(' ') {
            inside = line.contains(&format!("] {category} (x"));
        }
        if inside {
            out += line;
            out += "\n";
        }
    }
    out
}

#[test]
fn a_named_checker_lists_every_finding() {
    let mut wrong = Vec::new();
    for (checker, categories) in CATEGORIES {
        for category in *categories {
            let plant = match plant(category) {
                Plant::Two(plant) => plant,
                Plant::Cannot(why) => {
                    assert!(!why.is_empty(), "say why nobody can plant two `{category}`");
                    continue;
                }
                Plant::Unknown => {
                    wrong.push(format!(
                        "plant two `{category}` here, or say why nobody can"
                    ));
                    continue;
                }
            };
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("every.db");
            let db = Db::open(path.to_str().unwrap()).unwrap();
            let both = plant(&db);
            let args = CheckArgs {
                checkers: vec![checker.to_string()],
                ..Default::default()
            };
            let (found, _) = report(&args, &db).unwrap();
            let shown = render_human(&found);
            let under = listed(&shown, category);
            let missing: Vec<&String> = both.iter().filter(|one| !under.contains(*one)).collect();
            if under.is_empty() || !missing.is_empty() {
                wrong.push(format!(
                    "`check {checker}` lists no {missing:?} under {category}:\n{shown}"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
